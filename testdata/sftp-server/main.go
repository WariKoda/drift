// Real SSH/SFTP server used by Rust protocol and GUI integration tests.
package main

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/pkg/sftp"
	"golang.org/x/crypto/ssh"
)

func main() {
	keyData, err := os.ReadFile(os.Args[1])
	if err != nil {
		panic(err)
	}
	hostKey, err := ssh.ParsePrivateKey(keyData)
	if err != nil {
		panic(err)
	}
	publicData, err := os.ReadFile(os.Args[2])
	if err != nil {
		panic(err)
	}
	publicKey, _, _, _, err := ssh.ParseAuthorizedKey(publicData)
	if err != nil {
		panic(err)
	}
	root := os.Args[3]
	config := &ssh.ServerConfig{
		PasswordCallback: func(c ssh.ConnMetadata, password []byte) (*ssh.Permissions, error) {
			if c.User() == "testuser" && string(password) == "test-password" {
				return nil, nil
			}
			return nil, fmt.Errorf("authentication rejected")
		},
		PublicKeyCallback: func(c ssh.ConnMetadata, key ssh.PublicKey) (*ssh.Permissions, error) {
			if c.User() == "testuser" && bytes.Equal(publicKey.Marshal(), key.Marshal()) {
				return nil, nil
			}
			return nil, fmt.Errorf("authentication rejected")
		},
	}
	config.AddHostKey(hostKey)
	listener, err := net.Listen("tcp4", "127.0.0.1:"+os.Args[4])
	if err != nil {
		panic(err)
	}
	fmt.Println(listener.Addr().(*net.TCPAddr).Port)
	for {
		conn, err := listener.Accept()
		if err != nil {
			return
		}
		go serve(conn, config, root)
	}
}

// Extension policy is selected before the first subsystem starts and is then
// immutable for this daemon. Set policy markers before connecting.
var extensionPolicy sync.Once

func serve(conn net.Conn, config *ssh.ServerConfig, root string) {
	defer conn.Close()
	_, channels, requests, err := ssh.NewServerConn(conn, config)
	if err != nil {
		return
	}
	defer func() {
		if marker(filepath.Dir(root), "record-transport-close") {
			record(filepath.Dir(root), "ssh-connections", "closed\n")
		}
	}()
	go func() {
		for request := range requests {
			if request.Type == "keepalive@openssh.com" {
				file, err := os.OpenFile(filepath.Join(filepath.Dir(root), "probes"), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0600)
				if err == nil {
					_, _ = file.WriteString("probe\n")
					_ = file.Close()
				}
				if os.Getenv("DRIFT_TEST_DROP_PROBES") == "true" {
					continue
				}
			}
			_ = request.Reply(request.Type == "keepalive@openssh.com", nil)
		}
	}()
	opened := 0
	for request := range channels {
		if _, limited := os.Stat(filepath.Join(filepath.Dir(root), "one-channel")); limited == nil && opened > 0 {
			record(filepath.Dir(root), "channels", "rejected\n")
			_ = request.Reject(ssh.ResourceShortage, "only one session channel allowed")
			continue
		}
		if request.ChannelType() != "session" {
			_ = request.Reject(ssh.UnknownChannelType, "session required")
			continue
		}
		channel, requests, err := request.Accept()
		if err != nil {
			return
		}
		opened++
		record(filepath.Dir(root), "channels", "accepted\n")
		go func() {
			defer channel.Close()
			for request := range requests {
				var payload struct{ Name string }
				if request.Type != "subsystem" || ssh.Unmarshal(request.Payload, &payload) != nil || payload.Name != "sftp" {
					_ = request.Reply(false, nil)
					continue
				}
				_ = request.Reply(true, nil)
				controlDir := filepath.Dir(root)
				diskPolicy := marker(controlDir, "restricted-rename") || marker(controlDir, "no-posix") || marker(controlDir, "posix-unsupported") || marker(controlDir, "deny-posix-rename")
				extensionPolicy.Do(func() {
					if diskPolicy {
						var extensions []string
						if !marker(controlDir, "no-posix") {
							extensions = append(extensions, "posix-rename@openssh.com")
						}
						if err := sftp.SetSFTPExtensions(extensions...); err != nil {
							panic(err)
						}
					}
				})
				stream := &closeFaultChannel{Channel: channel, conn: conn, controlDir: controlDir, renames: make(map[uint32]string)}
				if diskPolicy {
					files, err := os.OpenRoot(root)
					if err != nil {
						fmt.Fprintln(os.Stderr, err)
						return
					}
					defer files.Close()
					fs := &diskHandler{root: files, base: root, controlDir: controlDir}
					server := sftp.NewRequestServer(stream, sftp.Handlers{FileGet: fs, FilePut: fs, FileCmd: fs, FileList: fs}, sftp.WithStartDirectory(root))
					if err := server.Serve(); err != nil && err != io.EOF {
						fmt.Fprintln(os.Stderr, err)
					}
					_ = server.Close()
					if marker(controlDir, "record-transport-close") {
						record(controlDir, "sftp-resources", "released\n")
					}
					return
				}
				server, err := sftp.NewServer(stream, sftp.WithServerWorkingDirectory(root))
				if err != nil {
					return
				}
				if err := server.Serve(); err != nil && err != io.EOF {
					fmt.Fprintln(os.Stderr, err)
				}
				_ = server.Close()
				if marker(controlDir, "record-transport-close") {
					record(controlDir, "sftp-resources", "released\n")
				}
				return
			}
		}()
	}
}

