package remoteedit

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/remote"
	"github.com/WariKoda/drift/internal/tlstrust"
)

func newFTPSession(t *testing.T, tls bool, content string) (*Session, remote.Client, *ftptest.Server) {
	t.Helper()
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	server := ftptest.Start
	if tls {
		server = ftptest.StartTLS
	}
	srv := server(t, 8)
	srv.AddFile("/file.txt", content)
	host := srv.Host(t)
	trust := tlstrust.NewManager()
	conn, err := remote.Connect(context.Background(), host, trust, nil)
	if tls {
		var verification *tlstrust.VerificationError
		if !errors.As(err, &verification) {
			t.Fatalf("expected FTPS challenge: %v", err)
		}
		trust.TrustSession(verification.Challenge)
		conn, err = remote.Connect(context.Background(), host, trust, &verification.Challenge)
	}
	if err != nil {
		t.Fatal(err)
	}
	s := New(Request{Host: host, RemotePath: "/file.txt", ProjectRoot: t.TempDir()})
	t.Cleanup(func() { _ = s.Close(true); _ = conn.Close() })
	return s, conn, srv
}

func TestRemoteEditingPreservesBytesAndUploadsOnlyCapturedSnapshot(t *testing.T) {
	for _, tls := range []bool{false, true} {
		t.Run(map[bool]string{false: "FTP", true: "FTPS"}[tls], func(t *testing.T) {
			original := "\xef\xbb\xbfline\r\nsecond\n\t\x00\xff"
			s, conn, server := newFTPSession(t, tls, original)
			result := s.Prepare(context.Background(), conn)
			if result.Err != nil {
				t.Fatal(result.Err)
			}
			if data, err := os.ReadFile(result.File); err != nil || string(data) != original {
				t.Fatalf("download = %q, %v", data, err)
			}
			for file, want := range map[string]os.FileMode{s.dir: 0700, result.File: 0600} {
				info, err := os.Stat(file)
				if err != nil || info.Mode().Perm() != want {
					t.Fatalf("mode %s = %v, %v", file, info, err)
				}
			}
			if result := s.Capture(context.Background()); result.Err != nil || result.Changed {
				t.Fatalf("unchanged capture: %+v", result)
			}
			if server.CommandCount("STOR") != 0 {
				t.Fatal("download or capture uploaded automatically")
			}
			if err := os.WriteFile(s.file, []byte("confirmed"), 0644); err != nil {
				t.Fatal(err)
			}
			if result := s.Capture(context.Background()); result.Err != nil || !result.Changed {
				t.Fatalf("capture: %+v", result)
			}
			if err := os.WriteFile(s.file, []byte("later edits"), 0600); err != nil {
				t.Fatal(err)
			}
			result = s.Save(context.Background(), conn, nil)
			if result.Err != nil || !result.Saved || !result.Changed || result.Unknown {
				t.Fatalf("save result = %+v", result)
			}
			if content, _ := server.File("/file.txt"); content != "confirmed" {
				t.Fatalf("uploaded %q, want the confirmed snapshot", content)
			}
			if data, _ := os.ReadFile(s.file); string(data) != "later edits" {
				t.Fatal("later edits were lost")
			}
		})
	}
}

