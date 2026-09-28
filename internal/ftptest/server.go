// Package ftptest provides a real in-process FTP server for tests that need
// a remote host without external infrastructure.
package ftptest

import (
	"bufio"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"fmt"
	"io"
	"math/big"
	"net"
	"path"
	"sort"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
)

// Server is a real FTP server covering the commands drift sends during a
// comparison or sync: the login handshake, optional explicit TLS, SIZE,
// LIST, RETR, STOR, rename and DELE. It serves at most maxSessions logins
// and answers every further connection with 421, emulating a server that limits
// sessions per user.
type Server struct {
	listener    net.Listener
	maxSessions int
	tlsConfig   *tls.Config // non-nil for explicit-TLS FTPS

	mu          sync.Mutex
	files       map[string]string
	accepted    int
	rejected    int
	commands    []string
	dropCommand func(command, argument string) bool
	denyCommand func(command, argument string) bool
	sendData    func(net.Conn, string) error
	wg          sync.WaitGroup
}

// Start runs a plain FTP server until the test ends.
func Start(t testing.TB, maxSessions int) *Server {
	t.Helper()
	return start(t, maxSessions, nil)
}

// StartTLS runs an explicit-TLS FTPS server with a fresh self-signed
// certificate for 127.0.0.1, so clients must trust it explicitly. Control and
// data connections are protected once the client sends AUTH TLS and PROT P.
func StartTLS(t testing.TB, maxSessions int) *Server {
	t.Helper()
	return start(t, maxSessions, &tls.Config{Certificates: []tls.Certificate{selfSignedCertificate(t)}})
}

func start(t testing.TB, maxSessions int, tlsConfig *tls.Config) *Server {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	server := &Server{
		listener:    listener,
		maxSessions: maxSessions,
		tlsConfig:   tlsConfig,
		files:       map[string]string{},
	}
	server.wg.Add(1)
	go func() {
		defer server.wg.Done()
		server.acceptLoop()
	}()
	t.Cleanup(func() {
		_ = listener.Close()
		server.wg.Wait()
	})
	return server
}

func selfSignedCertificate(t testing.TB) tls.Certificate {
	t.Helper()
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatalf("generate key: %v", err)
	}
	template := &x509.Certificate{
		SerialNumber: big.NewInt(1),
		Subject:      pkix.Name{CommonName: "ftptest"},
		IPAddresses:  []net.IP{net.ParseIP("127.0.0.1")},
		NotBefore:    time.Now().Add(-time.Hour),
		NotAfter:     time.Now().Add(time.Hour),
		KeyUsage:     x509.KeyUsageDigitalSignature,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
	}
	der, err := x509.CreateCertificate(rand.Reader, template, template, &key.PublicKey, key)
	if err != nil {
		t.Fatalf("create certificate: %v", err)
	}
	return tls.Certificate{Certificate: [][]byte{der}, PrivateKey: key}
}

// SetDropCommand installs a filter; returning true drops the control
// connection before the command is answered.
func (s *Server) SetDropCommand(fn func(command, argument string) bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.dropCommand = fn
}

// SetDenyCommand installs a filter; returning true answers the command with a
// 550 permission failure without hiding the path from its parent listing.
func (s *Server) SetDenyCommand(fn func(command, argument string) bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.denyCommand = fn
}

// SetSendData replaces how RETR content is written to the data connection,
// for tests that need slow or stalled transfers.
func (s *Server) SetSendData(fn func(data net.Conn, content string) error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.sendData = fn
}

// AddFile stores content at remotePath.
func (s *Server) AddFile(remotePath, content string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.files[path.Clean(remotePath)] = content
}

// File returns the content stored at remotePath.
func (s *Server) File(remotePath string) (string, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	content, ok := s.files[path.Clean(remotePath)]
	return content, ok
}

