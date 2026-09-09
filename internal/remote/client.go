// Package remote defines the protocol-agnostic interface for remote file
// operations and provides a factory that returns the right implementation
// (SFTP or FTP) based on host.Protocol.
package remote

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	driftftp "github.com/WariKoda/drift/internal/ftp"
	"github.com/WariKoda/drift/internal/sftp"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// Client abstracts all remote file operations needed by drift.
// Both *sftp.Client and *ftp.Client satisfy this interface.
//
// No method takes a local path: the local side of a transfer belongs to
// fs.Root, which confines it to the project. A download is Open plus
// fs.Root.WriteAtomic, an upload is fs.Root.Open plus Upload.
type Client interface {
	Stat(path string) (os.FileInfo, error)
	ReadDir(path string) ([]*fs.FileEntry, error)
	Open(path string) (io.ReadCloser, error)
	ReadFile(path string) ([]byte, error)
	Upload(remotePath string, src io.Reader) error
	WalkFiles(root string, fn func(string) error) error
	// WalkFilesWithActivity also reports visited directories, including empty ones.
	// activity must be concurrency-safe and may return an error to stop the walk.
	WalkFilesWithActivity(root string, fn func(string) error, activity func() error) error
	DeleteFile(path string) error
	// Done closes when the connection is closed or its keep-alive monitor fails.
	Done() <-chan struct{}
	// Err returns the terminal monitor error; normal Close leaves it nil.
	Err() error
	Close() error
}

// Connect dials the host using the protocol specified in host.Protocol.
// An empty or "sftp" protocol uses SSH/SFTP; "ftp" uses plain FTP; "ftps" uses FTP over explicit TLS.
func Connect(ctx context.Context, host config.Host, trust *tlstrust.Manager, required *tlstrust.Challenge) (Client, error) {
	switch host.Protocol {
	case "ftp":
		return driftftp.Connect(ctx, host, tlstrust.NewPolicy(nil))
	case "ftps":
		endpoint, err := tlstrust.NormalizeEndpoint(host.Protocol, host.Hostname, host.Port)
		if err != nil {
			return nil, err
		}
		if required != nil && required.Endpoint != endpoint {
			return nil, fmt.Errorf("FTPS retry certificate belongs to %s, not %s", required.Endpoint.Address(), endpoint.Address())
		}
		if required != nil && trust == nil {
			return nil, errors.New("FTPS retry certificate requires a trust manager")
		}
		policy := tlstrust.NewPolicy(nil)
		if trust != nil {
			if required == nil {
				policy, err = trust.Policy()
			} else {
				policy, err = trust.PolicyForRetry(*required)
			}
			if err != nil {
				return nil, fmt.Errorf("load trusted FTPS certificates: %w", err)
			}
		}
		return driftftp.Connect(ctx, host, policy)
	default:
		return sftp.Connect(ctx, host)
	}
}
