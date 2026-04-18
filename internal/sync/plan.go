// Package sync handles building and executing file sync plans.
package sync

import "github.com/WariKoda/drift/internal/config"

// SyncDirection specifies whether to push local→remote or pull remote→local.
type SyncDirection int

const (
	DirectionUpload   SyncDirection = iota // local → remote
	DirectionDownload                      // remote → local
)

// SyncItem is a single file to transfer.
type SyncItem struct {
	LocalPath  string
	RemotePath string
	Direction  SyncDirection
}

// Plan collects all items to be synced for a given host.
type Plan struct {
	Host  config.Host
	Items []SyncItem
}

// ItemStatus tracks the state of a single SyncItem during execution.
type ItemStatus int

const (
	ItemPending ItemStatus = iota
	ItemInFlight
	ItemDone
	ItemFailed
)

// ItemProgress holds the current transfer state for one SyncItem.
type ItemProgress struct {
	Item       SyncItem
	Status     ItemStatus
	BytesDone  int64
	BytesTotal int64
	Err        error
}

// Progress holds the overall sync execution state.
type Progress struct {
	Items      []ItemProgress
	TotalBytes int64
	SentBytes  int64
}