// Host returns a password-authenticated FTP host for this server.
func (s *Server) Host(t testing.TB) config.Host {
	t.Helper()
	hostname, portString, err := net.SplitHostPort(s.listener.Addr().String())
	if err != nil {
		t.Fatalf("split server address: %v", err)
	}
	port, err := strconv.Atoi(portString)
	if err != nil {
		t.Fatalf("parse server port: %v", err)
	}
	return config.Host{
		Name:     "test",
		Hostname: hostname,
		Port:     port,
		User:     "drift",
		Auth:     config.Auth{Password: "secret"},
		Protocol: s.protocol(),
	}
}

func (s *Server) protocol() string {
	if s.tlsConfig != nil {
		return "ftps"
	}
	return "ftp"
}

// AcceptedSessions counts logins admitted within the session limit.
func (s *Server) AcceptedSessions() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.accepted
}

// RejectedSessions counts logins refused because of the session limit.
func (s *Server) RejectedSessions() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.rejected
}

// CommandCount counts how often command was received on any connection.
func (s *Server) CommandCount(command string) int {
	s.mu.Lock()
	defer s.mu.Unlock()
	count := 0
	for _, got := range s.commands {
		if got == command {
			count++
		}
	}
	return count
}

func (s *Server) acceptLoop() {
	for {
		conn, err := s.listener.Accept()
		if err != nil {
			return
		}
		allowed := s.reserveSession()
		s.wg.Add(1)
		go func() {
			defer s.wg.Done()
			defer conn.Close()
			if !allowed {
				_, _ = io.WriteString(conn, "421 too many sessions\r\n")
				return
			}
			s.serve(conn)
		}()
	}
}

func (s *Server) reserveSession() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.accepted >= s.maxSessions {
		s.rejected++
		return false
	}
	s.accepted++
	return true
}

