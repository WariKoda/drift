package ftp

import (
	"errors"
	"fmt"
	"net/textproto"
	"os"
	"strings"
	"testing"
)

func TestVerifiedMissingPaths(t *testing.T) {
	for _, operation := range []string{"stat", "walk", "read directory"} {
		for _, tc := range []struct {
			name      string
			target    string
			listings  map[string]string
			failures  map[string]string
			missing   bool
			wantError string
		}{
			{name: "missing basename", target: "/missing", listings: map[string]string{"/": "type=file; missing-extra\r\n"}, failures: map[string]string{"/missing": "550 unavailable"}, missing: true},
			{name: "missing ancestor", target: "/missing/parent/file", listings: map[string]string{"/": ""}, failures: map[string]string{"/missing/parent/file": "550 unavailable", "/missing/parent": "550 unavailable", "/missing": "550 unavailable"}, missing: true},
			{name: "existing denied target", target: "/denied", listings: map[string]string{"/": "type=dir; denied\r\n"}, failures: map[string]string{"/denied": "550 permission denied"}, wantError: "permission denied"},
			{name: "existing denied ancestor", target: "/denied/file", listings: map[string]string{"/": "type=dir; denied\r\n"}, failures: map[string]string{"/denied/file": "550 unavailable", "/denied": "550 parent denied"}, wantError: "parent denied"},
			{name: "unlistable root", target: "/missing", failures: map[string]string{"/missing": "550 unavailable", "/": "550 root denied"}, wantError: "root denied"},
			{name: "parent transfer failure", target: "/missing", failures: map[string]string{"/missing": "550 unavailable", "/": "450 temporary listing failure"}, wantError: "temporary listing failure"},
			{name: "target transfer failure", target: "/missing", failures: map[string]string{"/missing": "450 temporary target failure"}, wantError: "temporary target failure"},
			{name: "root itself", target: "/", failures: map[string]string{"/": "550 root denied"}, wantError: "root denied"},
			{name: "relative missing ancestor", target: "missing/child", listings: map[string]string{".": ""}, failures: map[string]string{"missing/child": "550 unavailable", "missing": "550 unavailable"}, missing: true},
		} {
			t.Run(operation+"/"+tc.name, func(t *testing.T) {
				s := startKeepAliveServer(t, keepAliveServerOptions{sizeError: "550 metadata denied", listings: tc.listings, listingErrors: tc.failures})
				c := s.connect(t, 0, testProbeTimeout)
				var err error
				if operation == "stat" {
					_, err = c.Stat(tc.target)
				} else if operation == "read directory" {
					_, err = c.ReadDir(tc.target)
				} else {
					err = c.WalkFiles(tc.target, func(p string) error { return fmt.Errorf("unexpected file %s", p) })
				}
				if err == nil || errors.Is(err, os.ErrNotExist) != tc.missing {
					t.Fatalf("error = %v, want missing=%v", err, tc.missing)
				}
				if tc.wantError != "" && !strings.Contains(err.Error(), tc.wantError) {
					t.Fatalf("error = %v, want %q preserved", err, tc.wantError)
				}
				if !tc.missing {
					var reply *textproto.Error
					if !errors.As(err, &reply) {
						t.Fatalf("FTP status lost: %v", err)
					}
				}
				if !c.opMu.TryLock() {
					t.Fatal("operation lock retained")
				}
				c.opMu.Unlock()
			})
		}
	}
}

func TestWalkDescendantFailureIsNotMissingRoot(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{
		listings:      map[string]string{"/root": "type=dir; vanished\r\ntype=dir; other\r\n", "/": ""},
		listingErrors: map[string]string{"/root/vanished": "550 child unavailable"},
	})
	c := s.connect(t, 0, testProbeTimeout)
	err := c.WalkFiles("/root", func(string) error { return nil })
	if err == nil || errors.Is(err, os.ErrNotExist) || !strings.Contains(err.Error(), "child unavailable") {
		t.Fatalf("descendant failure = %v", err)
	}
}
