package tlstrust

import (
	"fmt"
	"sync"
	"time"

	"github.com/WariKoda/drift/internal/config"
)

// Manager owns trust granted for the current process and composes it with the
// persistent trust store for each new connection.
type Manager struct {
	mu      sync.RWMutex
	session map[string]Trust
}

// NewManager creates an empty session trust manager.
func NewManager() *Manager {
	return &Manager{session: make(map[string]Trust)}
}

// Policy reloads persistent trust and returns an immutable handshake snapshot.
func (m *Manager) Policy() (Policy, error) {
	persistent, err := config.LoadTrustedCertificates()
	if err != nil {
		return Policy{}, err
	}
	entries := make([]Trust, 0, len(persistent))
	for _, stored := range persistent {
		entry, convertErr := trustFromConfig(stored)
		if convertErr != nil {
			return Policy{}, convertErr
		}
		entries = append(entries, entry)
	}
	m.mu.RLock()
	session := make([]Trust, 0, len(m.session))
	for _, entry := range m.session {
		session = append(session, entry)
	}
	m.mu.RUnlock()
	entries = append(entries, session...)
	return NewPolicy(entries), nil
}

// PolicyForRetry additionally pins the next connection to the certificate the
// user just inspected.
func (m *Manager) PolicyForRetry(challenge Challenge) (Policy, error) {
	policy, err := m.Policy()
	if err != nil {
		return Policy{}, err
	}
	return policy.Require(challenge.Trust(time.Now())), nil
}

// TrustSession grants an exception until this process exits.
func (m *Manager) TrustSession(challenge Challenge) Trust {
	entry := challenge.Trust(time.Now())
	if len(entry.Problems) == 0 {
		return entry
	}
	m.mu.Lock()
	m.session[entry.Endpoint.key()] = entry
	m.mu.Unlock()
	return entry
}

// TrustPermanently persists an exception, then makes it immediately available
// to this process. A failed write grants no session fallback.
func (m *Manager) TrustPermanently(challenge Challenge) (Trust, error) {
	entry := challenge.Trust(time.Now())
	if len(entry.Problems) == 0 {
		return entry, nil
	}
	if err := config.SaveTrustedCertificate(trustToConfig(entry)); err != nil {
		return Trust{}, err
	}
	m.mu.Lock()
	m.session[entry.Endpoint.key()] = entry
	m.mu.Unlock()
	return entry, nil
}

// Reset removes session and persistent trust for endpoint.
func (m *Manager) Reset(endpoint Endpoint) error {
	if err := config.DeleteTrustedCertificate(endpoint.Protocol, endpoint.Hostname, endpoint.Port); err != nil {
		return err
	}
	m.mu.Lock()
	delete(m.session, endpoint.key())
	m.mu.Unlock()
	return nil
}

// HasTrust reports whether the current snapshot contains an exception for an
// endpoint. It is intended for the host manager UI.
func (m *Manager) HasTrust(endpoint Endpoint) (bool, error) {
	policy, err := m.Policy()
	if err != nil {
		return false, err
	}
	_, ok := policy.trusted[endpoint.key()]
	return ok, nil
}

func trustFromConfig(stored config.TrustedCertificate) (Trust, error) {
	endpoint, err := NormalizeEndpoint(stored.Protocol, stored.Hostname, stored.Port)
	if err != nil {
		return Trust{}, err
	}
	problems := make([]Problem, len(stored.Problems))
	for i, problem := range stored.Problems {
		problems[i] = Problem(problem)
	}
	entry := Trust{
		Endpoint:    endpoint,
		Fingerprint: stored.Fingerprint,
		Problems:    normalizeProblems(problems),
		TrustedAt:   stored.TrustedAt,
	}
	if entry.Endpoint.Hostname != stored.Hostname {
		return Trust{}, fmt.Errorf("trusted certificate hostname %q is not normalized", stored.Hostname)
	}
	return entry, nil
}

func trustToConfig(entry Trust) config.TrustedCertificate {
	problems := make([]string, len(entry.Problems))
	for i, problem := range entry.Problems {
		problems[i] = string(problem)
	}
	return config.TrustedCertificate{
		Protocol:    entry.Endpoint.Protocol,
		Hostname:    entry.Endpoint.Hostname,
		Port:        entry.Endpoint.Port,
		Fingerprint: entry.Fingerprint,
		Problems:    problems,
		TrustedAt:   entry.TrustedAt,
	}
}
