package diff

import (
	"bytes"
	"crypto/sha256"
	"errors"
	"fmt"
	"io"
	"net/textproto"
	"os"
	"strings"

	ftplib "github.com/jlaffaye/ftp"
	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/WariKoda/drift/internal/fs"
)

const maxTextSize = 2 * 1024 * 1024 // 2 MB

// RemoteClient is the subset of remote operations needed for diffing.
type RemoteClient interface {
	Stat(path string) (os.FileInfo, error)
	Open(path string) (io.ReadCloser, error)
	ReadFile(path string) ([]byte, error)
}

// Compare produces a DiffResult for a local/remote file pair. Local reads go
// through root, so a local path that only looks like a project path — because a
// symlinked component points elsewhere — fails instead of pulling in a file
// from outside the project.
func Compare(root *fs.Root, localPath, remotePath string, client RemoteClient) (*DiffResult, error) {
	result := &DiffResult{
		LocalPath:  localPath,
		RemotePath: remotePath,
	}

	// Stat local
	localInfo, localErr := root.Stat(localPath)
	if localErr == nil {
		result.SizeLocal = localInfo.Size()
		result.ModLocal = localInfo.ModTime()
	}

	// Stat remote
	remoteInfo, remoteErr := client.Stat(remotePath)
	if remoteErr == nil {
		result.SizeRemote = remoteInfo.Size()
		result.ModRemote = remoteInfo.ModTime()
	}

	localMissing := isNotExistError(localErr)
	remoteMissing := isNotExistError(remoteErr)
	localExists := localErr == nil
	remoteExists := remoteErr == nil

	if localErr != nil && !localMissing {
		return result, localErr
	}
	if remoteErr != nil && !remoteMissing {
		return result, remoteErr
	}

	if localMissing && remoteMissing {
		return result, nil
	}
	if localMissing {
		result.RemoteOnly = true
		if result.SizeRemote <= maxTextSize {
			if data, err := client.ReadFile(remotePath); err == nil {
				if isBinary(data) {
					result.Binary = true
				} else {
					for i, line := range splitLines(string(data)) {
						result.Lines = append(result.Lines, DiffLine{
							Text: line, Kind: LineAdded, RemoteNum: i + 1,
						})
					}
				}
			}
		} else {
			result.Binary = true
		}
		return result, nil
	}
	if remoteMissing {
		result.LocalOnly = true
		if result.SizeLocal <= maxTextSize {
			if data, err := root.ReadFile(localPath); err == nil {
				if isBinary(data) {
					result.Binary = true
				} else {
					for i, line := range splitLines(string(data)) {
						result.Lines = append(result.Lines, DiffLine{
							Text: line, Kind: LineRemoved, LocalNum: i + 1,
						})
					}
				}
			}
		} else {
			result.Binary = true
		}
		return result, nil
	}

	// Both exist — read content
	if !localExists || !remoteExists {
		return result, nil
	}

	// Fast path: when size and modification time match exactly, treat the files
	// as unchanged without downloading the remote content.
	if result.SizeLocal == result.SizeRemote && !result.ModLocal.IsZero() && result.ModLocal.Equal(result.ModRemote) {
		return result, nil
	}

	if result.SizeLocal > maxTextSize || result.SizeRemote > maxTextSize {
		result.Binary = true
		if result.SizeLocal != result.SizeRemote {
			result.ContentDiff = true
			return result, nil
		}

		equal, err := contentEqual(root, localPath, remotePath, client)
		if err != nil {
			return result, err
		}
		result.ContentDiff = !equal
		return result, nil
	}

	localData, err := root.ReadFile(localPath)
	if err != nil {
		return result, err
	}
	remoteData, err := client.ReadFile(remotePath)
	if err != nil {
		return result, err
	}

	if isBinary(localData) || isBinary(remoteData) {
		result.Binary = true
		result.ContentDiff = !bytes.Equal(localData, remoteData)
		return result, nil
	}

	result.Lines = lineDiff(string(localData), string(remoteData))
	return result, nil
}

// contentEqual compares local and remote content with constant memory. It is
// used when a line diff would be too expensive to build.
func contentEqual(root *fs.Root, localPath, remotePath string, client RemoteClient) (bool, error) {
	localFile, err := root.Open(localPath)
	if err != nil {
		return false, err
	}
	localSum, localSize, err := digestAndClose(localFile)
	if err != nil {
		return false, fmt.Errorf("hash local file %s: %w", localPath, err)
	}

	remoteFile, err := client.Open(remotePath)
	if err != nil {
		return false, err
	}
	remoteSum, remoteSize, err := digestAndClose(remoteFile)
	if err != nil {
		return false, fmt.Errorf("hash remote file %s: %w", remotePath, err)
	}

	return localSize == remoteSize && localSum == remoteSum, nil
}

func digestAndClose(r io.ReadCloser) ([sha256.Size]byte, int64, error) {
	h := sha256.New()
	size, readErr := io.Copy(h, r)
	closeErr := r.Close()

	var sum [sha256.Size]byte
	copy(sum[:], h.Sum(nil))
	return sum, size, errors.Join(readErr, closeErr)
}

// lineDiff computes a unified line diff between local and remote text.
// Removals and additions are emitted as separate rows (never paired into a
// single modified row).
func lineDiff(local, remote string) []DiffLine {
	dmp := diffmatchpatch.New()
	a, b, lines := dmp.DiffLinesToChars(local, remote)
	diffs := dmp.DiffMain(a, b, false)
	diffs = dmp.DiffCharsToLines(diffs, lines)

	// Pre-size the result: counting newlines on both sides is a cheap O(n) pass
	// that avoids repeated slice growth/copy as rows are appended below.
	result := make([]DiffLine, 0, strings.Count(local, "\n")+strings.Count(remote, "\n")+1)
	localNum := 1
	remoteNum := 1

	for _, d := range diffs {
		switch d.Type {
		case diffmatchpatch.DiffEqual:
			for _, t := range splitLines(d.Text) {
				result = append(result, DiffLine{
					Text: t, Kind: LineEqual,
					LocalNum: localNum, RemoteNum: remoteNum,
				})
				localNum++
				remoteNum++
			}
		case diffmatchpatch.DiffDelete:
			for _, t := range splitLines(d.Text) {
				result = append(result, DiffLine{
					Text: t, Kind: LineRemoved, LocalNum: localNum,
				})
				localNum++
			}
		case diffmatchpatch.DiffInsert:
			for _, t := range splitLines(d.Text) {
				result = append(result, DiffLine{
					Text: t, Kind: LineAdded, RemoteNum: remoteNum,
				})
				remoteNum++
			}
		}
	}
	return result
}

// splitLines splits text into lines, discarding a trailing empty line.
func splitLines(s string) []string {
	lines := strings.Split(s, "\n")
	if len(lines) > 0 && lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	return lines
}

// isBinary returns true if data appears to be a binary file.
func isBinary(data []byte) bool {
	n := len(data)
	if n > 512 {
		n = 512
	}
	for _, b := range data[:n] {
		if b == 0 {
			return true
		}
	}
	return false
}

func isNotExistError(err error) bool {
	if err == nil {
		return false
	}
	if errors.Is(err, os.ErrNotExist) || os.IsNotExist(err) {
		return true
	}

	var protoErr *textproto.Error
	if errors.As(err, &protoErr) {
		return protoErr.Code == ftplib.StatusFileUnavailable
	}

	return false
}
