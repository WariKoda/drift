package main

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/pem"
	"math/big"
	"net"
	"os"
	"path/filepath"
	"time"
)

// Certificates are generated for actual TLS handshakes, including invalid
// chains. No client verifier or transport is substituted in these tests.
func (s *server) prepareTLS(mode string) error {
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return err
	}
	now := time.Now()
	root := &x509.Certificate{SerialNumber: big.NewInt(1), Subject: pkix.Name{CommonName: "drift test CA"}, NotBefore: now.Add(-48 * time.Hour), NotAfter: now.Add(48 * time.Hour), IsCA: true, BasicConstraintsValid: true, KeyUsage: x509.KeyUsageCertSign}
	rootDER, err := x509.CreateCertificate(rand.Reader, root, root, &key.PublicKey, key)
	if err != nil {
		return err
	}
	if err = os.WriteFile(filepath.Join(s.control, "root.der"), rootDER, 0600); err != nil {
		return err
	}
	if err = os.WriteFile(filepath.Join(s.control, "root.pem"), pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: rootDER}), 0600); err != nil {
		return err
	}
	for i := range 2 {
		leafKey, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
		if err != nil {
			return err
		}
		leaf := &x509.Certificate{SerialNumber: big.NewInt(int64(10 + i)), Subject: pkix.Name{CommonName: "drift FTPS test"}, NotBefore: now.Add(-time.Hour), NotAfter: now.Add(time.Hour), IPAddresses: []net.IP{net.ParseIP("127.0.0.1")}, DNSNames: []string{"localhost"}, KeyUsage: x509.KeyUsageDigitalSignature, ExtKeyUsage: []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth}, BasicConstraintsValid: true}
		switch mode {
		case "expired":
			leaf.NotBefore = now.Add(-24 * time.Hour)
			leaf.NotAfter = now.Add(-time.Hour)
		case "future":
			leaf.NotBefore = now.Add(time.Hour)
			leaf.NotAfter = now.Add(24 * time.Hour)
		case "wrong-name":
			leaf.IPAddresses = nil
			leaf.DNSNames = []string{"other.invalid"}
		case "bad-usage":
			leaf.ExtKeyUsage = []x509.ExtKeyUsage{x509.ExtKeyUsageClientAuth}
		}
		parent, signer := root, key
		if mode == "self-signed" {
			parent, signer = leaf, leafKey
		}
		der, err := x509.CreateCertificate(rand.Reader, leaf, parent, &leafKey.PublicKey, signer)
		if err != nil {
			return err
		}
		if mode == "bad-signature" {
			der[len(der)-1] ^= 1
		}
		chain := [][]byte{der, rootDER}
		if mode == "self-signed" {
			chain = [][]byte{der}
		}
		s.certificates[i] = tls.Certificate{Certificate: chain, PrivateKey: leafKey}
		if err = os.WriteFile(filepath.Join(s.control, []string{"leaf.der", "rotated.der"}[i]), der, 0600); err != nil {
			return err
		}
	}
	return nil
}
func (s *server) secure(conn net.Conn, data bool) (net.Conn, error) {
	index := 0
	if s.flag("rotate-cert") || (data && s.flag("rotate-data")) {
		index = 1
	}
	secure := tls.Server(conn, &tls.Config{MinVersion: tls.VersionTLS12, MaxVersion: tls.VersionTLS12, Certificates: []tls.Certificate{s.certificates[index]}})
	if err := secure.Handshake(); err != nil {
		_ = conn.Close()
		return nil, err
	}
	s.record("TLS", "1.2")
	return secure, nil
}
