package ftp

import (
	"errors"
	"fmt"
	"io"
	"net"
	"net/textproto"
	"sync"
	"time"

	"github.com/WariKoda/drift/internal/log"
)

const ftpConnectTimeout = 15 * time.Second

// Done closes after the terminal error has been stored. A normal Close also
// signals Done, but leaves Err nil.
func (c *Client) Done() <-chan struct{} { return c.done }

func (c *Client) Err() error {
	c.lifeMu.Lock()
	defer c.lifeMu.Unlock()
	return c.err
}

// Close aborts control and data transports without waiting for opMu. Sending
// QUIT would race a command or block behind an abandoned streaming reader.
// Raw sockets are closed even for FTPS, avoiding a blocking TLS close_notify.
func (c *Client) Close() error {
	if c == nil {
		return nil
	}
	c.terminate(nil)
	c.dialWG.Wait()
	<-c.monitorDone
	return nil
}

func (c *Client) terminate(err error) {
	c.closeOnce.Do(func() {
		c.lifeMu.Lock()
		c.closed = true
		c.err = err
		transports := make([]*trackedConn, 0, len(c.transports))
		for conn := range c.transports {
			transports = append(transports, conn)
		}
		children := make([]*Client, 0, len(c.children))
		for child := range c.children {
			children = append(children, child)
		}
		close(c.done)
		c.lifeMu.Unlock()

		c.cancel()
		for _, conn := range transports {
			_ = conn.Close()
		}
		for _, child := range children {
			_ = child.Close()
		}
		if err != nil {
			log.Error("FTP connection lost", "protocol", c.Host.Protocol,
				"host", c.Host.Name, "hostname", c.Host.Hostname,
				"endpoint", c.control.RemoteAddr().String(), "err", err)
		}
	})
}

func (c *Client) connectionError() error {
	c.lifeMu.Lock()
	defer c.lifeMu.Unlock()
	if c.err != nil {
		return c.err
	}
	if c.closed {
		return net.ErrClosed
	}
	return nil
}

func (c *Client) beginOperation() error {
	for {
		// Unlike Mutex.Lock, this wait can be interrupted by Close even
		// when the caller of Open has not yet closed its reader.
		c.lifeMu.Lock()
		if c.closed {
			err := c.err
			c.lifeMu.Unlock()
			if err == nil {
				err = net.ErrClosed
			}
			return err
		}
		released := c.opReleased
		acquired := c.opMu.TryLock()
		c.lifeMu.Unlock()
		if acquired {
			return nil
		}
		select {
		case <-c.done:
			return c.connectionError()
		case <-released:
		}
	}
}

func (c *Client) releaseOperation() {
	c.lifeMu.Lock()
	c.opMu.Unlock()
	close(c.opReleased)
	c.opReleased = make(chan struct{})
	c.lifeMu.Unlock()
}

func (c *Client) endOperation(err error) {
	// File-level FTP refusals do not kill a healthy session. Transport
	// failures and 421 (service closing) do. A data reader's ordinary EOF
	// never reaches here; Response.Close consumes the final control reply.
	var networkErr net.Error
	var replyErr *textproto.Error
	if err != nil && (errors.As(err, &networkErr) || errors.Is(err, io.EOF) ||
		errors.Is(err, io.ErrUnexpectedEOF) || errors.Is(err, net.ErrClosed) ||
		(errors.As(err, &replyErr) && replyErr.Code == 421)) {
		c.terminate(err)
	}
	c.lastActivity = time.Now()
	c.releaseOperation()
}

func (c *Client) monitor(interval, timeout time.Duration) {
	defer close(c.monitorDone)
	timer := time.NewTimer(interval)
	defer timer.Stop()
	for {
		select {
		case <-c.done:
			return
		case <-timer.C:
		}
		// No queued probe behind a transfer, including Response.Close's
		// final 226 reply. Recheck idle time only after taking ownership.
		if !c.opMu.TryLock() {
			timer.Reset(interval)
			continue
		}
		if c.connectionError() != nil {
			c.releaseOperation()
			return
		}
		if idle := time.Since(c.lastActivity); idle < interval {
			c.releaseOperation()
			// Schedule at the end of the actual idle interval, rather than
			// delaying the probe up to another full interval after activity.
			timer.Reset(interval - idle)
			continue
		}
		err := c.control.SetDeadline(time.Now().Add(timeout))
		if err == nil {
			err = c.conn.NoOp()
		}
		// Clear the probe-only deadline before another command can run.
		// On any failure the stream is discarded, never resynchronized.
		err = errors.Join(err, c.control.SetDeadline(time.Time{}))
		if err != nil {
			c.terminate(fmt.Errorf("FTP keep-alive NOOP: %w", err))
		}
		c.lastActivity = time.Now()
		c.releaseOperation()
		if err != nil {
			return
		}
		timer.Reset(interval)
	}
}

// dial tracks every raw socket, including passive data sockets. Registration
// and shutdown share lifeMu, so a dial finishing after Close cannot escape.
func (c *Client) dial(network, address string) (net.Conn, error) {
	c.lifeMu.Lock()
	if c.closed {
		c.lifeMu.Unlock()
		return nil, net.ErrClosed
	}
	c.dialWG.Add(1)
	c.lifeMu.Unlock()
	defer c.dialWG.Done()

	dialer := net.Dialer{Timeout: ftpConnectTimeout}
	raw, err := dialer.DialContext(c.lifeCtx, network, address)
	if err != nil {
		return nil, err
	}
	conn := &trackedConn{Conn: raw, owner: c}
	c.lifeMu.Lock()
	if c.closed {
		c.lifeMu.Unlock()
		_ = raw.Close()
		return nil, net.ErrClosed
	}
	c.transports[conn] = struct{}{}
	c.lifeMu.Unlock()
	return conn, nil
}

// Removing sockets on their ordinary Close keeps long sessions from retaining
// every completed transfer. Embedding preserves deadlines and RemoteAddr.
// TLS wraps this type, not vice versa, so forced shutdown always reaches TCP.
type trackedConn struct {
	net.Conn
	owner *Client
	once  sync.Once
	err   error
}

func (c *trackedConn) Close() error {
	c.once.Do(func() {
		c.err = c.Conn.Close()
		c.owner.lifeMu.Lock()
		delete(c.owner.transports, c)
		c.owner.lifeMu.Unlock()
	})
	return c.err
}
