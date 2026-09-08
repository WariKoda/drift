package ftp

import (
	"bufio"
	"context"
	"crypto/tls"
	"errors"
	"fmt"
	"io"
	"net"
	"net/textproto"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tlstrust"
)

const testIdleInterval = 20 * time.Millisecond
const testProbeTimeout = 100 * time.Millisecond

func TestKeepAliveIdleAndConnectContextDetached(t *testing.T) {
	for _, secure := range []bool{false, true} {
		t.Run(fmt.Sprintf("TLS=%v", secure), func(t *testing.T) {
			s := startKeepAliveServer(t, keepAliveServerOptions{secure: secure})
			c := s.connect(t, testIdleInterval, testProbeTimeout)
			// The setup context has already been canceled by the helper.
			command := s.waitCommand(t, "NOOP")
			if command.secure != secure {
				t.Fatalf("NOOP TLS = %v, want %v", command.secure, secure)
			}
			if s.dataAccepted.Load() != 0 {
				t.Fatal("keepalive opened a data channel")
			}
			if _, err := c.Stat("/file"); err != nil {
				t.Fatalf("command after probe: %v", err)
			}
			s.waitCommand(t, "NOOP")
			closeFTPClient(t, c)
			if c.Err() != nil {
				t.Fatalf("normal Close error = %v", c.Err())
			}
		})
	}
}

func TestKeepAliveSchedulesFromLastOperation(t *testing.T) {
	release := make(chan struct{})
	s := startKeepAliveServer(t, keepAliveServerOptions{block: "DELE", release: release})
	interval := 400 * time.Millisecond
	c := s.connect(t, interval, testProbeTimeout)
	result := make(chan error, 1)
	go func() { result <- c.DeleteFile("/file") }()
	s.waitCommand(t, "DELE")
	s.assertNoCommand(t, "NOOP", 280*time.Millisecond)
	close(release)
	if err := waitFTPResult(t, result); err != nil {
		t.Fatal(err)
	}
	finished := time.Now()
	s.waitCommand(t, "NOOP")
	if idle := time.Since(finished); idle > interval+80*time.Millisecond {
		t.Fatalf("probe delayed %s after activity, interval is %s", idle, interval)
	}
}

func TestWalkFilesKeepsWorkingWhenOptionalConnectionIsRefused(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{})
	c := s.connect(t, 0, testProbeTimeout)
	// The established control connection remains usable. Only additional
	// worker connections are refused; data listeners are separate sockets.
	if err := s.Listener.Close(); err != nil {
		t.Fatal(err)
	}
	var files []string
	if err := c.WalkFiles("/", func(path string) error {
		files = append(files, path)
		return nil
	}); err != nil {
		t.Fatalf("optional worker failure broke the walk: %v", err)
	}
	if len(files) != 1 || files[0] != "/file" {
		t.Fatalf("files = %v", files)
	}
}

func TestKeepAliveExplicitZeroDisablesMonitor(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{})
	host := s.host()
	zero := 0
	host.KeepAliveInterval = &zero
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	c, err := Connect(ctx, host, tlstrust.NewPolicy(nil))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = c.Close() })
	select {
	case <-c.monitorDone:
	default:
		t.Fatal("disabled keepalive started a monitor")
	}
	if _, err := c.Stat("/file"); err != nil {
		t.Fatal(err)
	}
	s.assertNoCommand(t, "NOOP", 4*testIdleInterval)
	select {
	case <-c.Done():
		t.Fatal("disabled monitor closed a healthy connection")
	default:
	}
}