// Pass real SFTP frames through unchanged unless a test marker arms a fault.
// A test can arm a socket failure at CLOSE after comparison, so EOF succeeds but
// close cannot be acknowledged. READ markers inject deliberate test-only bugs into
// actual DATA replies after pkg/sftp reads the real file. IDs and framing remain
// valid, and EOF STATUS is never rewritten. Neither READ marker closes the socket
// or interferes with CLOSE or SSH keepalives.
type readFault struct {
	requested uint32
	policy    string
}

type closeFaultChannel struct {
	ssh.Channel
	conn       net.Conn
	controlDir string
	pending    []byte
	outgoing   []byte
	mu         sync.Mutex
	renames    map[uint32]string
	reads      map[uint32]readFault
	closes     map[uint32]bool
}

func (c *closeFaultChannel) Read(p []byte) (int, error) {
	n, err := c.Channel.Read(p)
	c.pending = append(c.pending, p[:n]...)
	for len(c.pending) >= 4 {
		length := int(binary.BigEndian.Uint32(c.pending[:4]))
		if length < 1 || length > 1024*1024 {
			return 0, fmt.Errorf("invalid SFTP frame length")
		}
		if len(c.pending) < 4+length {
			break
		}
		frame := c.pending[4 : 4+length]
		if len(frame) >= 5 {
			kind := ""
			paths := frame[5:]
			switch frame[0] {
			case 18: // SSH_FXP_RENAME
				kind = "standard"
			case 200: // SSH_FXP_EXTENDED
				name, rest, ok := sshString(paths)
				if ok && name == "posix-rename@openssh.com" {
					kind, paths = "posix", rest
				}
			case 3, 13, 5, 6, 4: // OPEN, REMOVE, READ, WRITE, CLOSE
				names := map[byte]string{3: "open", 13: "remove", 5: "read", 6: "write", 4: "close"}
				path, _, _ := sshString(paths)
				if frame[0] == 5 || frame[0] == 6 || frame[0] == 4 {
					path = "" // Handles are opaque, not filesystem paths.
				}
				record(c.controlDir, "sftp-operations", names[frame[0]]+"\t"+path+"\n")
				if frame[0] == 5 {
					_, rest, ok := sshString(paths)
					policy := ""
					if marker(c.controlDir, "arm-empty-read") {
						policy = "empty"
					} else if marker(c.controlDir, "arm-oversized-read") {
						policy = "oversized"
					}
					if ok && len(rest) == 12 && policy != "" {
						c.mu.Lock()
						if c.reads == nil {
							c.reads = make(map[uint32]readFault)
						}
						c.reads[binary.BigEndian.Uint32(frame[1:5])] = readFault{
							requested: binary.BigEndian.Uint32(rest[8:12]), policy: policy,
						}
						c.mu.Unlock()
					}
				}
				if frame[0] == 4 && marker(c.controlDir, "record-close-status") {
					c.mu.Lock()
					if c.closes == nil {
						c.closes = make(map[uint32]bool)
					}
					c.closes[binary.BigEndian.Uint32(frame[1:5])] = true
					c.mu.Unlock()
				}
			}
			if kind != "" {
				source, rest, ok := sshString(paths)
				target, _, okTarget := sshString(rest)
				if !ok || !okTarget {
					return 0, fmt.Errorf("invalid rename paths")
				}
				record(c.controlDir, "sftp-operations", kind+"\t"+source+"\t"+target+"\n")
				if marker(c.controlDir, "arm-rename-before-"+kind) {
					record(c.controlDir, "sftp-operations", "before-commit\t"+kind+"\n")
					_ = c.conn.Close()
					return 0, io.EOF
				}
				c.mu.Lock()
				c.renames[binary.BigEndian.Uint32(frame[1:5])] = kind
				c.mu.Unlock()
			}
		}
		if c.pending[4] == 6 { // SSH_FXP_WRITE: throttle real uploads for cancellation tests.
			if _, armed := os.Stat(filepath.Join(c.controlDir, "slow-data")); armed == nil {
				_ = os.WriteFile(filepath.Join(c.controlDir, "write-started"), []byte("write\n"), 0600)
				time.Sleep(20 * time.Millisecond)
			}
		}
		if c.pending[4] == 4 { // SSH_FXP_CLOSE
			if _, armed := os.Stat(filepath.Join(c.controlDir, "drop-on-close")); armed == nil {
				_ = os.WriteFile(filepath.Join(c.controlDir, "close-dropped"), []byte("socket closed\n"), 0600)
				_ = c.conn.Close()
				return 0, io.EOF
			}
		}
		c.pending = c.pending[4+length:]
	}
	return n, err
}

