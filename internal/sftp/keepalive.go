package sftp

import (
	"errors"
	"fmt"
	"io"
	"net"
	"time"

	"github.com/WariKoda/drift/internal/log"
)

// Done closes once the connection has terminated and its monitor and resources
// have been released. All observers see the same terminal Err.
func (c *Client) Done() <-chan struct{} { return c.done }

// Err returns the first terminal connection error, or nil after normal Close.
func (c *Client) Err() error {
	c.errMu.RLock()
	defer c.errMu.RUnlock()
	return c.err
}

// terminate selects the terminal result once and interrupts network I/O without
// acquiring SFTP's write lock. Cleanup belongs to monitor, never to a probe.
func (c *Client) terminate(err error) {
	c.stopOnce.Do(func() {
		c.errMu.Lock()
		c.err = err
		c.errMu.Unlock()
		close(c.stop)
		c.closeErr = c.transport.Close()
		if errors.Is(c.closeErr, net.ErrClosed) {
			c.closeErr = nil
		}
		if err != nil {
			log.Error("remote connection failed", "protocol", "sftp",
				"host", c.Host.Name, "endpoint", c.transport.RemoteAddr().String(), "err", err)
		}
	})
}

// monitor owns cleanup independently of the context used to establish SSH.
// SSH probes establish peer reachability, not SFTP subsystem health.
func (c *Client) monitor(interval, timeout time.Duration) {
	sshDone := make(chan struct{})
	var sshErr error
	go func() {
		sshErr = c.sshConn.Wait()
		close(sshDone)
	}()
	defer func() {
		c.terminate(nil)
		<-sshDone
		// The raw transport is already closed, so SFTP Close cannot get stuck
		// behind a channel write. Its EOF is expected during teardown.
		_ = c.sftp.Close()
		if c.authClose != nil {
			c.closeErr = errors.Join(c.closeErr, c.authClose.Close())
		}
		close(c.done)
	}()

	var ticks <-chan time.Time
	if interval > 0 {
		ticker := time.NewTicker(interval)
		defer ticker.Stop()
		ticks = ticker.C
	}
	for {
		select {
		case <-c.stop:
			return
		case <-sshDone:
			if sshErr == nil {
				sshErr = io.EOF
			}
			c.terminate(fmt.Errorf("SSH connection lost: %w", sshErr))
			return
		case <-ticks:
			if err := c.probe(timeout); err != nil {
				c.terminate(err)
				return
			}
		}
	}
}

func (c *Client) probe(timeout time.Duration) error {
	result := make(chan error, 1)
	timer := time.NewTimer(timeout)
	defer timer.Stop()
	go func() {
		// Even a negative reply proves that the SSH peer is alive.
		_, _, err := c.sshConn.SendRequest("keepalive@openssh.com", true, nil)
		result <- err
	}()
	select {
	case err := <-result:
		if err != nil {
			return fmt.Errorf("SSH keepalive: %w", err)
		}
		return nil
	case <-timer.C:
		err := fmt.Errorf("SSH keepalive timed out after %s", timeout)
		c.terminate(err)
		<-result // closing the raw transport releases SendRequest
		return err
	case <-c.stop:
		<-result
		return nil
	}
}
