package sftp

import (
	"errors"
	"io"
	"net"
	"os"
	"strings"
	"testing"
	"time"

	pkgsftp "github.com/pkg/sftp"
)

// The SFTP library keeps only the last element of a listed name. A server that
// sends "x/.." therefore produces the entry "..", which the walker would join
// onto the directory and follow into its parent.
func TestListingNamesMustBeSinglePathComponents(t *testing.T) {
	for _, hostile := range []string{"x/..", "x/.", "/"} {
		t.Run(hostile, func(t *testing.T) {
			client := newHostileListingClient(t, hostile)

			if _, err := client.ReadDir("/site/uploads"); err == nil ||
				!strings.Contains(err.Error(), "not a single path component") {
				t.Fatalf("ReadDir error = %v, want the listing rejected", err)
			}

			done := make(chan error, 1)
			var walked []string
			go func() {
				done <- client.WalkFiles("/site/uploads", func(p string) error {
					walked = append(walked, p)
					return nil
				})
			}()
			select {
			case err := <-done:
				if err == nil || !strings.Contains(err.Error(), "not a single path component") {
					t.Fatalf("WalkFiles error = %v, want the listing rejected", err)
				}
			case <-time.After(5 * time.Second):
				t.Fatal("walk did not stop at the hostile entry")
			}
			for _, p := range walked {
				if !strings.HasPrefix(p, "/site/uploads/") {
					t.Fatalf("walk reported %q outside the listed directory", p)
				}
			}
		})
	}
}

// newHostileListingClient talks to a server whose every directory lists one
// regular file and one directory with the given name.
func newHostileListingClient(t *testing.T, hostileName string) *Client {
	t.Helper()
	serverConn, clientConn := net.Pipe()
	handler := hostileLister{name: hostileName}
	server := pkgsftp.NewRequestServer(serverConn, pkgsftp.Handlers{
		FileGet: handler, FilePut: handler, FileCmd: handler, FileList: handler,
	})
	serveDone := make(chan error, 1)
	go func() { serveDone <- server.Serve() }()

	protocolClient, err := pkgsftp.NewClientPipe(clientConn, clientConn)
	if err != nil {
		serverConn.Close()
		clientConn.Close()
		t.Fatal(err)
	}
	t.Cleanup(func() {
		_ = protocolClient.Close()
		_ = clientConn.Close()
		_ = serverConn.Close()
		<-serveDone
	})
	return &Client{sftp: protocolClient}
}

type hostileLister struct{ name string }

func (h hostileLister) Fileread(*pkgsftp.Request) (io.ReaderAt, error) {
	return nil, errors.New("not served")
}

func (h hostileLister) Filewrite(*pkgsftp.Request) (io.WriterAt, error) {
	return nil, errors.New("not served")
}

func (h hostileLister) Filecmd(*pkgsftp.Request) error { return errors.New("not served") }

func (h hostileLister) Filelist(r *pkgsftp.Request) (pkgsftp.ListerAt, error) {
	if r.Method == "List" {
		return listedEntries{
			listedEntry{name: "ok.txt"},
			listedEntry{name: h.name, dir: true},
		}, nil
	}
	return listedEntries{listedEntry{name: r.Filepath, dir: true}}, nil
}

type listedEntries []os.FileInfo

func (l listedEntries) ListAt(out []os.FileInfo, offset int64) (int, error) {
	if offset >= int64(len(l)) {
		return 0, io.EOF
	}
	n := copy(out, l[offset:])
	if n < len(out) {
		return n, io.EOF
	}
	return n, nil
}

type listedEntry struct {
	name string
	dir  bool
}

func (e listedEntry) Name() string       { return e.name }
func (e listedEntry) Size() int64        { return 1 }
func (e listedEntry) ModTime() time.Time { return time.Unix(0, 0) }
func (e listedEntry) IsDir() bool        { return e.dir }
func (e listedEntry) Sys() any           { return nil }
func (e listedEntry) Mode() os.FileMode {
	if e.dir {
		return os.ModeDir | 0o755
	}
	return 0o644
}