// Buffer only to observe complete real replies. A successful STATUS is produced
// by pkg/sftp after the filesystem rename; dropping it makes the outcome unknown
// to the client without changing that reply or filesystem operation. The READ
// corruption policy changes only real DATA responses, never filesystem I/O.
func (c *closeFaultChannel) Write(p []byte) (int, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.outgoing = append(c.outgoing, p...)
	for len(c.outgoing) >= 4 {
		length := int(binary.BigEndian.Uint32(c.outgoing[:4]))
		if length < 1 || length > 1024*1024 {
			return 0, fmt.Errorf("invalid SFTP response length")
		}
		if len(c.outgoing) < 4+length {
			break
		}
		frame := c.outgoing[4 : 4+length]
		response := c.outgoing[:4+length]
		if len(frame) >= 5 {
			id := binary.BigEndian.Uint32(frame[1:5])
			if fault, ok := c.reads[id]; ok {
				delete(c.reads, id)
				if len(frame) >= 9 && frame[0] == 103 { // SSH_FXP_DATA, not EOF STATUS
					native := frame[9:]
					if int(binary.BigEndian.Uint32(frame[5:9])) != len(native) || len(native) == 0 {
						return 0, fmt.Errorf("invalid native DATA response")
					}
					var data []byte
					if fault.policy == "oversized" {
						if fault.requested > 32*1024 {
							return 0, fmt.Errorf("READ corruption requires a bounded request")
						}
						data = make([]byte, int(fault.requested)+1)
						copy(data, native)
					}
					response = make([]byte, 13+len(data))
					binary.BigEndian.PutUint32(response[:4], uint32(len(response)-4))
					copy(response[4:9], frame[:5]) // Keep the actual response type and ID.
					binary.BigEndian.PutUint32(response[9:13], uint32(len(data)))
					copy(response[13:], data)
					record(c.controlDir, "sftp-operations", fmt.Sprintf("fault-data\t%s\t%d\t%d\t%d\t%d\n", fault.policy, id, fault.requested, len(native), len(data)))
				}
			}
		}
		if len(frame) >= 9 && frame[0] == 101 { // SSH_FXP_STATUS
			id := binary.BigEndian.Uint32(frame[1:5])
			if c.closes[id] {
				delete(c.closes, id)
				record(c.controlDir, "sftp-operations", fmt.Sprintf("close-status\t%d\n", binary.BigEndian.Uint32(frame[5:9])))
			}
			if kind, ok := c.renames[id]; ok {
				delete(c.renames, id)
				code := binary.BigEndian.Uint32(frame[5:9])
				record(c.controlDir, "sftp-operations", fmt.Sprintf("rename-status\t%s\t%d\n", kind, code))
				if code == 0 && marker(c.controlDir, "arm-rename-close-"+kind) {
					record(c.controlDir, "sftp-operations", "ack-dropped\t"+kind+"\n")
					_ = c.conn.Close()
					return 0, io.ErrClosedPipe
				}
			}
		}
		if _, err := c.Channel.Write(response); err != nil {
			return 0, err
		}
		c.outgoing = c.outgoing[4+length:]
	}
	return len(p), nil
}

