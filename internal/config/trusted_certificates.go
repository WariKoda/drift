package config

import (
	"errors"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/BurntSushi/toml"
)

// TrustedCertificate is a persistent, endpoint-scoped FTPS certificate
// exception. Problem values are interpreted by internal/tlstrust.
type TrustedCertificate struct {
	Protocol    string    `toml:"protocol"`
	Hostname    string    `toml:"hostname"`
	Port        int       `toml:"port"`
	Fingerprint string    `toml:"fingerprint"`
	Problems    []string  `toml:"problems"`
	TrustedAt   time.Time `toml:"trusted_at"`
}

type trustedCertificateFile struct {
	Certificates []TrustedCertificate `toml:"certificates"`
}

var trustedCertificateMu sync.Mutex

// LoadTrustedCertificates reads all persistent certificate exceptions.
func LoadTrustedCertificates() ([]TrustedCertificate, error) {
	entries, err := loadTrustedCertificateFile()
	if err != nil {
		return nil, fmt.Errorf("load trusted certificates: %w", err)
	}
	return append([]TrustedCertificate(nil), entries...), nil
}

// SaveTrustedCertificate replaces the exception for entry's endpoint.
func SaveTrustedCertificate(entry TrustedCertificate) error {
	if err := validateTrustedCertificate(entry); err != nil {
		return fmt.Errorf("validate trusted certificate: %w", err)
	}
	return withTrustedCertificateLock(func() error {
		entries, err := loadTrustedCertificateFile()
		if err != nil {
			return fmt.Errorf("reload trusted certificates: %w", err)
		}
		out := make([]TrustedCertificate, 0, len(entries)+1)
		for _, existing := range entries {
			if sameTrustedEndpoint(existing, entry.Protocol, entry.Hostname, entry.Port) {
				continue
			}
			out = append(out, existing)
		}
		out = append(out, entry)
		sortTrustedCertificates(out)
		return writeTrustedCertificates(out)
	})
}

// DeleteTrustedCertificate removes the persistent exception for an endpoint.
func DeleteTrustedCertificate(protocol, hostname string, port int) error {
	return withTrustedCertificateLock(func() error {
		entries, err := loadTrustedCertificateFile()
		if err != nil {
			return fmt.Errorf("reload trusted certificates: %w", err)
		}
		out := make([]TrustedCertificate, 0, len(entries))
		for _, entry := range entries {
			if sameTrustedEndpoint(entry, protocol, hostname, port) {
				continue
			}
			out = append(out, entry)
		}
		if len(out) == len(entries) {
			return nil
		}
		return writeTrustedCertificates(out)
	})
}

func loadTrustedCertificateFile() ([]TrustedCertificate, error) {
	path := trustedCertificatesPath()
	var file trustedCertificateFile
	if _, err := toml.DecodeFile(path, &file); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return []TrustedCertificate{}, nil
		}
		return nil, fmt.Errorf("read trusted certificates: %w", err)
	}
	seen := make(map[string]struct{}, len(file.Certificates))
	for index, entry := range file.Certificates {
		if err := validateTrustedCertificate(entry); err != nil {
			return nil, fmt.Errorf("trusted certificate %d: %w", index+1, err)
		}
		key := trustedEndpointKey(entry.Protocol, entry.Hostname, entry.Port)
		if _, exists := seen[key]; exists {
			return nil, fmt.Errorf("trusted certificate %d duplicates endpoint %s", index+1, netAddress(entry.Hostname, entry.Port))
		}
		seen[key] = struct{}{}
	}
	sortTrustedCertificates(file.Certificates)
	return file.Certificates, nil
}

func writeTrustedCertificates(entries []TrustedCertificate) error {
	path := trustedCertificatesPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return fmt.Errorf("create config directory: %w", err)
	}
	if err := WriteTOML(path, trustedCertificateFile{Certificates: entries}); err != nil {
		return fmt.Errorf("write trusted certificates: %w", err)
	}
	return nil
}

func validateTrustedCertificate(entry TrustedCertificate) error {
	if entry.Protocol != "ftps" {
		return fmt.Errorf("unsupported protocol %q", entry.Protocol)
	}
	if strings.TrimSpace(entry.Hostname) == "" {
		return errors.New("hostname is required")
	}
	if entry.Port < 1 || entry.Port > 65535 {
		return fmt.Errorf("invalid port %d", entry.Port)
	}
	parts := strings.Split(entry.Fingerprint, ":")
	if len(parts) != 32 {
		return errors.New("fingerprint must contain 32 SHA-256 bytes")
	}
	for _, part := range parts {
		if len(part) != 2 || strings.Trim(part, "0123456789ABCDEF") != "" {
			return errors.New("fingerprint must be uppercase colon-separated SHA-256")
		}
	}
	if len(entry.Problems) == 0 {
		return errors.New("at least one certificate problem is required")
	}
	seen := make(map[string]struct{}, len(entry.Problems))
	for _, problem := range entry.Problems {
		switch problem {
		case "unknown_authority", "hostname_mismatch", "expired", "not_yet_valid":
		default:
			return fmt.Errorf("unknown certificate problem %q", problem)
		}
		if _, exists := seen[problem]; exists {
			return fmt.Errorf("duplicate certificate problem %q", problem)
		}
		seen[problem] = struct{}{}
	}
	if entry.TrustedAt.IsZero() {
		return errors.New("trusted_at is required")
	}
	return nil
}

func sameTrustedEndpoint(entry TrustedCertificate, protocol, hostname string, port int) bool {
	return entry.Protocol == protocol && entry.Hostname == hostname && entry.Port == port
}

func trustedEndpointKey(protocol, hostname string, port int) string {
	return protocol + "\x00" + hostname + "\x00" + fmt.Sprintf("%d", port)
}

func sortTrustedCertificates(entries []TrustedCertificate) {
	sort.Slice(entries, func(i, j int) bool {
		return trustedEndpointKey(entries[i].Protocol, entries[i].Hostname, entries[i].Port) <
			trustedEndpointKey(entries[j].Protocol, entries[j].Hostname, entries[j].Port)
	})
}

func trustedCertificatesPath() string {
	return filepath.Join(Dir(), "trusted-certificates.toml")
}

func trustedCertificatesLockPath() string {
	return trustedCertificatesPath() + ".lock"
}

func withTrustedCertificateLock(fn func() error) error {
	trustedCertificateMu.Lock()
	defer trustedCertificateMu.Unlock()

	if err := os.MkdirAll(Dir(), 0o700); err != nil {
		return fmt.Errorf("create config directory: %w", err)
	}
	lockPath := trustedCertificatesLockPath()
	deadline := time.Now().Add(5 * time.Second)
	for {
		err := os.Mkdir(lockPath, 0o700)
		if err == nil {
			break
		}
		if !errors.Is(err, os.ErrExist) {
			return fmt.Errorf("lock trusted certificates: %w", err)
		}
		info, statErr := os.Stat(lockPath)
		if statErr == nil && time.Since(info.ModTime()) > 30*time.Second {
			_ = os.RemoveAll(lockPath)
			continue
		}
		if time.Now().After(deadline) {
			return errors.New("timed out locking trusted certificates")
		}
		time.Sleep(10 * time.Millisecond)
	}
	defer os.RemoveAll(lockPath)
	if err := fn(); err != nil {
		return fmt.Errorf("update trusted certificates: %w", err)
	}
	return nil
}

func netAddress(hostname string, port int) string {
	return net.JoinHostPort(hostname, strconv.Itoa(port))
}
