package remoteedit

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"errors"
	"net"
	"os"
	"path/filepath"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/remote"
	pkgsftp "github.com/pkg/sftp"
	gossh "golang.org/x/crypto/ssh"
)

func TestSFTPEditingUsesRealSSHAndPreservesRemoteMode(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	remoteDir := t.TempDir()
	_, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := gossh.NewSignerFromKey(key)
	if err != nil {
		t.Fatal(err)
	}
	cfg := &gossh.ServerConfig{PasswordCallback: func(gossh.ConnMetadata, []byte) (*gossh.Permissions, error) { return nil, nil }}
	cfg.AddHostKey(signer)
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	accepted := make(chan net.Conn, 1)
	done := make(chan struct{})
	go func() {
		defer close(done)
		socket, err := listener.Accept()
		accepted <- socket
		if err != nil {
			return
		}
		defer socket.Close()
		conn, channels, requests, err := gossh.NewServerConn(socket, cfg)
		if err != nil {
			return
		}
		defer conn.Close()
		go gossh.DiscardRequests(requests)
		for channel := range channels {
			if channel.ChannelType() != "session" {
				_ = channel.Reject(gossh.UnknownChannelType, "session required")
				continue
			}
			stream, requests, err := channel.Accept()
			if err != nil {
				continue
			}
			for request := range requests {
				var subsystem struct{ Name string }
				ok := request.Type == "subsystem" && gossh.Unmarshal(request.Payload, &subsystem) == nil && subsystem.Name == "sftp"
				if err := request.Reply(ok, nil); err != nil {
					break
				}
				if !ok {
					continue
				}
				server, err := pkgsftp.NewServer(stream, pkgsftp.WithServerWorkingDirectory(remoteDir))
				if err == nil {
					_ = server.Serve()
					_ = server.Close()
				}
				break
			}
			_ = stream.Close()
		}
	}()
	t.Cleanup(func() {
		_ = listener.Close()
		if socket := <-accepted; socket != nil {
			_ = socket.Close()
		}
		<-done
	})
	zero := 0
	host := config.Host{Name: "ssh", Hostname: "127.0.0.1", Port: listener.Addr().(*net.TCPAddr).Port,
		User: "test", RootPath: remoteDir, Auth: config.Auth{Type: "password", Password: "test"}, KeepAliveInterval: &zero}
	conn, err := remote.Connect(context.Background(), host, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = conn.Close() })
	target := filepath.Join(remoteDir, "file.txt")
	if err := os.WriteFile(target, []byte("old"), 0750); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(target, 0750); err != nil {
		t.Fatal(err)
	}
	s := New(Request{Host: host, RemotePath: target, ProjectRoot: t.TempDir()})
	t.Cleanup(func() { _ = s.Close(true) })
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.file, []byte("new"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil {
		t.Fatal(result.Err)
	}
	if result := s.Save(context.Background(), conn, nil); result.Err != nil || !result.Saved {
		t.Fatalf("save: %+v", result)
	}
	info, err := os.Stat(target)
	if err != nil || info.Mode().Perm() != 0750 {
		t.Fatalf("remote mode: %v, %v", info, err)
	}
	link := filepath.Join(remoteDir, "link.txt")
	if err := os.Symlink(target, link); err != nil {
		t.Fatal(err)
	}
	linked := New(Request{Host: host, RemotePath: link, ProjectRoot: s.request.ProjectRoot})
	t.Cleanup(func() { _ = linked.Close(true) })
	if result := linked.Prepare(context.Background(), conn); !errors.Is(result.Err, ErrConflict) {
		t.Fatalf("remote symlink accepted: %+v", result)
	}
}