func sshString(data []byte) (string, []byte, bool) {
	if len(data) < 4 {
		return "", nil, false
	}
	length := int(binary.BigEndian.Uint32(data[:4]))
	if length > len(data)-4 {
		return "", nil, false
	}
	return string(data[4 : 4+length]), data[4+length:], true
}

func marker(dir, name string) bool {
	_, err := os.Stat(filepath.Join(dir, name))
	return err == nil
}

func record(dir, name, line string) {
	file, err := os.OpenFile(filepath.Join(dir, name), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0600)
	if err != nil {
		panic(err)
	}
	if _, err := file.WriteString(line); err != nil {
		panic(err)
	}
	if err := file.Close(); err != nil {
		panic(err)
	}
}

// This optional test-only filesystem policy deliberately implements SFTP v3
// no-replace Rename separately from POSIX replacement. All I/O uses real files
// through os.Root; the ordinary fixture keeps its original NewServer behavior.
type diskHandler struct {
	root       *os.Root
	base       string
	controlDir string
	renameMu   sync.Mutex
}

func (f *diskHandler) relative(path string) (string, error) {
	if !filepath.IsAbs(path) {
		path = filepath.Join(f.base, path)
	}
	rel, err := filepath.Rel(f.base, path)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(os.PathSeparator)) {
		return "", os.ErrPermission
	}
	return rel, nil
}

func (f *diskHandler) Fileread(r *sftp.Request) (io.ReaderAt, error) {
	path, err := f.relative(r.Filepath)
	if err != nil {
		return nil, err
	}
	return f.root.Open(path)
}

func (f *diskHandler) Filewrite(r *sftp.Request) (io.WriterAt, error) {
	return f.OpenFile(r)
}

func (f *diskHandler) OpenFile(r *sftp.Request) (sftp.WriterAtReaderAt, error) {
	path, err := f.relative(r.Filepath)
	if err != nil {
		return nil, err
	}
	p := r.Pflags()
	flags := os.O_RDONLY
	if p.Write {
		flags = os.O_WRONLY
		if p.Read {
			flags = os.O_RDWR
		}
	}
	if p.Creat {
		flags |= os.O_CREATE
	}
	if p.Trunc {
		flags |= os.O_TRUNC
	}
	if p.Excl {
		flags |= os.O_EXCL
	}
	return f.root.OpenFile(path, flags, 0666)
}

