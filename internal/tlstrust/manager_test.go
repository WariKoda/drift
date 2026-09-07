package tlstrust

import (
	"testing"
	"time"
)

func TestManagerSeparatesSessionAndPermanentTrust(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	endpoint, err := NormalizeEndpoint("ftps", "server.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	challenge := Challenge{
		Endpoint:    endpoint,
		Fingerprint: "AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA",
		Problems:    []Problem{ProblemUnknownAuthority},
	}

	manager := NewManager()
	manager.TrustSession(challenge)
	trusted, err := manager.HasTrust(endpoint)
	if err != nil {
		t.Fatal(err)
	}
	if !trusted {
		t.Fatal("session trust is missing")
	}
	fresh := NewManager()
	trusted, err = fresh.HasTrust(endpoint)
	if err != nil {
		t.Fatal(err)
	}
	if trusted {
		t.Fatal("session trust survived in a new manager")
	}

	if _, err := manager.TrustPermanently(challenge); err != nil {
		t.Fatalf("trust permanently: %v", err)
	}
	trusted, err = fresh.HasTrust(endpoint)
	if err != nil {
		t.Fatal(err)
	}
	if !trusted {
		t.Fatal("persistent trust is missing in a new manager")
	}
	if err := fresh.Reset(endpoint); err != nil {
		t.Fatalf("reset trust: %v", err)
	}
	trusted, err = manager.HasTrust(endpoint)
	if err != nil {
		t.Fatal(err)
	}
	if !trusted {
		t.Fatal("reset in another manager unexpectedly removed this process's session trust")
	}
}

func TestChallengeTrustUsesUTC(t *testing.T) {
	at := time.Date(2026, 2, 1, 12, 0, 0, 0, time.FixedZone("test", 3600))
	trust := (Challenge{}).Trust(at)
	if trust.TrustedAt.Location() != time.UTC {
		t.Fatalf("location = %v", trust.TrustedAt.Location())
	}
}