func TestKeepAliveProbeFailures(t *testing.T) {
	for _, response := range []string{"eof", "500 NOOP refused", "silent"} {
		t.Run(response, func(t *testing.T) {
			s := startKeepAliveServer(t, keepAliveServerOptions{noop: response})
			c := s.connect(t, testIdleInterval, testProbeTimeout)
			s.waitCommand(t, "NOOP")
			select {
			case <-c.Done():
			case <-time.After(2 * time.Second):
				t.Fatal("probe failure did not signal Done")
			}
			err := c.Err()
			if err == nil || !strings.Contains(err.Error(), "NOOP") {
				t.Fatalf("terminal error = %v", err)
			}
			switch response {
			case "eof":
				if !errors.Is(err, io.EOF) {
					t.Fatalf("want EOF, got %v", err)
				}
			case "silent":
				var timeout net.Error
				if !errors.As(err, &timeout) || !timeout.Timeout() {
					t.Fatalf("want timeout, got %v", err)
				}
			default:
				var reply *textproto.Error
				if !errors.As(err, &reply) || reply.Code != 500 {
					t.Fatalf("want 500 reply, got %v", err)
				}
			}
			closeFTPClient(t, c)
			if c.Err() != err {
				t.Fatal("Close replaced the terminal error")
			}
			if _, operationErr := c.Stat("/file"); operationErr != err {
				t.Fatalf("operation on failed client = %v, want original error", operationErr)
			}
		})
	}
}

func TestKeepAliveReservesStreamingReaderUntilClose(t *testing.T) {
	for _, secure := range []bool{false, true} {
		t.Run(fmt.Sprintf("TLS=%v", secure), func(t *testing.T) {
			s := startKeepAliveServer(t, keepAliveServerOptions{secure: secure})
			c := s.connect(t, testIdleInterval, testProbeTimeout)
			r, err := c.Open("/file")
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = r.Close() })
			data, err := io.ReadAll(r)
			if err != nil || string(data) != "payload" {
				t.Fatalf("read = %q, %v", data, err)
			}
			s.assertNoCommand(t, "NOOP", 4*testIdleInterval)
			if err := r.Close(); err != nil {
				t.Fatal(err)
			}
			c.opMu.Lock()
			idle := time.Since(c.lastActivity)
			c.opMu.Unlock()
			if idle >= testIdleInterval {
				t.Fatalf("stream Close did not refresh activity: %s", idle)
			}
			s.waitCommand(t, "NOOP")
			if c.Err() != nil {
				t.Fatalf("stream EOF was treated as terminal: %v", c.Err())
			}
			c.lifeMu.Lock()
			transports := len(c.transports)
			c.lifeMu.Unlock()
			if transports != 1 {
				t.Fatalf("retained %d transports after transfer, want control only", transports)
			}
		})
	}
}

func TestKeepAliveDeadlineClearedAndReaderCloseReserved(t *testing.T) {
	release := make(chan struct{})
	s := startKeepAliveServer(t, keepAliveServerOptions{beforeFinal: release})
	c := s.connect(t, testIdleInterval, testProbeTimeout)
	s.waitCommand(t, "NOOP")
	r, err := c.Open("/file")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := io.ReadAll(r); err != nil {
		t.Fatal(err)
	}
	closed := make(chan error, 1)
	go func() { closed <- r.Close() }()
	select {
	case err := <-closed:
		t.Fatalf("reader Close returned before final reply: %v", err)
	case <-time.After(2 * testProbeTimeout):
	}
	if c.opMu.TryLock() {
		c.opMu.Unlock()
		t.Fatal("reader Close released the operation lock before final reply")
	}
	close(release)
	if err := waitFTPResult(t, closed); err != nil {
		t.Fatalf("probe deadline leaked into streaming Close: %v", err)
	}
	s.waitCommand(t, "NOOP")
}

func TestCloseDuringProbeIsNormalAndConcurrentSafe(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{noop: "silent"})
	c := s.connect(t, testIdleInterval, time.Hour)
	s.waitCommand(t, "NOOP")
	var wg sync.WaitGroup
	for range 20 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_ = c.Close()
		}()
	}
	closed := make(chan error, 1)
	go func() { wg.Wait(); closed <- nil }()
	waitFTPResult(t, closed)
	if c.Err() != nil {
		t.Fatalf("intentional close reported probe failure: %v", c.Err())
	}
	closeFTPClient(t, c)
}

