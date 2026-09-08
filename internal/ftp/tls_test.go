package ftp

import (
	"bufio"
	"context"
	"crypto/rand"
	"crypto/rsa"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"errors"
	"fmt"
	"math/big"
	"net"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tlstrust"
)

func TestFTPSConnectRequiresAndHonorsExactCertificateTrust(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	server := startFTPSTestServer(t)
	hostname, portText, err := net.SplitHostPort(server.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	port, err := strconv.Atoi(portText)
	if err != nil {
		t.Fatal(err)
	}
	host := config.Host{
		Name: "ftps-test", Hostname: hostname, Port: port, Protocol: "ftps",
		User: "drift", Auth: config.Auth{Password: "secret"},
	}
	endpoint, err := tlstrust.NormalizeEndpoint("ftps", hostname, port)
	if err != nil {
		t.Fatal(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_, err = Connect(ctx, host, tlstrust.NewPolicy(nil))
	var verificationErr *tlstrust.VerificationError
	if !errors.As(err, &verificationErr) {
		t.Fatalf("connect error = %T %v, want VerificationError", err, err)
	}
	if verificationErr.Challenge.Endpoint != endpoint {
		t.Fatalf("challenge endpoint = %+v, want %+v", verificationErr.Challenge.Endpoint, endpoint)
	}

	manager := tlstrust.NewManager()
	manager.TrustSession(verificationErr.Challenge)
	policy, err := manager.PolicyForRetry(verificationErr.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	client, err := Connect(ctx, host, policy)
	if err != nil {
		t.Fatalf("connect with exact session trust: %v", err)
	}
	if err := client.Close(); err != nil {
		t.Fatalf("close FTPS client: %v", err)
	}
}

type ftpsTestServer struct {
	net.Listener
	config tls.Config
	wg     sync.WaitGroup
}

func startFTPSTestServer(t *testing.T) *ftpsTestServer {
	t.Helper()
	certificate := makeTLSCertificate(t)
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	server := &ftpsTestServer{
		Listener: listener,
		config: tls.Config{
			Certificates: []tls.Certificate{certificate},
			MinVersion:   tls.VersionTLS12,
			MaxVersion:   tls.VersionTLS12,
		},
	}
	server.wg.Add(1)
	go func() {
		defer server.wg.Done()
		for {
			conn, acceptErr := listener.Accept()
			if acceptErr != nil {
				return
			}
			server.wg.Add(1)
			go func() {
				defer server.wg.Done()
				_ = server.handle(conn)
			}()
		}
	}()
	t.Cleanup(func() {
		_ = listener.Close()
		server.wg.Wait()
	})
	return server
}

func (s *ftpsTestServer) handle(conn net.Conn) error {
	defer conn.Close()
	plainReader := bufio.NewReader(conn)
	plainWriter := bufio.NewWriter(conn)
	if _, err := fmt.Fprint(plainWriter, "220 FTPS test server\r\n"); err != nil {
		return err
	}
	if err := plainWriter.Flush(); err != nil {
		return err
	}
	line, err := plainReader.ReadString('\n')
	if err != nil {
		return err
	}
	if strings.TrimSpace(strings.ToUpper(line)) != "AUTH TLS" {
		return fmt.Errorf("first command = %q", line)
	}
	if _, err := fmt.Fprint(plainWriter, "234 continue with TLS\r\n"); err != nil {
		return err
	}
	if err := plainWriter.Flush(); err != nil {
		return err
	}

	tlsConn := tls.Server(conn, &s.config)
	if err := tlsConn.Handshake(); err != nil {
		return err
	}
	reader := bufio.NewReader(tlsConn)
	writer := bufio.NewWriter(tlsConn)
	for {
		line, err = reader.ReadString('\n')
		if err != nil {
			return err
		}
		command, _, _ := strings.Cut(strings.TrimSpace(line), " ")
		var response string
		switch strings.ToUpper(command) {
		case "USER":
			response = "331 password required"
		case "PASS":
			response = "230 logged in"
		case "FEAT":
			response = "211 no features"
		case "TYPE", "OPTS", "PBSZ", "PROT":
			response = "200 ok"
		case "QUIT":
			response = "221 goodbye"
		default:
			response = "502 unsupported"
		}
		if _, err := fmt.Fprintf(writer, "%s\r\n", response); err != nil {
			return err
		}
		if err := writer.Flush(); err != nil {
			return err
		}
		if strings.EqualFold(command, "QUIT") {
			return nil
		}
	}
}

func makeTLSCertificate(t *testing.T) tls.Certificate {
	t.Helper()
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Now()
	template := &x509.Certificate{
		SerialNumber: big.NewInt(now.UnixNano()),
		Subject:      pkix.Name{CommonName: "127.0.0.1"},
		IPAddresses:  []net.IP{net.ParseIP("127.0.0.1")},
		NotBefore:    now.Add(-time.Hour),
		NotAfter:     now.Add(time.Hour),
		KeyUsage:     x509.KeyUsageDigitalSignature,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
	}
	raw, err := x509.CreateCertificate(rand.Reader, template, template, &key.PublicKey, key)
	if err != nil {
		t.Fatal(err)
	}
	return tls.Certificate{Certificate: [][]byte{raw}, PrivateKey: key}
}
