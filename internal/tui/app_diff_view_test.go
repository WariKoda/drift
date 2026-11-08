package tui

import (
	"testing"

	"github.com/WariKoda/drift/internal/progress"
	"github.com/WariKoda/drift/internal/tui/diffview"
)

func TestDiffViewSnapshotIsBoundToRequest(t *testing.T) {
	for _, action := range []string{"replace", "abandon", "cancel"} {
		t.Run(action, func(t *testing.T) {
			a := App{}
			id := a.beginDiffRequest()
			snapshot := diffview.ViewSnapshot{}
			a.pendingDiffView, a.pendingDiffViewID = &snapshot, id
			switch action {
			case "replace":
				a.beginDiffRequest()
			case "abandon":
				a.abandonDiffRequest()
			case "cancel":
				a.startNetworkActivity(activityDiffLoad, "Loading", progress.NewTracker("Loading"))
				a.cancelNetworkActivity()
			}
			if a.pendingDiffView != nil || a.pendingDiffViewID != 0 {
				t.Fatal("obsolete request retained reading snapshot")
			}
		})
	}
}
