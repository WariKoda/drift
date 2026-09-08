// Package tlstrust verifies FTPS server certificates and represents narrowly
// scoped exceptions approved by the user.
package tlstrust

import (
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"encoding/hex"
	"errors"
	"fmt"
	"net"
	"sort"
	"strings"
	"sync"
	"time"
)

// Problem identifies a certificate verification failure that a user may
// explicitly approve for one certificate and endpoint.
type Problem string

const (
	ProblemUnknownAuthority  Problem = "unknown_authority"
	ProblemHostnameMismatch  Problem = "hostname_mismatch"
	ProblemExpired           Problem = "expired"
	ProblemNotYetValid       Problem = "not_yet_valid"
	ProblemCertificateChange Problem = "certificate_changed"
)

// Endpoint identifies an FTPS control endpoint. Passive data connections use
// the same endpoint identity because the FTP library reuses its TLS config.
type Endpoint struct {
	Protocol string
	Hostname string
	Port     int
}

// NormalizeEndpoint returns the canonical identity used by trust entries.
func NormalizeEndpoint(protocol, hostname string, port int) (Endpoint, error) {
	protocol = strings.ToLower(strings.TrimSpace(protocol))
	if protocol != "ftps" {
		return Endpoint{}, fmt.Errorf("certificate trust is only supported for ftps, got %q", protocol)
	}
	if port == 0 {
		port = 21
	}
	if port < 1 || port > 65535 {
		return Endpoint{}, fmt.Errorf("invalid FTPS port %d", port)
	}
	hostname = strings.TrimSpace(hostname)
	if hostname == "" {
		return Endpoint{}, errors.New("FTPS hostname is required")
	}
	if ip := net.ParseIP(strings.Trim(hostname, "[]")); ip != nil {
		hostname = ip.String()
	} else {
		hostname = strings.ToLower(strings.TrimSuffix(hostname, "."))
		if hostname == "" {
			return Endpoint{}, errors.New("FTPS hostname is required")
		}
	}
	return Endpoint{Protocol: protocol, Hostname: hostname, Port: port}, nil
}

// Address returns the endpoint in host:port form.
func (e Endpoint) Address() string {
	return net.JoinHostPort(e.Hostname, fmt.Sprintf("%d", e.Port))
}

func (e Endpoint) key() string {
	return e.Protocol + "\x00" + e.Hostname + "\x00" + fmt.Sprintf("%d", e.Port)
}

// Trust permits the listed problems for one exact leaf certificate.
type Trust struct {
	Endpoint    Endpoint
	Fingerprint string
	Problems    []Problem
	TrustedAt   time.Time
}

// Challenge contains bounded certificate details suitable for a trust prompt.
type Challenge struct {
	Endpoint       Endpoint
	Fingerprint    string
	Problems       []Problem
	Subject        string
	Issuer         string
	DNSNames       []string
	IPAddresses    []string
	NotBefore      time.Time
	NotAfter       time.Time
	PreviousSHA256 string
}

// Trust creates the exact exception represented by this challenge.
func (c Challenge) Trust(at time.Time) Trust {
	problems := make([]Problem, 0, len(c.Problems))
	for _, problem := range c.Problems {
		if problem != ProblemCertificateChange {
			problems = append(problems, problem)
		}
	}
	return Trust{
		Endpoint:    c.Endpoint,
		Fingerprint: c.Fingerprint,
		Problems:    normalizeProblems(problems),
		TrustedAt:   at.UTC(),
	}
}

// VerificationError reports a certificate that failed normal verification.
type VerificationError struct {
	Challenge Challenge
	cause     error
}

func (e *VerificationError) Error() string {
	parts := make([]string, len(e.Challenge.Problems))
	for i, problem := range e.Challenge.Problems {
		parts[i] = string(problem)
	}
	return fmt.Sprintf("verify FTPS certificate for %s: %s", e.Challenge.Endpoint.Address(), strings.Join(parts, ", "))
}

func (e *VerificationError) Unwrap() error { return e.cause }

// Policy is an immutable snapshot used by TLS handshakes.
type Policy struct {
	trusted  map[string]Trust
	required *Trust
	now      func() time.Time
}

