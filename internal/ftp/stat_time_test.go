package ftp

import (
	"errors"
	"io"
	"net/textproto"
	"os"
	"testing"
	"time"
)

func TestStatMDTMResponses(t *testing.T) {
	for _, tc := range []struct {
		name         string
		response     string
		wantCode     int
		wantEOF      bool
		wantParse    bool
		wantProtocol bool
		terminal     bool
		wantTime     bool
	}{
		{name: "not advertised"},
		{name: "timestamp", response: "213 20240102030405", wantTime: true},
		{name: "not implemented", response: "502 not implemented"},
		{name: "not implemented for parameter", response: "504 not implemented for parameter"},
		{name: "unexpected success", response: "200 ok", wantCode: 200},
		{name: "unexpected superfluous command", response: "202 superfluous", wantCode: 202},
		{name: "parse failure", response: "500 cannot parse command", wantCode: 500},
		{name: "bad arguments", response: "501 bad pathname", wantCode: 501},
		{name: "bad sequence", response: "503 bad command sequence", wantCode: 503},
		{name: "not logged in", response: "530 not logged in", wantCode: 530},
		{name: "metadata denied", response: "550 permission denied", wantCode: 550},
		{name: "temporary failure", response: "450 temporarily unavailable", wantCode: 450},
		{name: "server failure", response: "451 local processing error", wantCode: 451},
		{name: "service closing", response: "421 shutting down", wantCode: 421, terminal: true},
		{name: "transport EOF", response: "eof", wantEOF: true, terminal: true},
		{name: "invalid timestamp", response: "213 not-a-timestamp", wantParse: true},
		{name: "malformed reply", response: "not an FTP reply", wantProtocol: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := startKeepAliveServer(t, keepAliveServerOptions{mdtm: tc.response})
			client := server.connect(t, 0, testProbeTimeout)
			info, err := client.Stat("/file")
			wantErr := tc.wantCode != 0 || tc.wantEOF || tc.wantParse || tc.wantProtocol
			if (err != nil) != wantErr {
				t.Fatalf("Stat = %v, %v, want error=%v", info, err, wantErr)
			}
			if wantErr {
				if info != nil || errors.Is(err, os.ErrNotExist) {
					t.Fatalf("failed MDTM reported metadata or a missing file: %v, %v", info, err)
				}
				if tc.wantCode != 0 {
					var reply *textproto.Error
					if !errors.As(err, &reply) || reply.Code != tc.wantCode {
						t.Fatalf("Stat error = %v, want FTP status %d", err, tc.wantCode)
					}
				}
				if tc.wantEOF && !errors.Is(err, io.EOF) {
					t.Fatalf("Stat error = %v, want EOF", err)
				}
				if tc.wantParse {
					var parseErr *time.ParseError
					if !errors.As(err, &parseErr) {
						t.Fatalf("Stat error = %v, want timestamp parse failure", err)
					}
				}
				if tc.wantProtocol {
					var protocolErr textproto.ProtocolError
					if !errors.As(err, &protocolErr) {
						t.Fatalf("Stat error = %v, want protocol failure", err)
					}
				}
			} else {
				want := time.Time{}
				if tc.wantTime {
					want = time.Date(2024, 1, 2, 3, 4, 5, 0, time.UTC)
				}
				if info == nil || info.IsDir() || info.Size() != 7 || !info.ModTime().Equal(want) {
					t.Fatalf("Stat = %+v, want file with size 7 and mtime %s", info, want)
				}
			}
			if (client.Err() != nil) != tc.terminal {
				t.Fatalf("terminal error = %v, want terminal=%v", client.Err(), tc.terminal)
			}
			if tc.wantEOF && !errors.Is(client.Err(), io.EOF) {
				t.Fatalf("transport failure lost its cause: %v", client.Err())
			}
			if !client.opMu.TryLock() {
				t.Fatal("Stat retained the operation lock")
			}
			client.opMu.Unlock()
			if tc.response != "" {
				server.waitCommand(t, "MDTM")
			} else {
				server.assertNoCommand(t, "MDTM", testIdleInterval)
			}
		})
	}
}

func TestStatMDTMVerifiesMissingPath(t *testing.T) {
	for _, tc := range []struct {
		name     string
		listings map[string]string
		failures map[string]string
		missing  bool
	}{
		{name: "vanished after SIZE", listings: map[string]string{"/": ""}, missing: true},
		{name: "existing denied file", listings: map[string]string{"/": "type=file; file\r\n"}},
		{name: "unlistable parent", failures: map[string]string{"/": "550 parent denied"}},
		{name: "parent transfer failure", failures: map[string]string{"/": "450 temporary failure"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := startKeepAliveServer(t, keepAliveServerOptions{
				mdtm: "550 metadata unavailable", listings: tc.listings, listingErrors: tc.failures,
			})
			client := server.connect(t, 0, testProbeTimeout)
			info, err := client.Stat("/file")
			if info != nil || err == nil || errors.Is(err, os.ErrNotExist) != tc.missing {
				t.Fatalf("Stat = %v, %v, want missing=%v", info, err, tc.missing)
			}
			if !tc.missing {
				var reply *textproto.Error
				if !errors.As(err, &reply) || reply.Code != 550 {
					t.Fatalf("metadata failure lost its FTP status: %v", err)
				}
			}
			if client.Err() != nil {
				t.Fatalf("metadata refusal killed the connection: %v", client.Err())
			}
		})
	}
}
