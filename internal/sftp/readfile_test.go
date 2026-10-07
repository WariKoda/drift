package sftp

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"

	pkgsftp "github.com/pkg/sftp"
)

func TestReadFileReadAndCloseErrors(t *testing.T) {
	for _, tc := range []struct {
		name        string
		remotePath  string
		rejectClose bool
		wantErrors  int
		missing     bool
	}{
		{name: "success", remotePath: "file"},
		{name: "read failure", remotePath: "dir", wantErrors: 1},
		{name: "close failure", remotePath: "file", rejectClose: true, wantErrors: 1},
		{name: "read and close failure", remotePath: "dir", rejectClose: true, wantErrors: 2},
		{name: "open failure", remotePath: "missing", missing: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			remoteDir := t.TempDir()
			if err := os.WriteFile(filepath.Join(remoteDir, "file"), []byte("payload"), 0o600); err != nil {
				t.Fatal(err)
			}
			if err := os.Mkdir(filepath.Join(remoteDir, "dir"), 0o700); err != nil {
				t.Fatal(err)
			}
			serverConn, clientConn := net.Pipe()
			t.Cleanup(func() {
				_ = clientConn.Close()
				_ = serverConn.Close()
			})
			if err := clientConn.SetDeadline(time.Now().Add(5 * time.Second)); err != nil {
				t.Fatal(err)
			}
			wire := &closeFaultConn{Conn: serverConn, rejectClose: tc.rejectClose}
			server, err := pkgsftp.NewServer(wire, pkgsftp.WithServerWorkingDirectory(remoteDir))
			if err != nil {
				t.Fatal(err)
			}
			serveDone := make(chan error, 1)
			go func() { serveDone <- server.Serve() }()
			t.Cleanup(func() {
				_ = clientConn.Close()
				_ = serverConn.Close()
				select {
				case err := <-serveDone:
					if err != nil && !errors.Is(err, net.ErrClosed) {
						t.Errorf("SFTP server: %v", err)
					}
				case <-time.After(5 * time.Second):
					t.Error("SFTP server did not stop")
				}
			})
			protocolClient, err := pkgsftp.NewClientPipe(clientConn, clientConn)
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = protocolClient.Close() })
			client := &Client{sftp: protocolClient}

			data, err := client.ReadFile(tc.remotePath)
			if tc.missing {
				if !errors.Is(err, os.ErrNotExist) || wire.closes.Load() != 0 {
					t.Fatalf("open failure = %v, close requests = %d", err, wire.closes.Load())
				}
				return
			}
			if wire.closes.Load() != 1 {
				t.Fatalf("close requests = %d, want 1 even after a read failure", wire.closes.Load())
			}
			if tc.remotePath == "file" && string(data) != "payload" {
				t.Fatalf("ReadFile data = %q, want payload", data)
			}
			if tc.wantErrors == 0 {
				if err != nil {
					t.Fatalf("ReadFile: %v", err)
				}
				return
			}
			joined, ok := err.(interface{ Unwrap() []error })
			if !ok || len(joined.Unwrap()) != tc.wantErrors {
				t.Fatalf("ReadFile error = %v, want %d joined failures", err, tc.wantErrors)
			}
			for _, cause := range joined.Unwrap() {
				var status *pkgsftp.StatusError
				if !errors.As(cause, &status) || status.FxCode() != pkgsftp.ErrSSHFxFailure {
					t.Fatalf("lost SFTP failure status: %v", cause)
				}
			}
		})
	}
}

// Fault injection changes only CLOSE's handle on the wire. The real SFTP
// filesystem server still handles every request, returning EBADF for the
// invalid handle instead of substituting a mocked client or file.
type closeFaultConn struct {
	net.Conn
	rejectClose bool
	pending     []byte
	closes      atomic.Int32
}

func (c *closeFaultConn) Read(p []byte) (int, error) {
	if len(p) == 0 {
		return 0, nil
	}
	if len(c.pending) == 0 {
		var header [4]byte
		if _, err := io.ReadFull(c.Conn, header[:]); err != nil {
			return 0, err
		}
		length := binary.BigEndian.Uint32(header[:])
		if length == 0 || length > 1<<20 {
			return 0, fmt.Errorf("invalid SFTP packet length %d", length)
		}
		c.pending = make([]byte, 4+int(length))
		copy(c.pending, header[:])
		if _, err := io.ReadFull(c.Conn, c.pending[4:]); err != nil {
			return 0, err
		}
		const sshFXPClose = 4
		if c.pending[4] == sshFXPClose {
			c.closes.Add(1)
			if c.rejectClose && len(c.pending) > 13 {
				// CLOSE carries type, request ID, then a length-prefixed handle.
				clear(c.pending[13:])
			}
		}
	}
	n := copy(p, c.pending)
	c.pending = c.pending[n:]
	return n, nil
}
