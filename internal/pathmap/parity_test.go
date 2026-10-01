package pathmap

import (
	"github.com/BurntSushi/toml"
	"github.com/WariKoda/drift/internal/config"
	"testing"
)

func TestSharedRustMappingFixtures(t *testing.T) {
	var fixture struct {
		Cases []struct {
			Name            string           `toml:"name"`
			ProjectRoot     string           `toml:"project_root"`
			RootPath        string           `toml:"root_path"`
			Local           string           `toml:"local"`
			Remote          string           `toml:"remote"`
			ProjectMappings []config.Mapping `toml:"project_mappings"`
			HostMappings    []config.Mapping `toml:"host_mappings"`
			Error           bool             `toml:"error"`
		} `toml:"cases"`
	}
	if _, err := toml.DecodeFile("../../testdata/parity/pathmap.toml", &fixture); err != nil {
		t.Fatal(err)
	}
	if len(fixture.Cases) == 0 {
		t.Fatal("empty parity fixture")
	}
	for _, tc := range fixture.Cases {
		t.Run(tc.Name, func(t *testing.T) {
			mapper := New(tc.ProjectRoot, tc.ProjectMappings, config.Host{RootPath: tc.RootPath, Mappings: tc.HostMappings})
			remote, localErr := mapper.LocalToRemote(tc.Local)
			local, remoteErr := mapper.RemoteToLocal(tc.Remote)
			if tc.Error {
				if localErr == nil || remoteErr == nil {
					t.Fatalf("expected errors in both directions: %v, %v", localErr, remoteErr)
				}
				return
			}
			if localErr != nil || remoteErr != nil {
				t.Fatalf("mapping errors: %v, %v", localErr, remoteErr)
			}
			if remote != tc.Remote || local != tc.Local {
				t.Fatalf("got local=%q remote=%q; want local=%q remote=%q", local, remote, tc.Local, tc.Remote)
			}
		})
	}
}
