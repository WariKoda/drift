package tui

import (
	"bufio"
	"context"
	"fmt"
	"net"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
	"github.com/WariKoda/drift/internal/tui/browser"
	"github.com/WariKoda/drift/internal/tui/certtrust"
	"github.com/WariKoda/drift/internal/tui/diffview"
)

// loopbackConnection uses a real FTP control connection. Closing the server
// socket lets the client's monitor discover a loss independently of UI input.
func loopbackConnection(t *testing.T) (remote.Client, config.Host, func()) {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	accepted := make(chan net.Conn, 1)
	finished := make(chan struct{})
	go func() {
		defer close(finished)
		socket, err := listener.Accept()
		if err != nil {
			return
		}
		defer socket.Close()
		accepted <- socket
		fmt.Fprint(socket, "220 loopback FTP\r\n")
		scanner := bufio.NewScanner(socket)
		for scanner.Scan() {
			command, _, _ := strings.Cut(scanner.Text(), " ")
			response := "200 OK\r\n"
			switch command {
			case "USER":
				response = "331 password\r\n"
			case "PASS":
				response = "230 logged in\r\n"
			case "FEAT":
				response = "211 no features\r\n"
			case "QUIT":
				fmt.Fprint(socket, "221 goodbye\r\n")
				return
			}
			if _, err := fmt.Fprint(socket, response); err != nil {
				return
			}
		}
	}()
	hostname, portString, err := net.SplitHostPort(listener.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	port, err := strconv.Atoi(portString)
	if err != nil {
		t.Fatal(err)
	}
	interval := 1
	host := config.Host{Name: "loopback", Hostname: hostname, Port: port, Protocol: "ftp", User: "test", KeepAliveInterval: &interval}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	conn, err := remote.Connect(ctx, host, nil, nil)
	if err != nil {
		listener.Close()
		t.Fatal(err)
	}
	socket := <-accepted
	stop := func() { _ = socket.Close() }
	t.Cleanup(func() {
		stop()
		_ = listener.Close()
		_ = conn.Close()
		select {
		case <-finished:
		case <-time.After(3 * time.Second):
			t.Error("FTP server did not stop")
		}
	})
	return conn, host, stop
}

func waitConnectionLoss(t *testing.T, conn remote.Client) {
	t.Helper()
	select {
	case <-conn.Done():
		if conn.Err() == nil {
			t.Fatal("connection ended without reporting the probe failure")
		}
	case <-time.After(4 * time.Second):
		t.Fatal("keep-alive did not discover the closed server")
	}
}

func TestConnectionLossIsHandledBehindCertificateModal(t *testing.T) {
	conn, host, stop := loopbackConnection(t)
	app, err := New(t.TempDir(), &config.MergedConfig{}, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(app.Close)
	app.diffView = diffview.New(nil, host, conn, nil, 80, 24)
	app.state.Screen = ScreenDiffView
	watch := app.watchConnection(conn)
	prompt := certtrust.New(tlstrust.Challenge{}, 80, 24)
	app.certPrompt = &prompt
	stop()
	waitConnectionLoss(t, conn)
	model, _ := app.Update(watch())
	app = model.(App)
	if !strings.Contains(app.globalError, "Remote disconnected") {
		t.Fatalf("status = %q", app.globalError)
	}
	if app.certPrompt == nil {
		t.Fatal("connection failure dismissed the modal")
	}
}

func TestStaleRemoteResultIsClosedBehindCertificateModal(t *testing.T) {
	conn, host, _ := loopbackConnection(t)
	app, err := New(t.TempDir(), &config.MergedConfig{}, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	defer app.Close()
	prompt := certtrust.New(tlstrust.Challenge{}, 80, 24)
	app.certPrompt = &prompt
	model, closeCmd := app.Update(browser.MsgRemoteLoaded{Host: host, ID: 1, Conn: conn})
	if closeCmd == nil {
		t.Fatal("modal discarded the stale connection without cleanup")
	}
	closeCmd()
	select {
	case <-conn.Done():
	default:
		t.Fatal("stale connection is still open")
	}
	if model.(App).certPrompt == nil {
		t.Fatal("stale result dismissed the certificate modal")
	}
}

func TestConnectionWatchRejectsOldConnectionAndProject(t *testing.T) {
	first, _, stop := loopbackConnection(t)
	second, _, _ := loopbackConnection(t)
	app, err := New(t.TempDir(), &config.MergedConfig{}, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	defer app.Close()
	oldWatch := app.watchConnection(first)
	stop()
	waitConnectionLoss(t, first)
	old := oldWatch().(msgConnectionEnded)
	app.watchConnection(second)
	app.connectionEnded(old)
	if app.globalError != "" {
		t.Fatal("stale connection changed status")
	}
	old.id = app.connectionSeq
	old.root = "another-project"
	app.connectionEnded(old)
	if app.globalError != "" {
		t.Fatal("old project changed status")
	}
}

func TestConnectionLossFollowsBrowserToDiffOwnership(t *testing.T) {
	conn, host, stop := loopbackConnection(t)
	app, err := New(t.TempDir(), &config.MergedConfig{}, nil, nil, ScreenBrowser, false)
	if err != nil {
		t.Fatal(err)
	}
	defer app.Close()
	watch := app.watchConnection(conn)
	app.diffView = diffview.New(nil, host, conn, nil, 80, 24)
	app.state.Screen = ScreenDiffView
	stop()
	waitConnectionLoss(t, conn)
	model, _ := app.Update(watch())
	app = model.(App)
	if !strings.Contains(app.globalError, "Remote disconnected") {
		t.Fatalf("diff status = %q", app.globalError)
	}
}

func TestConnectionWatchStopsOnNormalCloseOrReplacement(t *testing.T) {
	conn, _, _ := loopbackConnection(t)
	app := App{}
	watch := app.watchConnection(conn)
	app.watchConnection(nil)
	if msg := watch(); msg != nil {
		t.Fatalf("cancelled observer returned %T", msg)
	}
	watch = app.watchConnection(conn)
	if err := conn.Close(); err != nil {
		t.Fatal(err)
	}
	model, _ := app.Update(watch())
	if model.(App).globalError != "" {
		t.Fatal("normal close produced an error")
	}
}
