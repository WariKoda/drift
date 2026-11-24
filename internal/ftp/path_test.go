package ftp

import (
	"context"
	"io"
	"strings"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/tlstrust"
)

// FTP ends a command at the line break, so a path holding one would carry a
// second command. Local file names may contain line breaks, and the remote path
// is built from them.
func TestPathsWithLineBreaksNeverReachTheServer(t *testing.T) {
	const victim = "/site/victim.txt"
	for _, tc := range []struct {
		name string
		path string
	}{
		{"line feed", "/site/a\nDELE " + victim},
		{"carriage return and line feed", "/site/a\r\nDELE " + victim},
		{"carriage return", "/site/a\rDELE " + victim},
		{"NUL", "/site/a\x00DELE " + victim},
		{"line feed in a directory", "/site/a\nDELE /site/victim.txt/b"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := ftptest.Start(t, 4)
			server.AddFile(victim, "keep")
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			client, err := Connect(ctx, server.Host(t), tlstrust.NewPolicy(nil))
			if err != nil {
				t.Fatalf("connect: %v", err)
			}
			t.Cleanup(func() { _ = client.Close() })

			operations := map[string]func() error{
				"Stat":    func() error { _, err := client.Stat(tc.path); return err },
				"ReadDir": func() error { _, err := client.ReadDir(tc.path); return err },
				"Open": func() error {
					r, err := client.Open(tc.path)
					if err == nil {
						_ = r.Close()
					}
					return err
				},
				"ReadFile":   func() error { _, err := client.ReadFile(tc.path); return err },
				"Upload":     func() error { return client.Upload(tc.path, strings.NewReader("data")) },
				"DeleteFile": func() error { return client.DeleteFile(tc.path) },
				"WalkFiles":  func() error { return client.WalkFiles(tc.path, func(string) error { return nil }) },
			}
			for name, run := range operations {
				err := run()
				if err == nil || !strings.Contains(err.Error(), "control character") {
					t.Errorf("%s: error = %v, want the path rejected", name, err)
				}
			}
			for _, command := range []string{"DELE", "SIZE", "LIST", "RETR", "STOR", "MKD", "RNFR", "EPSV"} {
				if count := server.CommandCount(command); count != 0 {
					t.Errorf("server received %d %s command(s)", count, command)
				}
			}
			if _, ok := server.File(victim); !ok {
				t.Error("the injected command deleted the file")
			}

			// The connection must still be usable: a rejected path is a
			// per-file error, not a transport failure.
			r, err := client.Open(victim)
			if err != nil {
				t.Fatalf("open after rejected paths: %v", err)
			}
			content, readErr := io.ReadAll(r)
			if err := r.Close(); err != nil || readErr != nil || string(content) != "keep" {
				t.Fatalf("read after rejected paths = %q, %v, %v", content, readErr, err)
			}
		})
	}
}
