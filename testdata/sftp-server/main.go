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
func serve(conn net.Conn, config *ssh.ServerConfig, root string) {
	defer conn.Close()
	_, channels, requests, err := ssh.NewServerConn(conn, config)
	if err != nil {
		return
	}
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
		go func() {
			defer channel.Close()
			for request := range requests {
				var payload struct{ Name string }
				if request.Type != "subsystem" || ssh.Unmarshal(request.Payload, &payload) != nil || payload.Name != "sftp" {
					_ = request.Reply(false, nil)
					continue
				}
				_ = request.Reply(true, nil)
				stream := &closeFaultChannel{Channel: channel, conn: conn, controlDir: filepath.Dir(root)}
				server, err := sftp.NewServer(stream, sftp.WithServerWorkingDirectory(root))
				if err != nil {
					return
				}
				if err := server.Serve(); err != nil && err != io.EOF {
					fmt.Fprintln(os.Stderr, err)
				}
				_ = server.Close()
				return
			}
		}()
	}
}

// Pass real SFTP frames through unchanged. A test can arm a socket failure at
// CLOSE after it has finished comparison, so EOF succeeds but close cannot be
// acknowledged. This is a real connection loss, not a fabricated SFTP reply.
type closeFaultChannel struct {
	ssh.Channel
	conn       net.Conn
	controlDir string
	pending    []byte
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
