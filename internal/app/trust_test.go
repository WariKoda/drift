package app

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// The app answers a VerificationError with the certificate prompt; after the
// user trusts it, the retry carries the challenge. Every connection of that
// retry, including the extra compare workers, must accept the certificate.
func TestLoadRetriesUnknownFTPSCertificateAfterTrust(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir()) // persistent trust lives in the config dir
	root := t.TempDir()
	server := ftptest.StartTLS(t, 4)
	for _, name := range []string{"a.txt", "b.txt"} {
		if err := os.WriteFile(filepath.Join(root, name), []byte("local "+name), 0o600); err != nil {
			t.Fatal(err)
		}
		server.AddFile("/"+name, "remote "+name)
	}
	selection := fs.NewSelectionState()
	selection.Marked[root] = struct{}{}
	trust := tlstrust.NewManager()
	request := LoadRequest{
		Host:        server.Host(t),
		Config:      &config.MergedConfig{ProjectRoot: root},
		Local:       selection,
		Trust:       trust,
		IdleTimeout: 5 * time.Second,
	}

	tracker := progress.NewTracker("Connecting…")
	_, err := Load(tracker.Context(), request, tracker)
	var verificationErr *tlstrust.VerificationError
	if !errors.As(err, &verificationErr) {
		t.Fatalf("load with an unknown certificate = %v, want a verification error", err)
	}

	trust.TrustSession(verificationErr.Challenge)
	challenge := verificationErr.Challenge
	request.Required = &challenge
	before := server.AcceptedSessions()
	tracker = progress.NewTracker("Connecting…")
	loaded, err := Load(tracker.Context(), request, tracker)
	if err != nil {
		t.Fatalf("retry after trust: %v", err)
	}
	defer loaded.Conn.Close()
	defer loaded.Root.Close()
	if len(loaded.Sessions) != 2 {
		t.Fatalf("sessions = %+v, want both differing files", loaded.Sessions)
	}
	for _, session := range loaded.Sessions {
		if session.Err != nil {
			t.Fatalf("compare %s: %v", session.RemotePath, session.Err)
		}
	}
	if used := server.AcceptedSessions() - before; used < 2 {
		t.Fatalf("retry used %d connection(s), want the primary plus an extra worker", used)
	}
}
