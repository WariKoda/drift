package fs

import (
	"context"
	"errors"
	"testing"
)

func TestClassifierConstructionHonorsCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(t.Context())
	cancel()
	if _, err := NewClassifier(ctx, t.TempDir()); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled classifier construction returned %v", err)
	}
}
