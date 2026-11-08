package app

import (
	"context"
	"errors"
	"fmt"
	"path"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// BrowseResult transfers ownership of Conn to the caller on success.
type BrowseResult struct {
	Conn    remote.Client
	Root    string
	Entries []*fs.FileEntry
}

// BrowseRemote connects and lists the host root. Failure closes the connection;
// cancellation must interrupt a stalled directory listing, not just the dial.
func BrowseRemote(ctx context.Context, host config.Host, trust *tlstrust.Manager, required *tlstrust.Challenge) (BrowseResult, error) {
	if err := ctx.Err(); err != nil {
		return BrowseResult{}, fmt.Errorf("connect to %s: %w", host.Hostname, err)
	}
	root := "/"
	if host.RootPath != "" {
		root = path.Clean(host.RootPath)
	}
	conn, err := remote.Connect(ctx, host, trust, required)
	if err != nil {
		err = errors.Join(ctx.Err(), err)
		log.Error("remote browser connect failed", "host", host.Name, "hostname", host.Hostname, "err", err)
		return BrowseResult{}, fmt.Errorf("connect to %s: %w", host.Hostname, err)
	}
	detach := remote.CloseOnContextDone(ctx, conn)
	entries, readErr := conn.ReadDir(root)
	detach()
	err = ctx.Err()
	if err == nil {
		err = readErr
	}
	if err == nil {
		err = conn.Err()
	}
	if err != nil {
		log.Error("remote browser root read failed", "host", host.Name, "remote", root, "err", err)
		return BrowseResult{}, fmt.Errorf("read %s: %w", root, errors.Join(err, conn.Close()))
	}
	return BrowseResult{Conn: conn, Root: root, Entries: entries}, nil
}

// TestHost verifies a connection and root listing without retaining a transport.
func TestHost(ctx context.Context, host config.Host, trust *tlstrust.Manager, required *tlstrust.Challenge) error {
	result, err := BrowseRemote(ctx, host, trust, required)
	if err != nil {
		return err
	}
	closeErr := result.Conn.Close()
	if err := errors.Join(closeErr, result.Conn.Err(), ctx.Err()); err != nil {
		log.Error("host connection test failed", "host", host.Name, "err", err)
		return fmt.Errorf("close connection: %w", err)
	}
	return nil
}