func (s *Server) serve(conn net.Conn) {
	reader := bufio.NewReader(conn)
	writer := bufio.NewWriter(conn)
	reply := func(format string, args ...any) error {
		if _, err := fmt.Fprintf(writer, format+"\r\n", args...); err != nil {
			return err
		}
		return writer.Flush()
	}

	if err := reply("220 drift diff worker test server"); err != nil {
		return
	}

	var dataListener net.Listener
	protected := false // PROT P: data connections use TLS
	renameFrom := ""
	// secureData runs after the 150 reply: clients start the data-channel
	// handshake only once the server has accepted the transfer.
	secureData := func(data net.Conn) (net.Conn, error) {
		if !protected {
			return data, nil
		}
		secured := tls.Server(data, s.tlsConfig)
		if err := secured.Handshake(); err != nil {
			return data, err // the caller still closes the raw connection
		}
		return secured, nil
	}
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
		command, argument, _ := strings.Cut(strings.TrimSpace(line), " ")
		command = strings.ToUpper(command)
		s.mu.Lock()
		s.commands = append(s.commands, command)
		drop := s.dropCommand
		deny := s.denyCommand
		s.mu.Unlock()
		if drop != nil && drop(command, argument) {
			return
		}
		if deny != nil && deny(command, path.Clean(argument)) {
			if err := reply("550 permission denied"); err != nil {
				return
			}
			continue
		}
		switch command {
		case "AUTH":
			if s.tlsConfig == nil || strings.ToUpper(argument) != "TLS" {
				err = reply("502 TLS not available")
				break
			}
			if err = reply("234 continue with TLS"); err != nil {
				break
			}
			secured := tls.Server(conn, s.tlsConfig)
			if err = secured.Handshake(); err != nil {
				break
			}
			conn = secured
			reader = bufio.NewReader(conn)
			writer = bufio.NewWriter(conn)
		case "PBSZ":
			err = reply("200 PBSZ=0")
		case "PROT":
			protected = strings.ToUpper(argument) == "P"
			err = reply("200 protection level set")
		case "MKD":
			err = reply("257 created")
		case "RNFR":
			if _, ok := s.File(argument); !ok {
				err = reply("550 file not found")
				break
			}
			renameFrom = path.Clean(argument)
			err = reply("350 ready for RNTO")
		case "RNTO":
			s.mu.Lock()
			content, ok := s.files[renameFrom]
			if ok {
				delete(s.files, renameFrom)
				s.files[path.Clean(argument)] = content
			}
			s.mu.Unlock()
			renameFrom = ""
			if ok {
				err = reply("250 renamed")
			} else {
				err = reply("503 RNFR first")
			}
		case "STOR":
			if dataListener == nil {
				err = reply("425 use EPSV first")
				break
			}
			var data net.Conn
			if data, err = dataListener.Accept(); err != nil {
				break
			}
			if err = reply("150 opening data connection"); err == nil {
				data, err = secureData(data)
			}
			if err == nil {
				var received strings.Builder
				_, err = io.Copy(&received, data)
				if err == nil {
					s.AddFile(argument, received.String())
				}
			}
			closeErr := data.Close()
			if err == nil {
				err = closeErr
			}
			if err == nil {
				err = reply("226 transfer complete")
			}
			_ = dataListener.Close()
			dataListener = nil
		case "USER":
			err = reply("331 password required")
		case "PASS":
			err = reply("230 logged in")
		case "FEAT":
			err = reply("500 features unavailable")
		case "TYPE":
			err = reply("200 transfer type set")
		case "NOOP":
			err = reply("200 alive")
		case "DELE":
			s.mu.Lock()
			_, exists := s.files[argument]
			delete(s.files, argument)
			s.mu.Unlock()
			if exists {
				err = reply("250 deleted")
			} else {
				err = reply("550 file not found")
			}
		case "SIZE":
			content, ok := s.File(argument)
			if !ok {
				err = reply("550 file not found")
				break
			}
			err = reply("213 %d", len(content))
		case "EPSV":
			if dataListener != nil {
				_ = dataListener.Close()
			}
			dataListener, err = net.Listen("tcp", "127.0.0.1:0")
			if err == nil {
				err = reply("229 Entering Extended Passive Mode (|||%d|)",
					dataListener.Addr().(*net.TCPAddr).Port)
			}
		case "LIST", "RETR":
			content, ok := s.File(argument)
			if command == "LIST" {
				dir := path.Clean(argument)
				prefix := strings.TrimSuffix(dir, "/") + "/"
				entries := make(map[string]string)
				ok = dir == "/" || dir == "."
				s.mu.Lock()
				for name, data := range s.files {
					if !strings.HasPrefix(name, prefix) {
						continue
					}
					ok = true
					base, _, isDir := strings.Cut(strings.TrimPrefix(name, prefix), "/")
					if isDir {
						entries[base] = fmt.Sprintf("drwxr-xr-x 1 drift drift 0 Jan 01 2025 %s\r\n", base)
					} else {
						entries[base] = fmt.Sprintf("-rw-r--r-- 1 drift drift %d Jan 01 2025 %s\r\n", len(data), base)
					}
				}
				s.mu.Unlock()
				names := make([]string, 0, len(entries))
				for name := range entries {
					names = append(names, name)
				}
				sort.Strings(names)
				var listing strings.Builder
				for _, name := range names {
					listing.WriteString(entries[name])
				}
				content = listing.String()
			}
			if !ok {
				err = reply("550 path not found")
				break
			}
			if dataListener == nil {
				err = reply("425 use EPSV first")
				break
			}
			var data net.Conn
			data, err = dataListener.Accept()
			if err != nil {
				break
			}
			if err = reply("150 opening data connection"); err == nil {
				data, err = secureData(data)
			}
			if err == nil {
				s.mu.Lock()
				send := s.sendData
				s.mu.Unlock()
				if send != nil && command == "RETR" {
					err = send(data, content)
				} else {
					_, err = io.WriteString(data, content)
				}
			}
			closeErr := data.Close()
			if err == nil {
				err = closeErr
			}
			if err == nil {
				err = reply("226 transfer complete")
			}
			_ = dataListener.Close()
			dataListener = nil
		case "QUIT":
			_ = reply("221 goodbye")
			return
		default:
			err = reply("502 command not implemented")
		}
		if err != nil {
			return
		}
	}
}
