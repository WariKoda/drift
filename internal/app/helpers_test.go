package app

import (
	"context"
	"testing"
	"time"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/remote"
)

func connectTestHost(t *testing.T, host config.Host) remote.Client {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	conn, err := remote.Connect(ctx, host, nil, nil)
	if err != nil {
		t.Fatalf("connect: %v", err)
	}
	t.Cleanup(func() { _ = conn.Close() })
	return conn
}

func waitTestFailure(t *testing.T, conn remote.Client) error {
	t.Helper()
	select {
	case <-conn.Done():
	case <-time.After(5 * time.Second):
		t.Fatal("connection monitor did not report failure")
	}
	if conn.Err() == nil {
		t.Fatal("connection closed without a terminal error")
	}
	return conn.Err()
}
