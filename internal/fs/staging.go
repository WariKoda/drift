package fs

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"strings"
)

const stagingMarker = ".drift-tmp-"

// StagingName returns an unpredictable hidden name for a write of base that is
// in progress. Staging files live beside their target so the final rename
// stays within one directory; an interrupted transfer can leave one behind.
func StagingName(base string) (string, error) {
	var token [16]byte
	if _, err := rand.Read(token[:]); err != nil {
		return "", fmt.Errorf("generate staging name: %w", err)
	}
	return "." + base + stagingMarker + hex.EncodeToString(token[:]), nil
}

// IsStagingName reports whether name has the form StagingName produces.
func IsStagingName(name string) bool {
	i := strings.LastIndex(name, stagingMarker)
	if i < 2 || name[0] != '.' {
		return false
	}
	token := name[i+len(stagingMarker):]
	if len(token) != 32 {
		return false
	}
	_, err := hex.DecodeString(token)
	return err == nil
}
