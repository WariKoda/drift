package browser

import (
	"errors"
	"testing"

	"github.com/WariKoda/drift/internal/config"
)

func TestRemotePreviewResultIsBoundToConnectionGenerationAndHost(t *testing.T) {
	currentHost := config.Host{Name: "current", Hostname: "current.example"}
	oldHost := config.Host{Name: "old", Hostname: "old.example"}
	model := Model{
		remoteHost:   &currentHost,
		remoteLoadID: 4,
		preview: filePreview{
			active:     true,
			source:     PaneRemote,
			generation: 7,
		},
	}
	base := previewRequest{
		generation: 7,
		source:     PaneRemote,
		path:       "/file.txt",
		host:       currentHost,
		remoteID:   4,
	}
	if !model.AcceptsPreviewResult(msgPreviewLoaded{request: base, err: errors.New("test")}) {
		t.Fatal("current preview result was rejected")
	}

	staleGeneration := base
	staleGeneration.remoteID = 3
	if model.AcceptsPreviewResult(msgPreviewLoaded{request: staleGeneration}) {
		t.Fatal("old remote connection generation was accepted")
	}
	staleHost := base
	staleHost.host = oldHost
	if model.AcceptsPreviewResult(msgPreviewLoaded{request: staleHost}) {
		t.Fatal("old remote host was accepted")
	}
}
