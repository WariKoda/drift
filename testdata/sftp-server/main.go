// Real SSH/SFTP server used by Rust protocol and GUI integration tests.
package main

import (
	"bytes"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"

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
	for request := range channels {
		if request.ChannelType() != "session" {
			_ = request.Reject(ssh.UnknownChannelType, "session required")
			continue
		}
		channel, requests, err := request.Accept()
		if err != nil {
			return
		}
		go func() {
			defer channel.Close()
			for request := range requests {
				var payload struct{ Name string }
				if request.Type != "subsystem" || ssh.Unmarshal(request.Payload, &payload) != nil || payload.Name != "sftp" {
					_ = request.Reply(false, nil)
					continue
				}
				_ = request.Reply(true, nil)
				server, err := sftp.NewServer(channel, sftp.WithServerWorkingDirectory(root))
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
