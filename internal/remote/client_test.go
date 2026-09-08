package remote

import (
	"context"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/tlstrust"
)

func TestConnectRejectsRetryChallengeForDifferentEndpoint(t *testing.T) {
	endpoint, err := tlstrust.NormalizeEndpoint("ftps", "other.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	_, err = Connect(context.Background(), config.Host{
		Protocol: "ftps", Hostname: "server.example", Port: 21,
	}, tlstrust.NewManager(), &tlstrust.Challenge{Endpoint: endpoint})
	if err == nil || !strings.Contains(err.Error(), "belongs to") {
		t.Fatalf("error = %v, want endpoint mismatch", err)
	}
}