// NewPolicy creates a verification policy from approved exceptions.
func NewPolicy(trusted []Trust) Policy {
	entries := make(map[string]Trust, len(trusted))
	for _, entry := range trusted {
		entry.Problems = normalizeProblems(entry.Problems)
		entries[entry.Endpoint.key()] = entry
	}
	return Policy{trusted: entries, now: time.Now}
}

// Require returns a copy that requires the next handshake to present the exact
// certificate just approved. This closes the gap between prompt and retry.
func (p Policy) Require(trust Trust) Policy {
	trust.Problems = normalizeProblems(trust.Problems)
	p.required = &trust
	return p
}

// TLSConfig builds a TLS configuration that performs full x509 verification
// inside VerifyConnection so an exact approved exception can be considered.
func (p Policy) TLSConfig(endpoint Endpoint) *tls.Config {
	var mu sync.Mutex
	var controlFingerprint string
	return &tls.Config{
		ServerName:         endpoint.Hostname,
		MinVersion:         tls.VersionTLS12,
		MaxVersion:         tls.VersionTLS12,
		InsecureSkipVerify: true, //nolint:gosec // VerifyConnection below performs complete verification
		VerifyConnection: func(state tls.ConnectionState) error {
			mu.Lock()
			defer mu.Unlock()
			policy := p
			if controlFingerprint != "" {
				policy = p.Require(Trust{Endpoint: endpoint, Fingerprint: controlFingerprint})
			}
			if err := policy.verify(endpoint, state.PeerCertificates); err != nil {
				return err
			}
			if controlFingerprint == "" && len(state.PeerCertificates) > 0 {
				controlFingerprint = challengeFor(endpoint, state.PeerCertificates[0]).Fingerprint
			}
			return nil
		},
	}
}

func (p Policy) verify(endpoint Endpoint, certificates []*x509.Certificate) error {
	if len(certificates) == 0 {
		return errors.New("FTPS server sent no certificate")
	}
	leaf := certificates[0]
	challenge := challengeFor(endpoint, leaf)

	approved, hasApproval := p.trusted[endpoint.key()]
	changedBeforeRetry := p.required != nil && p.required.Endpoint.key() == endpoint.key() && p.required.Fingerprint != challenge.Fingerprint
	problems, verifyErr := verifyProblems(endpoint.Hostname, certificates, p.currentTime())
	if changedBeforeRetry {
		problems = append(problems, ProblemCertificateChange)
		problems = normalizeProblems(problems)
		challenge.PreviousSHA256 = p.required.Fingerprint
		if verifyErr == nil {
			verifyErr = errors.New("certificate changed before reconnect")
		}
	} else if verifyErr != nil && hasApproval && approved.Fingerprint != challenge.Fingerprint {
		problems = append(problems, ProblemCertificateChange)
		problems = normalizeProblems(problems)
		challenge.PreviousSHA256 = approved.Fingerprint
	}
	if verifyErr == nil {
		return nil
	}
	challenge.Problems = problems
	if len(problems) == 0 {
		return fmt.Errorf("verify FTPS certificate for %s: %w", endpoint.Address(), verifyErr)
	}

	if hasApproval && approved.Fingerprint == challenge.Fingerprint && sameProblems(approved.Problems, problems) {
		return nil
	}
	return &VerificationError{Challenge: challenge, cause: verifyErr}
}

func (p Policy) currentTime() time.Time {
	if p.now == nil {
		return time.Now()
	}
	return p.now()
}

func challengeFor(endpoint Endpoint, leaf *x509.Certificate) Challenge {
	sum := sha256.Sum256(leaf.Raw)
	fingerprint := strings.ToUpper(hex.EncodeToString(sum[:]))
	fingerprint = strings.Join(splitEvery(fingerprint, 2), ":")
	ips := make([]string, len(leaf.IPAddresses))
	for i, ip := range leaf.IPAddresses {
		ips[i] = ip.String()
	}
	return Challenge{
		Endpoint:    endpoint,
		Fingerprint: fingerprint,
		Subject:     boundedName(leaf.Subject.String()),
		Issuer:      boundedName(leaf.Issuer.String()),
		DNSNames:    boundedStrings(leaf.DNSNames),
		IPAddresses: boundedStrings(ips),
		NotBefore:   leaf.NotBefore,
		NotAfter:    leaf.NotAfter,
	}
}