func TestCloseAbortsBlockedOperations(t *testing.T) {
	for _, command := range []string{"SIZE", "MLSD", "STOR", "RNTO"} {
		t.Run(command, func(t *testing.T) {
			s := startKeepAliveServer(t, keepAliveServerOptions{block: command, release: make(chan struct{})})
			c := s.connect(t, testIdleInterval, testProbeTimeout)
			result := make(chan error, 1)
			go func() {
				var err error
				switch command {
				case "SIZE":
					_, err = c.Stat("/file")
				case "MLSD":
					_, err = c.ReadDir("/")
				default:
					err = c.Upload("/file", strings.NewReader("payload"))
				}
				result <- err
			}()
			s.waitCommand(t, command)
			s.assertNoCommand(t, "NOOP", 4*testIdleInterval)
			closeFTPClient(t, c)
			if err := waitFTPResult(t, result); err == nil {
				t.Fatal("aborted operation succeeded")
			}
			if c.Err() != nil {
				t.Fatalf("normal Close error = %v", c.Err())
			}
		})
	}
}

func TestCloseReleasesQueuedOperationsBeforeReaderClose(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{})
	c := s.connect(t, testIdleInterval, testProbeTimeout)
	r, err := c.Open("/file")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := io.ReadAll(r); err != nil {
		t.Fatal(err)
	}
	result := make(chan error, 1)
	go func() { _, err := c.Stat("/file"); result <- err }()
	// The stream still owns opMu. Neither the queued operation nor a new
	// operation may depend on the caller eventually closing that reader.
	closeFTPClient(t, c)
	if err := waitFTPResult(t, result); !errors.Is(err, net.ErrClosed) {
		t.Fatalf("queued Stat error = %v", err)
	}
	if _, err := c.ReadDir("/"); !errors.Is(err, net.ErrClosed) {
		t.Fatalf("ReadDir after Close = %v", err)
	}
	_ = r.Close()
}

func TestFTPSUploadPreservesDataTLSIncludingEmptyFiles(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{secure: true})
	c := s.connect(t, testIdleInterval, testProbeTimeout)
	for _, payload := range []string{"payload", ""} {
		if err := c.Upload("/file", strings.NewReader(payload)); err != nil {
			t.Fatalf("TLS upload of %d bytes: %v", len(payload), err)
		}
	}
	if got := s.dataAccepted.Load(); got != 2 {
		t.Fatalf("data connections = %d, want 2", got)
	}
	s.waitCommand(t, "NOOP")
}

func TestCloseAbortsBlockedDataRead(t *testing.T) {
	for _, secure := range []bool{false, true} {
		t.Run(fmt.Sprintf("TLS=%v", secure), func(t *testing.T) {
			s := startKeepAliveServer(t, keepAliveServerOptions{secure: secure, beforeData: make(chan struct{})})
			c := s.connect(t, testIdleInterval, testProbeTimeout)
			r, err := c.Open("/file")
			if err != nil {
				t.Fatal(err)
			}
			result := make(chan error, 1)
			go func() {
				_, readErr := io.ReadAll(r)
				result <- errors.Join(readErr, r.Close())
			}()
			s.waitCommand(t, "RETR")
			closeFTPClient(t, c)
			if err := waitFTPResult(t, result); err == nil {
				t.Fatal("aborted read succeeded")
			}
		})
	}
}

func TestWalkClientsUseKeepAliveAndCloseWithParent(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{block: "MLSD", release: make(chan struct{})})
	c := s.connect(t, testIdleInterval, testProbeTimeout)
	result := make(chan error, 1)
	go func() { result <- c.WalkFiles("/", func(string) error { return nil }) }()
	s.waitCommand(t, "MLSD")
	c.lifeMu.Lock()
	children := make([]*Client, 0, len(c.children))
	for child := range c.children {
		children = append(children, child)
	}
	c.lifeMu.Unlock()
	if len(children) != maxWalkWorkers-1 {
		t.Fatalf("walk created %d extra clients", len(children))
	}
	for _, child := range children {
		select {
		case <-child.monitorDone:
			t.Fatal("worker monitor stopped with its setup context")
		default:
		}
	}
	closeFTPClient(t, c)
	if err := waitFTPResult(t, result); err == nil {
		t.Fatal("aborted walker lost its error")
	}
	for _, child := range children {
		closeFTPClient(t, child)
	}
}

