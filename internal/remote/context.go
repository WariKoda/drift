package remote

import (
	"context"
	"sync"
)

// CloseOnContextDone closes client if ctx ends before detach is called.
// detach waits for a close already in progress, so returning from an operation
// cannot race a late cancellation that closes a connection handed back to its
// owner.
func CloseOnContextDone(ctx context.Context, client Client) (detach func()) {
	stop := make(chan struct{})
	done := make(chan struct{})
	go func() {
		defer close(done)
		select {
		case <-ctx.Done():
			_ = client.Close()
		case <-stop:
		}
	}()

	var once sync.Once
	return func() {
		once.Do(func() { close(stop) })
		// If cancellation and detachment became ready together, select may have
		// taken the stop branch. Closing here preserves the promise that an ended
		// context always closes the client before detach returns.
		if ctx.Err() != nil {
			_ = client.Close()
		}
		<-done
	}
}