func verifyProblems(hostname string, certificates []*x509.Certificate, now time.Time) ([]Problem, error) {
	leaf := certificates[0]
	intermediates := x509.NewCertPool()
	var problems []Problem
	validationTime := now
	latestStart := certificates[0].NotBefore
	earliestEnd := certificates[0].NotAfter
	for index, certificate := range certificates {
		if index > 0 {
			intermediates.AddCert(certificate)
		}
		if now.Before(certificate.NotBefore) {
			problems = append(problems, ProblemNotYetValid)
		}
		if now.After(certificate.NotAfter) {
			problems = append(problems, ProblemExpired)
		}
		if certificate.NotBefore.After(latestStart) {
			latestStart = certificate.NotBefore
		}
		if certificate.NotAfter.Before(earliestEnd) {
			earliestEnd = certificate.NotAfter
		}
	}
	if !latestStart.Before(earliestEnd) {
		return nil, errors.New("FTPS certificate chain has no common validity period")
	}
	if validationTime.Before(latestStart) || validationTime.After(earliestEnd) {
		validationTime = latestStart.Add(earliestEnd.Sub(latestStart) / 2)
	}
	if err := leaf.VerifyHostname(hostname); err != nil {
		problems = append(problems, ProblemHostnameMismatch)
	}

	standardOpts := x509.VerifyOptions{
		DNSName:       hostname,
		Intermediates: intermediates,
		KeyUsages:     []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
		CurrentTime:   now,
	}
	if _, err := leaf.Verify(standardOpts); err == nil {
		return nil, nil
	} else {
		chainOpts := standardOpts
		chainOpts.DNSName = ""
		chainOpts.CurrentTime = validationTime
		_, chainErr := leaf.Verify(chainOpts)
		if chainErr != nil {
			var unknown x509.UnknownAuthorityError
			if errors.As(chainErr, &unknown) {
				temporaryRoots := x509.NewCertPool()
				temporaryRoots.AddCert(certificates[len(certificates)-1])
				structuralOpts := chainOpts
				structuralOpts.Roots = temporaryRoots
				if _, structuralErr := leaf.Verify(structuralOpts); structuralErr != nil {
					return nil, errors.Join(err, structuralErr)
				}
				problems = append(problems, ProblemUnknownAuthority)
			} else {
				return nil, err
			}
		}
		problems = normalizeProblems(problems)
		if len(problems) == 0 {
			return nil, err
		}
		return problems, err
	}
}

func normalizeProblems(problems []Problem) []Problem {
	seen := make(map[Problem]struct{}, len(problems))
	out := make([]Problem, 0, len(problems))
	for _, problem := range problems {
		if _, exists := seen[problem]; exists {
			continue
		}
		seen[problem] = struct{}{}
		out = append(out, problem)
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

func sameProblems(left, right []Problem) bool {
	left = normalizeProblems(left)
	right = normalizeProblems(right)
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		if left[i] != right[i] {
			return false
		}
	}
	return true
}

const maxDisplayField = 512
const maxDisplayNames = 32

func boundedName(value string) string {
	value = strings.Map(func(r rune) rune {
		if r < 0x20 || r == 0x7f || r == '\x1b' {
			return -1
		}
		return r
	}, value)
	runes := []rune(value)
	if len(runes) > maxDisplayField {
		return string(runes[:maxDisplayField]) + "…"
	}
	return value
}

func boundedStrings(values []string) []string {
	if len(values) > maxDisplayNames {
		values = values[:maxDisplayNames]
	}
	out := make([]string, len(values))
	for i, value := range values {
		out[i] = boundedName(value)
	}
	return out
}

func splitEvery(value string, size int) []string {
	out := make([]string, 0, len(value)/size)
	for len(value) > 0 {
		n := min(size, len(value))
		out = append(out, value[:n])
		value = value[n:]
	}
	return out
}