func TestConflictNeverUploadsAndReloadNeedsExplicitCall(t *testing.T) {
	s, conn, server := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.file, []byte("edited"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil {
		t.Fatal(result.Err)
	}
	server.AddFile("/file.txt", "new") // same length as the original
	result := s.Save(context.Background(), conn, nil)
	if !errors.Is(result.Err, ErrConflict) || result.Saved || server.CommandCount("STOR") != 0 {
		t.Fatalf("conflict wrote: %+v", result)
	}
	if data, _ := os.ReadFile(s.file); string(data) != "edited" {
		t.Fatal("conflict discarded the working copy")
	}
	if result := s.Reload(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if data, _ := os.ReadFile(s.file); string(data) != "new" {
		t.Fatal("explicit reload did not replace working copy")
	}
}

func TestPermissionFailureIsNotReportedAsConfirmedDeletion(t *testing.T) {
	s, conn, server := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.file, []byte("new"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil {
		t.Fatal(result.Err)
	}
	server.SetDenyCommand(func(command, _ string) bool { return command == "RETR" })
	result := s.Save(context.Background(), conn, nil)
	if result.Err == nil || errors.Is(result.Err, ErrConflict) || server.CommandCount("STOR") != 0 {
		t.Fatalf("permission failure = %+v", result)
	}
}

func TestUploadFailureHasUnknownOutcomeAndRetainsCopy(t *testing.T) {
	s, conn, server := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.file, []byte("new"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil {
		t.Fatal(result.Err)
	}
	server.SetDropCommand(func(command, _ string) bool { return command == "RNFR" })
	result := s.Save(context.Background(), conn, nil)
	if result.Err == nil || !result.Unknown || result.Saved {
		t.Fatalf("lost connection result = %+v", result)
	}
	if server.CommandCount("STOR") != 1 {
		t.Fatal("upload was retried")
	}
	if err := s.Close(false); err != nil {
		t.Fatal(err)
	}
	if data, err := os.ReadFile(s.file); err != nil || string(data) != "new" {
		t.Fatalf("failed upload lost edits: %q, %v", data, err)
	}
}

func TestReviewRecognizesAlreadyUploadedCandidateWithoutWriting(t *testing.T) {
	s, conn, server := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	if err := os.WriteFile(s.file, []byte("new"), 0600); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil {
		t.Fatal(result.Err)
	}
	server.AddFile("/file.txt", "new")
	result := s.Review(context.Background(), conn)
	if result.Err != nil || !result.Saved || server.CommandCount("STOR") != 0 {
		t.Fatalf("review = %+v", result)
	}
}

func TestScopeSizeAndLocalEscapeChecks(t *testing.T) {
	for _, test := range []string{"mapping", "excluded", "remote too large", "local too large", "symlink", "directory", "cache in project"} {
		t.Run(test, func(t *testing.T) {
			s, conn, server := newFTPSession(t, false, "old")
			switch test {
			case "mapping":
				s.request.Host.Mappings = []config.Mapping{{Local: "app", Remote: "app"}}
			case "excluded":
				s.request.RemotePath = "/.git/config"
			case "remote too large":
				server.AddFile("/file.txt", strings.Repeat("x", MaxBytes+1))
			case "cache in project":
				t.Setenv("XDG_CACHE_HOME", filepath.Join(s.request.ProjectRoot, "cache"))
			}
			result := s.Prepare(context.Background(), conn)
			if test == "mapping" || test == "excluded" || test == "remote too large" || test == "cache in project" {
				if result.Err == nil || server.CommandCount("RETR") != 0 {
					t.Fatalf("unsafe prepare: %+v", result)
				}
				return
			}
			if result.Err != nil {
				t.Fatal(result.Err)
			}
			switch test {
			case "local too large":
				file, err := os.OpenFile(s.file, os.O_WRONLY, 0600)
				if err != nil {
					t.Fatal(err)
				}
				if err := file.Truncate(MaxBytes + 1); err != nil {
					t.Fatal(err)
				}
				if err := file.Close(); err != nil {
					t.Fatal(err)
				}
			case "symlink", "directory":
				if err := os.Remove(s.file); err != nil {
					t.Fatal(err)
				}
				if test == "symlink" {
					outside := filepath.Join(t.TempDir(), "outside")
					if err := os.WriteFile(outside, []byte("secret"), 0600); err != nil {
						t.Fatal(err)
					}
					if err := os.Symlink(outside, s.file); err != nil {
						t.Fatal(err)
					}
				} else if err := os.Mkdir(s.file, 0700); err != nil {
					t.Fatal(err)
				}
			}
			if result := s.Capture(context.Background()); result.Err == nil {
				t.Fatalf("unsafe working file accepted: %+v", result)
			}
		})
	}
}

func TestAtomicEditorSaveKeepsExtensionAndRestrictsPermissions(t *testing.T) {
	s, conn, _ := newFTPSession(t, false, "old")
	if result := s.Prepare(context.Background(), conn); result.Err != nil {
		t.Fatal(result.Err)
	}
	replacement := filepath.Join(s.dir, "editor-save")
	if err := os.WriteFile(replacement, []byte("new"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(replacement, s.file); err != nil {
		t.Fatal(err)
	}
	if result := s.Capture(context.Background()); result.Err != nil || !result.Changed {
		t.Fatalf("capture: %+v", result)
	}
	info, err := os.Stat(s.file)
	if err != nil || info.Mode().Perm() != 0600 || filepath.Ext(s.file) != ".txt" {
		t.Fatalf("working copy: %v, %v", info, err)
	}
}
