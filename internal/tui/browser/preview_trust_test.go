package browser

import (
	"errors"
	"reflect"
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

func TestRemotePreviewIgnoresCompletionFromAnotherSessionOrOperation(t *testing.T) {
	for _, stale := range []string{"browser", "connection", "host", "operation"} {
		t.Run(stale, func(t *testing.T) {
			root := "/project"
			host := config.Host{Name: "staging"}
			m := Model{
				remoteHost: &host, remoteLoadID: 4, remoteSession: &root,
				remotePreviewReading: true, remotePreviewID: 7,
				preview: filePreview{active: true, source: PaneRemote, generation: 7, loading: true},
			}
			request := previewRequest{generation: 7, source: PaneRemote, path: "/file.txt", host: host, remoteID: 4, session: &root}
			switch stale {
			case "browser":
				otherRoot := root // Same path and counters, different browser instance.
				request.session = &otherRoot
			case "connection":
				request.remoteID--
			case "host":
				request.host.Name = "other"
			case "operation":
				request.generation--
			}
			msg := msgPreviewLoaded{request: request, lines: []string{"obsolete"}}
			if m.AcceptsPreviewResult(msg) {
				t.Fatal("stale result was accepted")
			}
			before := m
			m, cmd := m.Update(msg)
			if cmd != nil || !reflect.DeepEqual(m, before) {
				t.Fatal("stale completion changed the current preview or released its read")
			}
		})
	}
}
