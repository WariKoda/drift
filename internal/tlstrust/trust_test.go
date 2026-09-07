package tlstrust

import (
	"crypto/rand"
	"crypto/rsa"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"errors"
	"math/big"
	"slices"
	"strings"
	"testing"
	"time"
)

func TestNormalizeEndpoint(t *testing.T) {
	endpoint, err := NormalizeEndpoint("FTPS", "Example.COM.", 0)
	if err != nil {
		t.Fatalf("normalize endpoint: %v", err)
	}
	if endpoint != (Endpoint{Protocol: "ftps", Hostname: "example.com", Port: 21}) {
		t.Fatalf("endpoint = %+v", endpoint)
	}

	ipEndpoint, err := NormalizeEndpoint("ftps", "[2001:0db8::1]", 990)
	if err != nil {
		t.Fatalf("normalize IP endpoint: %v", err)
	}
	if ipEndpoint.Hostname != "2001:db8::1" {
		t.Fatalf("IP hostname = %q", ipEndpoint.Hostname)
	}
}

func TestPolicyRequiresExplicitTrustForExactCertificate(t *testing.T) {
	now := time.Now().UTC().Truncate(time.Second)
	certificate := makeCertificate(t, "wrong.example", now.Add(-time.Hour), now.Add(time.Hour))
	endpoint, err := NormalizeEndpoint("ftps", "server.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	policy := NewPolicy(nil)
	policy.now = func() time.Time { return now }

	err = policy.verify(endpoint, []*x509.Certificate{certificate})
	var verifyErr *VerificationError
	if !errorsAs(err, &verifyErr) {
		t.Fatalf("error = %T %v, want VerificationError", err, err)
	}
	wantProblems := []Problem{ProblemHostnameMismatch, ProblemUnknownAuthority}
	if !sameProblems(verifyErr.Challenge.Problems, wantProblems) {
		t.Fatalf("problems = %v, want %v", verifyErr.Challenge.Problems, wantProblems)
	}

	trust := Trust{
		Endpoint:    endpoint,
		Fingerprint: verifyErr.Challenge.Fingerprint,
		Problems:    verifyErr.Challenge.Problems,
		TrustedAt:   now,
	}
	trustedPolicy := NewPolicy([]Trust{trust})
	trustedPolicy.now = func() time.Time { return now }
	if err := trustedPolicy.verify(endpoint, []*x509.Certificate{certificate}); err != nil {
		t.Fatalf("trusted verification: %v", err)
	}

	otherEndpoint, err := NormalizeEndpoint("ftps", "server.example", 2121)
	if err != nil {
		t.Fatal(err)
	}
	if err := trustedPolicy.verify(otherEndpoint, []*x509.Certificate{certificate}); err == nil {
		t.Fatal("trust unexpectedly applied to a different port")
	}
}

func TestPolicyDoesNotIgnoreNewProblem(t *testing.T) {
	now := time.Now().UTC().Truncate(time.Second)
	certificate := makeCertificate(t, "server.example", now.Add(-2*time.Hour), now.Add(-time.Hour))
	endpoint, err := NormalizeEndpoint("ftps", "server.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	challenge := challengeFor(endpoint, certificate)
	policy := NewPolicy([]Trust{{
		Endpoint:    endpoint,
		Fingerprint: challenge.Fingerprint,
		Problems:    []Problem{ProblemUnknownAuthority},
	}})
	policy.now = func() time.Time { return now }

	err = policy.verify(endpoint, []*x509.Certificate{certificate})
	var verifyErr *VerificationError
	if !errorsAs(err, &verifyErr) {
		t.Fatalf("error = %T %v, want VerificationError", err, err)
	}
	wantProblems := []Problem{ProblemExpired, ProblemUnknownAuthority}
	if !sameProblems(verifyErr.Challenge.Problems, wantProblems) {
		t.Fatalf("problems = %v, want %v", verifyErr.Challenge.Problems, wantProblems)
	}
}

func TestStoredExceptionReportsChangedInvalidCertificate(t *testing.T) {
	now := time.Now().UTC().Truncate(time.Second)
	first := makeCertificate(t, "server.example", now.Add(-time.Hour), now.Add(time.Hour))
	second := makeCertificate(t, "server.example", now.Add(-time.Hour), now.Add(time.Hour))
	endpoint, err := NormalizeEndpoint("ftps", "server.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	firstChallenge := challengeFor(endpoint, first)
	policy := NewPolicy([]Trust{{
		Endpoint: endpoint, Fingerprint: firstChallenge.Fingerprint,
		Problems: []Problem{ProblemUnknownAuthority},
	}})
	policy.now = func() time.Time { return now }

	err = policy.verify(endpoint, []*x509.Certificate{second})
	var verifyErr *VerificationError
	if !errorsAs(err, &verifyErr) {
		t.Fatalf("error = %T %v, want VerificationError", err, err)
	}
	if !sameProblems(verifyErr.Challenge.Problems, []Problem{ProblemCertificateChange, ProblemUnknownAuthority}) {
		t.Fatalf("problems = %v", verifyErr.Challenge.Problems)
	}
	if verifyErr.Challenge.PreviousSHA256 != firstChallenge.Fingerprint {
		t.Fatalf("previous fingerprint = %q", verifyErr.Challenge.PreviousSHA256)
	}
}

func TestRequiredCertificateRejectsChangeBeforeRetry(t *testing.T) {
	now := time.Now().UTC().Truncate(time.Second)
	first := makeCertificate(t, "server.example", now.Add(-time.Hour), now.Add(time.Hour))
	second := makeCertificate(t, "server.example", now.Add(-time.Hour), now.Add(time.Hour))
	endpoint, err := NormalizeEndpoint("ftps", "server.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	firstChallenge := challengeFor(endpoint, first)
	trust := Trust{Endpoint: endpoint, Fingerprint: firstChallenge.Fingerprint, Problems: []Problem{ProblemUnknownAuthority}}
	policy := NewPolicy([]Trust{trust}).Require(trust)
	policy.now = func() time.Time { return now }

	err = policy.verify(endpoint, []*x509.Certificate{second})
	var verifyErr *VerificationError
	if !errorsAs(err, &verifyErr) {
		t.Fatalf("error = %T %v, want VerificationError", err, err)
	}
	if !sameProblems(verifyErr.Challenge.Problems, []Problem{ProblemCertificateChange, ProblemUnknownAuthority}) {
		t.Fatalf("problems = %v", verifyErr.Challenge.Problems)
	}
	if verifyErr.Challenge.PreviousSHA256 != firstChallenge.Fingerprint {
		t.Fatalf("previous fingerprint = %q", verifyErr.Challenge.PreviousSHA256)
	}
}

func TestUnknownAuthorityDoesNotHideUnsupportedUsage(t *testing.T) {
	now := time.Now().UTC().Truncate(time.Second)
	certificate := makeCertificate(t, "server.example", now.Add(-time.Hour), now.Add(time.Hour))
	certificate.ExtKeyUsage = []x509.ExtKeyUsage{x509.ExtKeyUsageClientAuth}
	endpoint, err := NormalizeEndpoint("ftps", "server.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	policy := NewPolicy(nil)
	policy.now = func() time.Time { return now }
	err = policy.verify(endpoint, []*x509.Certificate{certificate})
	var verificationErr *VerificationError
	if errorsAs(err, &verificationErr) {
		t.Fatalf("unsupported key usage was offered as trustable: %v", err)
	}
	if err == nil {
		t.Fatal("unsupported key usage was accepted")
	}
}

func TestTLSConfigRejectsDifferentDataCertificate(t *testing.T) {
	now := time.Now().UTC().Truncate(time.Second)
	control := makeCertificate(t, "server.example", now.Add(-time.Hour), now.Add(time.Hour))
	data := makeCertificate(t, "server.example", now.Add(-time.Hour), now.Add(time.Hour))
	endpoint, err := NormalizeEndpoint("ftps", "server.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	controlChallenge := challengeFor(endpoint, control)
	policy := NewPolicy([]Trust{{
		Endpoint: endpoint, Fingerprint: controlChallenge.Fingerprint,
		Problems: []Problem{ProblemUnknownAuthority},
	}})
	config := policy.TLSConfig(endpoint)
	if err := config.VerifyConnection(tls.ConnectionState{PeerCertificates: []*x509.Certificate{control}}); err != nil {
		t.Fatalf("control certificate: %v", err)
	}
	err = config.VerifyConnection(tls.ConnectionState{PeerCertificates: []*x509.Certificate{data}})
	var verificationErr *VerificationError
	if !errorsAs(err, &verificationErr) {
		t.Fatalf("data certificate error = %T %v", err, err)
	}
	if !slices.Contains(verificationErr.Challenge.Problems, ProblemCertificateChange) {
		t.Fatalf("problems = %v", verificationErr.Challenge.Problems)
	}
	if verificationErr.Challenge.PreviousSHA256 != controlChallenge.Fingerprint {
		t.Fatalf("previous fingerprint = %q", verificationErr.Challenge.PreviousSHA256)
	}
}

func TestChallengeSanitizesCertificateNames(t *testing.T) {
	now := time.Now().UTC()
	certificate := makeCertificate(t, "server.example\x1b[31m", now.Add(-time.Hour), now.Add(time.Hour))
	endpoint, err := NormalizeEndpoint("ftps", "server.example", 21)
	if err != nil {
		t.Fatal(err)
	}
	challenge := challengeFor(endpoint, certificate)
	if containsEscape(challenge.Subject) || containsEscape(challenge.DNSNames[0]) {
		t.Fatalf("challenge contains terminal escape: %+v", challenge)
	}
}

func makeCertificate(t *testing.T, dnsName string, notBefore, notAfter time.Time) *x509.Certificate {
	t.Helper()
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		t.Fatalf("generate key: %v", err)
	}
	template := &x509.Certificate{
		SerialNumber:          big.NewInt(notBefore.UnixNano()),
		Subject:               pkix.Name{CommonName: dnsName},
		DNSNames:              []string{dnsName},
		NotBefore:             notBefore,
		NotAfter:              notAfter,
		KeyUsage:              x509.KeyUsageDigitalSignature,
		ExtKeyUsage:           []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
		IsCA:                  true,
		BasicConstraintsValid: true,
	}
	raw, err := x509.CreateCertificate(rand.Reader, template, template, &key.PublicKey, key)
	if err != nil {
		t.Fatalf("create certificate: %v", err)
	}
	certificate, err := x509.ParseCertificate(raw)
	if err != nil {
		t.Fatalf("parse certificate: %v", err)
	}
	return certificate
}

func errorsAs(err error, target any) bool {
	return errors.As(err, target)
}

func containsEscape(value string) bool {
	return strings.ContainsRune(value, '\x1b')
}