func TestWalkFilesReportsIdleWorkerProbeFailure(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{noop: "500 NOOP refused"})
	// Disable only the parent's monitor. Direct worker Connect calls resolve
	// the host's one-second interval through the public configuration API.
	c := s.connect(t, 0, testProbeTimeout)
	seconds := 1
	c.Host.KeepAliveInterval = &seconds
	callbackEntered := make(chan struct{})
	release := make(chan struct{})
	var releaseOnce sync.Once
	defer releaseOnce.Do(func() { close(release) })
	result := make(chan error, 1)
	go func() {
		result <- c.WalkFiles("/", func(string) error {
			close(callbackEntered)
			<-release
			return nil
		})
	}()
	select {
	case <-callbackEntered:
	case <-time.After(3 * time.Second):
		t.Fatal("walker did not reach callback")
	}
	s.waitCommand(t, "NOOP")
	c.lifeMu.Lock()
	children := make([]*Client, 0, len(c.children))
	for child := range c.children {
		children = append(children, child)
	}
	c.lifeMu.Unlock()
	for _, child := range children {
		select {
		case <-child.Done():
		case <-time.After(3 * time.Second):
			t.Fatal("idle worker did not terminate on failed probe")
		}
	}
	releaseOnce.Do(func() { close(release) })
	if err := waitFTPResult(t, result); err == nil || !strings.Contains(err.Error(), "NOOP") {
		t.Fatalf("walker did not surface worker probe error: %v", err)
	}
	for _, child := range children {
		closeFTPClient(t, child)
	}
	if c.Err() != nil {
		t.Fatalf("worker failure incorrectly marked parent transport dead: %v", c.Err())
	}
}

func TestWalkFilesReportsWorkerListingFailure(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{listError: true})
	c := s.connect(t, testIdleInterval, testProbeTimeout)
	if err := c.WalkFiles("/", func(string) error { return nil }); err == nil {
		t.Fatal("walker swallowed LIST failure")
	}
	c.lifeMu.Lock()
	children := len(c.children)
	c.lifeMu.Unlock()
	if children != 0 {
		t.Fatalf("walk retained %d clients", children)
	}
}

func TestCloseJoinsPendingWalkerLogin(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{blockWorkerLogin: true, release: make(chan struct{})})
	c := s.connect(t, 0, testProbeTimeout)
	s.waitCommand(t, "TYPE") // drain the parent's login commands
	result := make(chan error, 1)
	go func() { result <- c.WalkFiles("/", func(string) error { return nil }) }()
	s.waitCommand(t, "USER")
	closeFTPClient(t, c)
	if err := waitFTPResult(t, result); err == nil {
		t.Fatal("walker did not report canceled worker setup")
	}
}

func TestConnectCancellationClosesLoginTransport(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{block: "USER", release: make(chan struct{})})
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	result := make(chan error, 1)
	go func() {
		c, err := Connect(ctx, s.host(), tlstrust.NewPolicy(nil))
		if c != nil {
			_ = c.Close()
		}
		result <- err
	}()
	s.waitCommand(t, "USER")
	cancel()
	if err := waitFTPResult(t, result); err == nil {
		t.Fatal("canceled login succeeded")
	}
}

func closeFTPClient(t *testing.T, c *Client) {
	t.Helper()
	closed := make(chan error, 1)
	go func() { closed <- c.Close() }()
	if err := waitFTPResult(t, closed); err != nil {
		t.Fatal(err)
	}
	select {
	case <-c.Done():
	default:
		t.Fatal("Close did not signal Done")
	}
	select {
	case <-c.monitorDone:
	default:
		t.Fatal("Close left the monitor running")
	}
	c.lifeMu.Lock()
	transports := len(c.transports)
	c.lifeMu.Unlock()
	if transports != 0 {
		t.Fatalf("Close retained %d transports", transports)
	}
}

func waitFTPResult(t *testing.T, result <-chan error) error {
	t.Helper()
	select {
	case err := <-result:
		return err
	case <-time.After(3 * time.Second):
		t.Fatal("FTP operation did not finish")
		return nil
	}
}

