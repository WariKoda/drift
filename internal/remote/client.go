// Package remote defines the protocol-agnostic interface for remote file
// operations and provides a factory that returns the right implementation
// (SFTP or FTP) based on host.Protocol.
package remote

import (
	"context"
	"io"
	"os"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	driftftp "github.com/WariKoda/drift/internal/ftp"
	"github.com/WariKoda/drift/internal/sftp"
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
	DeleteFile(path string) error
	Close() error
}

// Connect dials the host using the protocol specified in host.Protocol.
// An empty or "sftp" protocol uses SSH/SFTP; "ftp" uses plain FTP; "ftps" uses FTP over explicit TLS.
func Connect(ctx context.Context, host config.Host) (Client, error) {
	switch host.Protocol {
	case "ftp", "ftps":
		return driftftp.Connect(ctx, host)
	default:
		return sftp.Connect(ctx, host)
	}
}
