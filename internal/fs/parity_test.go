package fs

import (
	"github.com/BurntSushi/toml"
	"testing"
)

func TestSharedRustStagingFixtures(t *testing.T) {
	var fixture struct {
		Cases []struct {
			Name    string
			Staging bool
		}
	}
	if _, err := toml.DecodeFile("../../testdata/parity/staging.toml", &fixture); err != nil {
		t.Fatal(err)
	}
	if len(fixture.Cases) == 0 {
		t.Fatal("empty parity fixture")
	}
	for _, tc := range fixture.Cases {
		t.Run(tc.Name, func(t *testing.T) {
			if got := IsStagingName(tc.Name); got != tc.Staging {
				t.Fatalf("got %v, want %v", got, tc.Staging)
			}
		})
	}
}