// This fixture speaks FTP over loopback TCP, with explicit TLS and passive
// data connections. Gates delay real wire replies; no client APIs are mocked.
type keepAliveServerOptions struct {
	secure           bool
	noop             string
	block            string
	release          <-chan struct{}
	beforeData       <-chan struct{}
	beforeFinal      <-chan struct{}
	listError        bool
	blockWorkerLogin bool
}

type ftpCommand struct {
	name   string
	secure bool
}

type keepAliveServer struct {
	net.Listener
	options      keepAliveServerOptions
	tlsConfig    *tls.Config
	commands     chan ftpCommand
	stop         chan struct{}
	wg           sync.WaitGroup
	mu           sync.Mutex
	connections  map[net.Conn]struct{}
	dataAccepted atomic.Int32
	logins       atomic.Int32
}

func startKeepAliveServer(t *testing.T, options keepAliveServerOptions) *keepAliveServer {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	s := &keepAliveServer{
		Listener: listener, options: options, commands: make(chan ftpCommand, 1024),
		stop: make(chan struct{}), connections: make(map[net.Conn]struct{}),
	}
	if options.secure {
		s.tlsConfig = &tls.Config{Certificates: []tls.Certificate{makeTLSCertificate(t)}, MinVersion: tls.VersionTLS12, MaxVersion: tls.VersionTLS12}
	}
	s.wg.Add(1)
	go func() {
		defer s.wg.Done()
		for {
			conn, err := listener.Accept()
			if err != nil {
				return
			}
			s.wg.Add(1)
			go func() {
				defer s.wg.Done()
				s.track(conn)
				defer s.untrack(conn)
				s.serve(conn)
			}()
		}
	}()
	t.Cleanup(func() {
		close(s.stop)
		_ = listener.Close()
		s.mu.Lock()
		for conn := range s.connections {
			_ = conn.Close()
		}
		s.mu.Unlock()
		done := make(chan error, 1)
		go func() { s.wg.Wait(); done <- nil }()
		waitFTPResult(t, done)
	})
	return s
}

func (s *keepAliveServer) track(conn net.Conn) {
	s.mu.Lock()
	defer s.mu.Unlock()
	select {
	case <-s.stop:
		_ = conn.Close()
	default:
		s.connections[conn] = struct{}{}
	}
}

func (s *keepAliveServer) untrack(conn net.Conn) {
	_ = conn.Close()
	s.mu.Lock()
	delete(s.connections, conn)
	s.mu.Unlock()
}

func (s *keepAliveServer) host() config.Host {
	addr := s.Addr().(*net.TCPAddr)
	protocol := "ftp"
	if s.options.secure {
		protocol = "ftps"
	}
	return config.Host{Name: "keepalive-test", Hostname: addr.IP.String(), Port: addr.Port, Protocol: protocol, User: "drift", Auth: config.Auth{Password: "secret"}}
}

func (s *keepAliveServer) connect(t *testing.T, interval, timeout time.Duration) *Client {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	policy := tlstrust.NewPolicy(nil)
	if s.options.secure {
		_, err := connect(ctx, s.host(), policy, interval, timeout)
		var verificationErr *tlstrust.VerificationError
		if !errors.As(err, &verificationErr) {
			t.Fatalf("untrusted TLS error = %v", err)
		}
		manager := tlstrust.NewManager()
		manager.TrustSession(verificationErr.Challenge)
		policy, err = manager.PolicyForRetry(verificationErr.Challenge)
		if err != nil {
			t.Fatal(err)
		}
	}
	c, err := connect(ctx, s.host(), policy, interval, timeout)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = c.Close() })
	return c
}

func (s *keepAliveServer) waitCommand(t *testing.T, name string) ftpCommand {
	t.Helper()
	timer := time.NewTimer(3 * time.Second)
	defer timer.Stop()
	for {
		select {
		case command := <-s.commands:
			if command.name == name {
				return command
			}
		case <-timer.C:
			t.Fatalf("server did not receive %s", name)
			return ftpCommand{}
		}
	}
}

