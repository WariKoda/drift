// Real filesystem-backed FTP daemon for Rust/Go protocol parity tests.
// Fault files affect real sockets or final transfer replies, never client mocks.
package main

import (
	"bufio"
	"crypto/tls"
	"fmt"
	"io"
	"net"
	"os"
	"path"
	"strconv"
	"strings"
	"sync"
	"time"
)

type server struct {
	root         *os.Root
	control      string
	limit        int
	mu           sync.Mutex
	active       int
	certificates [2]tls.Certificate
}

func main() {
	root, err := os.OpenRoot(os.Args[1])
	if err != nil {
		panic(err)
	}
	defer root.Close()
	limit, err := strconv.Atoi(os.Args[2])
	if err != nil {
		panic(err)
	}
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}
	defer listener.Close()
	s := &server{root: root, control: os.Args[3], limit: limit}
	if len(os.Args) > 4 {
		if err = s.prepareTLS(os.Args[4]); err != nil {
			panic(err)
		}
	}
	fmt.Println(listener.Addr().(*net.TCPAddr).Port)
	for {
		conn, err := listener.Accept()
		if err != nil {
			return
		}
		go s.serve(conn)
	}
}
func (s *server) flag(name string) bool {
	_, err := os.Stat(path.Join(s.control, name))
	return err == nil
}
func (s *server) matches(name, value string) bool {
	content, err := os.ReadFile(path.Join(s.control, name))
	if err != nil {
		return false
	}
	for _, line := range strings.Split(string(content), "\n") {
		if strings.TrimSpace(line) == value {
			return true
		}
	}
	return false
}
func (s *server) record(command, argument string) {
	if command == "PASS" {
		argument = "<redacted>"
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	f, err := os.OpenFile(path.Join(s.control, "commands"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0600)
	if err == nil {
		_, _ = fmt.Fprintln(f, command, argument)
		_ = f.Close()
	}
}
func local(p string) string {
	p = strings.TrimPrefix(path.Clean("/"+p), "/")
	if p == "" {
		return "."
	}
	return p
}
func (s *server) serve(conn net.Conn) {
	defer conn.Close()
	s.mu.Lock()
	allowed := s.active < s.limit
	if allowed {
		s.active++
	}
	s.mu.Unlock()
	if !allowed {
		_, _ = io.WriteString(conn, "421 too many sessions\r\n")
		s.record("REJECT", "")
		return
	}
	s.record("OPEN", "")
	defer func() {
		s.mu.Lock()
		s.active--
		s.mu.Unlock()
		s.record("CLOSED", "")
	}()
	reader := bufio.NewReader(conn)
	reply := func(code int, message string) error {
		_, err := fmt.Fprintf(conn, "%d %s\r\n", code, message)
		return err
	}
	if reply(220, "drift filesystem FTP test server") != nil {
		return
	}
	cwd, from, logged := "/", "", false
	protected := false
	var passive net.Listener
	defer func() {
		if passive != nil {
			_ = passive.Close()
		}
	}()
	resolve := func(arg string) string {
		if strings.HasPrefix(arg, "/") {
			return path.Clean(arg)
		}
		return path.Join(cwd, arg)
	}
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			return
		}
		command, argument, _ := strings.Cut(strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r"), " ")
		command = strings.ToUpper(command)
		s.record(command, argument)
		p := resolve(argument)
		if s.matches("drop", command) {
			return
		}
		if s.matches("deny", command+" "+p) {
			if reply(550, "permission denied") != nil {
				return
			}
			continue
		}
		if !logged && command != "USER" && command != "PASS" && command != "QUIT" && command != "AUTH" && command != "PBSZ" && command != "PROT" {
			if reply(530, "login first") != nil {
				return
			}
			continue
		}
		switch command {
		case "AUTH":
			if s.certificates[0].PrivateKey == nil || argument != "TLS" {
				err = reply(502, "TLS unavailable")
				break
			}
			if reply(234, "start TLS") != nil {
				return
			}
			conn, err = s.secure(conn, false)
			if err != nil {
				return
			}
			reader = bufio.NewReader(conn)
		case "PBSZ":
			err = reply(200, "ok")
		case "PROT":
			protected = argument == "P"
			err = reply(200, "ok")
		case "USER":
			err = reply(331, "password required")
		case "PASS":
			if argument != "test-password" {
				err = reply(530, "invalid credentials")
			} else {
				logged = true
				err = reply(230, "logged in")
			}
		case "FEAT":
			_, err = io.WriteString(conn, "211-Features\r\n MLST type*;size*;modify*;\r\n UTF8\r\n211 End\r\n")
		case "OPTS", "TYPE", "NOOP":
			err = reply(200, "ok")
		case "PWD":
			err = reply(257, strconv.Quote(cwd))
		case "CWD":
			info, e := s.root.Stat(local(p))
			if e != nil || !info.IsDir() {
				err = reply(550, "directory unavailable")
			} else {
				cwd = p
				err = reply(250, "directory changed")
			}
		case "MLST":
			if s.flag("no-mlst") {
				err = reply(502, "MLST unavailable")
				break
			}
			info, e := s.root.Lstat(local(p))
			if e != nil {
				err = reply(550, "path unavailable")
				break
			}
			kind := "file"
			if info.IsDir() {
				kind = "dir"
			}
			if info.Mode()&os.ModeSymlink != 0 {
				kind = "OS.unix=slink"
			}
			_, err = fmt.Fprintf(conn, "250-Listing\r\n type=%s;size=%d;modify=%s; %s\r\n250 End\r\n", kind, info.Size(), info.ModTime().UTC().Format("20060102150405"), p)
		case "SIZE", "MDTM":
			info, e := s.root.Stat(local(p))
			if e != nil || !info.Mode().IsRegular() {
				err = reply(550, "file unavailable")
				break
			}
			if command == "SIZE" {
				err = reply(213, strconv.FormatInt(info.Size(), 10))
			} else {
				err = reply(213, info.ModTime().UTC().Format("20060102150405"))
			}
		case "EPSV", "PASV":
			if command == "EPSV" && s.flag("no-epsv") {
				err = reply(502, "EPSV unavailable")
				break
			}
			if passive != nil {
				_ = passive.Close()
			}
			passive, err = net.Listen("tcp", "127.0.0.1:0")
			if err != nil {
				return
			}
			port := passive.Addr().(*net.TCPAddr).Port
			if command == "EPSV" {
				err = reply(229, fmt.Sprintf("Entering Extended Passive Mode (|||%d|)", port))
			} else {
				err = reply(227, fmt.Sprintf("Entering Passive Mode (127,0,0,1,%d,%d)", port/256, port%256))
			}
		case "LIST", "MLSD", "RETR", "STOR":
			if passive == nil {
				err = reply(425, "use passive mode first")
				break
			}
			var src io.Reader
			var file *os.File
			if command == "LIST" || command == "MLSD" {
				dir, e := s.root.Open(local(p))
				if e != nil {
					err = reply(550, "directory unavailable")
					break
				}
				items, e := dir.ReadDir(-1)
				_ = dir.Close()
				if e != nil {
					err = reply(550, "directory unavailable")
					break
				}
				var lines strings.Builder
				for _, item := range items {
					info, e := item.Info()
					if e != nil {
						return
					}
					mode := "-rw-r--r--"
					if info.IsDir() {
						mode = "drwxr-xr-x"
					}
					if info.Mode()&os.ModeSymlink != 0 {
						mode = "lrwxrwxrwx"
					}
					name := item.Name()
					if item.Type()&os.ModeSymlink != 0 {
						target, e := s.root.Readlink(local(path.Join(p, name)))
						if e != nil {
							return
						}
						name += " -> " + target
					}
					if command == "MLSD" {
						kind := "file"
						if info.IsDir() {
							kind = "dir"
						}
						fmt.Fprintf(&lines, "type=%s;size=%d;modify=%s; %s\r\n", kind, info.Size(), info.ModTime().UTC().Format("20060102150405"), name)
					} else {
						fmt.Fprintf(&lines, "%s 1 drift drift %d %s %s\r\n", mode, info.Size(), info.ModTime().UTC().Format("Jan 02 2006"), name)
					}
				}
				src = strings.NewReader(lines.String())
			} else if command == "RETR" {
				file, err = s.root.Open(local(p))
				if err != nil {
					err = reply(550, "file unavailable")
					break
				}
				src = file
			} else {
				file, err = s.root.OpenFile(local(p), os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0600)
				if err != nil {
					err = reply(550, "file unavailable")
					break
				}
			}
			if command == "LIST" || command == "MLSD" {
				for s.flag("hold-list") {
					time.Sleep(10 * time.Millisecond)
				}
			}
			err = reply(150, "opening data connection")
			if err != nil {
				if file != nil {
					_ = file.Close()
				}
				return
			}
			data, e := passive.Accept()
			_ = passive.Close()
			passive = nil
			if e != nil {
				if file != nil {
					_ = file.Close()
				}
				return
			}
			if protected {
				data, e = s.secure(data, true)
				if e != nil {
					if file != nil {
						_ = file.Close()
					}
					if s.flag("hold-tls-failure") {
						continue
					}
					return
				}
			}
			if command == "STOR" {
				_ = os.WriteFile(path.Join(s.control, "stor-started"), []byte(p), 0600)
				if s.flag("slow-stor") {
					err = slowCopy(file, data)
				} else {
					_, err = io.Copy(file, data)
				}
			} else if command == "RETR" && s.flag("slow-retr") {
				err = slowCopy(data, src)
			} else {
				_, err = io.Copy(data, src)
			}
			_ = data.Close()
			if file != nil {
				closeErr := file.Close()
				if err == nil {
					err = closeErr
				}
			}
			if err != nil {
				return
			}
			if s.matches("fail-completion", command+" "+p) {
				err = reply(426, "data transfer completion failed")
			} else if s.matches("drop-completion", command) {
				return
			} else {
				err = reply(226, "transfer complete")
			}
		case "MKD":
			err = s.root.Mkdir(local(p), 0700)
			if err != nil {
				err = reply(550, "cannot create directory")
			} else {
				err = reply(257, "created")
			}
		case "RNFR":
			_, e := s.root.Lstat(local(p))
			if e != nil {
				err = reply(550, "file unavailable")
			} else {
				from = p
				err = reply(350, "ready to rename")
			}
		case "RNTO":
			if from == "" {
				err = reply(503, "RNFR first")
				break
			}
			if err = s.root.Rename(local(from), local(p)); err != nil {
				err = reply(550, "rename failed")
			} else {
				if s.flag("drop-rename-reply") {
					return
				}
				err = reply(250, "renamed")
			}
			from = ""
		case "DELE":
			if err = s.root.Remove(local(p)); err != nil {
				err = reply(550, "delete failed")
			} else {
				err = reply(250, "deleted")
			}
		case "QUIT":
			_ = reply(221, "goodbye")
			return
		default:
			err = reply(502, "command unavailable")
		}
		if err != nil {
			return
		}
	}
}
func slowCopy(dst io.Writer, src io.Reader) error {
	buf := make([]byte, 32*1024)
	for {
		n, err := src.Read(buf)
		if n > 0 {
			if _, e := dst.Write(buf[:n]); e != nil {
				return e
			}
			time.Sleep(15 * time.Millisecond)
		}
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
	}
}
