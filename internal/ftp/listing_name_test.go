package ftp

import (
	"strings"
	"testing"
)

// The library takes a listed name as the rest of the line, and the client
// joins it onto the listed directory. A name with a slash would point outside
// that directory, so the comparison of one folder could reach other files.
func TestListingNamesMustBeSinglePathComponents(t *testing.T) {
	for _, tc := range []struct {
		name    string
		listing string
	}{
		{"parent traversal", "type=file;size=1; ../../src/Kernel.php\r\n"},
		{"nested path", "type=file;size=1; sub/file.txt\r\n"},
		{"absolute path", "type=file;size=1; /etc/passwd\r\n"},
		{"traversal in a directory name", "type=dir; ../other\r\n"},
	} {
		for _, operation := range []string{"walk", "read directory"} {
			t.Run(operation+"/"+tc.name, func(t *testing.T) {
				s := startKeepAliveServer(t, keepAliveServerOptions{
					listings: map[string]string{"/root": "type=file;size=1; ok.txt\r\n" + tc.listing},
				})
				c := s.connect(t, 0, testProbeTimeout)
				var err error
				var walked []string
				if operation == "walk" {
					err = c.WalkFiles("/root", func(p string) error {
						walked = append(walked, p)
						return nil
					})
				} else {
					_, err = c.ReadDir("/root")
				}
				if err == nil || !strings.Contains(err.Error(), "not a single path component") {
					t.Fatalf("error = %v, want the listing rejected", err)
				}
				for _, p := range walked {
					if !strings.HasPrefix(p, "/root/") || strings.Contains(p, "..") {
						t.Fatalf("walk reported %q outside the listed directory", p)
					}
				}
			})
		}
	}
}

func TestListingSkipsDotEntries(t *testing.T) {
	s := startKeepAliveServer(t, keepAliveServerOptions{
		listings: map[string]string{"/root": "type=cdir; .\r\ntype=pdir; ..\r\ntype=file;size=1; ok.txt\r\n"},
	})
	c := s.connect(t, 0, testProbeTimeout)
	entries, err := c.ReadDir("/root")
	if err != nil || len(entries) != 1 || entries[0].Path != "/root/ok.txt" {
		t.Fatalf("entries = %+v, error = %v", entries, err)
	}
}