func (s *keepAliveServer) assertNoCommand(t *testing.T, name string, duration time.Duration) {
	t.Helper()
	timer := time.NewTimer(duration)
	defer timer.Stop()
	for {
		select {
		case command := <-s.commands:
			if command.name == name {
				t.Fatalf("unexpected %s", name)
			}
		case <-timer.C:
			return
		}
	}
}

func (s *keepAliveServer) waitGate(gate <-chan struct{}) bool {
	if gate == nil {
		return true
	}
	select {
	case <-gate:
		return true
	case <-s.stop:
		return false
	}
}

func (s *keepAliveServer) serve(raw net.Conn) {
	control := raw
	reader := bufio.NewReader(control)
	secure := false
	reply := func(text string) bool {
		_, err := fmt.Fprintf(control, "%s\r\n", text)
		return err == nil
	}
	if !reply("220 loopback FTP") {
		return
	}
	var dataListener net.Listener
	defer func() {
		if dataListener != nil {
			_ = dataListener.Close()
		}
	}()
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			return
		}
		command, _, _ := strings.Cut(strings.TrimSpace(line), " ")
		s.commands <- ftpCommand{name: command, secure: secure}
		blocked := command == s.options.block
		if command == "USER" && s.options.blockWorkerLogin && s.logins.Add(1) > 1 {
			blocked = true
		}
		if blocked && !s.waitGate(s.options.release) {
			return
		}
		switch command {
		case "AUTH":
			if s.tlsConfig == nil || !reply("234 start TLS") {
				return
			}
			tlsConn := tls.Server(raw, s.tlsConfig)
			if tlsConn.Handshake() != nil {
				return
			}
			control, reader, secure = tlsConn, bufio.NewReader(tlsConn), true
		case "USER":
			reply("331 password required")
		case "PASS":
			reply("230 logged in")
		case "FEAT":
			reply("211-Features:\r\n MLST type*;size*;modify*;\r\n211 End")
		case "TYPE", "PBSZ", "PROT":
			reply("200 ok")
		case "DELE", "RNTO":
			reply("250 ok")
		case "MLST":
			reply("500 unsupported")
		case "SIZE":
			reply("213 7")
		case "MKD":
			reply("257 created")
		case "RNFR":
			reply("350 rename target required")
		case "NOOP":
			switch s.options.noop {
			case "eof":
				return
			case "silent":
			case "":
				reply("200 alive")
			default:
				reply(s.options.noop)
			}
		case "EPSV":
			if dataListener != nil {
				_ = dataListener.Close()
			}
			dataListener, err = net.Listen("tcp", "127.0.0.1:0")
			if err != nil {
				return
			}
			reply(fmt.Sprintf("229 Passive (|||%d|)", dataListener.Addr().(*net.TCPAddr).Port))
		case "RETR", "STOR", "MLSD":
			if dataListener == nil {
				reply("425 need EPSV")
				continue
			}
			if command == "MLSD" && s.options.listError {
				reply("550 listing refused")
				continue
			}
			// The client dials before issuing RETR/STOR/MLSD, so Accept
			// always has a queued connection here, even during shutdown.
			data, err := dataListener.Accept()
			if err != nil {
				return
			}
			s.dataAccepted.Add(1)
			s.track(data)
			if !reply("150 opening data") {
				s.untrack(data)
				return
			}
			if !s.waitGate(s.options.beforeData) {
				s.untrack(data)
				return
			}
			stream := data
			if secure {
				stream = tls.Server(data, s.tlsConfig)
			}
			switch command {
			case "STOR":
				_, err = io.Copy(io.Discard, stream)
			case "RETR":
				_, err = io.WriteString(stream, "payload")
			case "MLSD":
				_, err = io.WriteString(stream, "type=file;size=7;modify=20240102030405; file\r\n")
			}
			_ = stream.Close()
			s.untrack(data)
			if err != nil || !s.waitGate(s.options.beforeFinal) {
				return
			}
			reply("226 transfer complete")
		case "QUIT":
			reply("221 goodbye")
			return
		default:
			reply("502 unsupported")
		}
	}
}