func (f *diskHandler) Filecmd(r *sftp.Request) error {
	path, err := f.relative(r.Filepath)
	if err != nil {
		return err
	}
	switch r.Method {
	case "Rename":
		return f.rename(r, false)
	case "Remove", "Rmdir":
		return f.root.Remove(path)
	case "Mkdir":
		return f.root.Mkdir(path, 0777)
	case "Setstat":
		flags, attrs := r.AttrFlags(), r.Attributes()
		if flags.Size {
			file, err := f.root.OpenFile(path, os.O_WRONLY, 0)
			if err != nil {
				return err
			}
			truncated := file.Truncate(int64(attrs.Size))
			closed := file.Close()
			if truncated != nil {
				return truncated
			}
			if closed != nil {
				return closed
			}
		}
		if flags.Permissions {
			if err := f.root.Chmod(path, attrs.FileMode()); err != nil {
				return err
			}
		}
		if flags.UidGid {
			if err := f.root.Chown(path, int(attrs.UID), int(attrs.GID)); err != nil {
				return err
			}
		}
		if flags.Acmodtime {
			return f.root.Chtimes(path, time.Unix(int64(attrs.Atime), 0), time.Unix(int64(attrs.Mtime), 0))
		}
		return nil
	default:
		return sftp.ErrSSHFxOpUnsupported
	}
}

func (f *diskHandler) PosixRename(r *sftp.Request) error {
	if marker(f.controlDir, "no-posix") || marker(f.controlDir, "posix-unsupported") {
		return sftp.ErrSSHFxOpUnsupported
	}
	if marker(f.controlDir, "deny-posix-rename") {
		return sftp.ErrSSHFxPermissionDenied
	}
	return f.rename(r, true)
}

func (f *diskHandler) rename(r *sftp.Request, replace bool) error {
	f.renameMu.Lock()
	defer f.renameMu.Unlock()
	source, err := f.relative(r.Filepath)
	if err != nil {
		return err
	}
	target, err := f.relative(r.Target)
	if err != nil {
		return err
	}
	if !replace && marker(f.controlDir, "restricted-rename") {
		if _, err := f.root.Lstat(target); err == nil {
			return os.ErrExist
		} else if !os.IsNotExist(err) {
			return err
		}
	}
	return f.root.Rename(source, target)
}

type diskList []os.FileInfo

func (l diskList) ListAt(dst []os.FileInfo, offset int64) (int, error) {
	if offset < 0 || offset >= int64(len(l)) {
		return 0, io.EOF
	}
	n := copy(dst, l[offset:])
	if int(offset)+n == len(l) {
		return n, io.EOF
	}
	return n, nil
}

func (f *diskHandler) Filelist(r *sftp.Request) (sftp.ListerAt, error) {
	path, err := f.relative(r.Filepath)
	if err != nil {
		// Upload verifies absolute parent directories, including ancestors of
		// the fixture root. Only their directory metadata is exposed.
		if r.Method == "Stat" && (filepath.Clean(r.Filepath) == string(os.PathSeparator) || strings.HasPrefix(f.base, filepath.Clean(r.Filepath)+string(os.PathSeparator))) {
			info, err := os.Stat(r.Filepath)
			return diskList{info}, err
		}
		return nil, err
	}
	if r.Method == "List" {
		dir, err := f.root.Open(path)
		if err != nil {
			return nil, err
		}
		defer dir.Close()
		infos, err := dir.Readdir(-1)
		return diskList(infos), err
	}
	info, err := f.root.Stat(path)
	return diskList{info}, err
}

func (f *diskHandler) Lstat(r *sftp.Request) (sftp.ListerAt, error) {
	path, err := f.relative(r.Filepath)
	if err != nil {
		return nil, err
	}
	info, err := f.root.Lstat(path)
	return diskList{info}, err
}

func (f *diskHandler) Readlink(path string) (string, error) {
	rel, err := f.relative(path)
	if err != nil {
		return "", err
	}
	return f.root.Readlink(rel)
}
