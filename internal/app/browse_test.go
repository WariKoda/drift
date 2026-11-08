package app

import (
	"context"
	"errors"
	"testing"

	"github.com/WariKoda/drift/internal/ftptest"
	"github.com/WariKoda/drift/internal/tlstrust"
)

func TestBrowseRemoteHandsOffLiveConnection(t *testing.T) {
	server := ftptest.Start(t, 1)
	server.AddFile("/site/file.txt", "content")
	host := server.Host(t)
	host.RootPath = "/site/../site/"
	ctx, cancel := context.WithCancel(context.Background())
	result, err := BrowseRemote(ctx, host, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer result.Conn.Close()
	cancel()
	if result.Root != "/site" || len(result.Entries) != 1 {
		t.Fatalf("result = %+v", result)
	}
	if _, err := result.Conn.ReadDir(result.Root); err != nil {
		t.Fatalf("caller context closed handed-off connection: %v", err)
	}
}

func TestHostListsRootOnRealConnection(t *testing.T) {
	server := ftptest.Start(t, 2)
	if err := TestHost(context.Background(), server.Host(t), nil, nil); err != nil {
		t.Fatal(err)
	}
	if server.CommandCount("LIST") != 1 {
		t.Fatal("connection test did not list root")
	}
}

func TestBrowseRemoteFailures(t *testing.T) {
	server := ftptest.Start(t, 3)
	server.SetDenyCommand(func(command, _ string) bool { return command == "LIST" })
	result, err := BrowseRemote(context.Background(), server.Host(t), nil, nil)
	if err == nil || result.Conn != nil {
		t.Fatalf("failed listing retained connection: %+v, %v", result, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := BrowseRemote(ctx, server.Host(t), nil, nil); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancellation = %v", err)
	}
}

func TestBrowseRemoteReturnsCertificateChallenge(t *testing.T) {
	server := ftptest.StartTLS(t, 1)
	_, err := BrowseRemote(context.Background(), server.Host(t), nil, nil)
	var verification *tlstrust.VerificationError
	if !errors.As(err, &verification) {
		t.Fatalf("certificate challenge lost: %v", err)
	}
}
