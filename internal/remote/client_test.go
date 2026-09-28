package remote

import (
	"context"
	"net"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tlstrust"
)

func TestConnectRejectsRetryChallengeForDifferentEndpoint(t *testing.T) {
	endpoint, err := tlstrust.NormalizeEndpoint("ftps", "other.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	_, err = Connect(context.Background(), config.Host{
		Protocol: "ftps", Hostname: "server.example", Port: 21,
	}, tlstrust.NewManager(), &tlstrust.Challenge{Endpoint: endpoint})
	if err == nil || !strings.Contains(err.Error(), "belongs to") {
		t.Fatalf("error = %v, want endpoint mismatch", err)
	}
}

// A failed connect must return a nil interface, not a nil *Client inside a
// non-nil interface: callers check conn != nil before calling conn.Err().
func TestConnectFailureReturnsNilClient(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := listener.Addr().(*net.TCPAddr).Port
	if err := listener.Close(); err != nil {
		t.Fatal(err)
	}
	for _, protocol := range []string{"sftp", "ftp", "ftps"} {
		t.Run(protocol, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			conn, err := Connect(ctx, config.Host{
				Protocol: protocol, Hostname: "127.0.0.1", Port: port, User: "drift",
				Auth: config.Auth{Type: "password", Password: "secret"},
			}, tlstrust.NewManager(), nil)
			if err == nil {
				t.Fatal("connect to a closed port succeeded")
			}
			if conn != nil {
				t.Fatalf("failed connect returned a non-nil client %T", conn)
			}
		})
	}
}
