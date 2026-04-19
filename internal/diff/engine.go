package diff

import (
	"errors"
	"net/textproto"
	"os"
	"strings"

	ftplib "github.com/jlaffaye/ftp"
	"github.com/sergi/go-diff/diffmatchpatch"
)

const maxTextSize = 2 * 1024 * 1024 // 2 MB

// RemoteClient is the subset of remote operations needed for diffing.
type RemoteClient interface {
	Stat(path string) (os.FileInfo, error)
	ReadFile(path string) ([]byte, error)
}

// Compare produces a DiffResult for a local/remote file pair.
func Compare(localPath, remotePath string, client RemoteClient) (*DiffResult, error) {
	result := &DiffResult{
		LocalPath:  localPath,
		RemotePath: remotePath,
	}

	// Stat local
	localInfo, localErr := os.Stat(localPath)
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
							RemoteLine: line, Kind: LineAdded, RemoteNum: i + 1,
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
			if data, err := os.ReadFile(localPath); err == nil {
				if isBinary(data) {
					result.Binary = true
				} else {
					for i, line := range splitLines(string(data)) {
						result.Lines = append(result.Lines, DiffLine{
							LocalLine: line, Kind: LineRemoved, LocalNum: i + 1,
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

	if result.SizeLocal > maxTextSize || result.SizeRemote > maxTextSize {
		result.Binary = true
		return result, nil
	}

	localData, err := os.ReadFile(localPath)
	if err != nil {
		return result, err
	}
	remoteData, err := client.ReadFile(remotePath)
	if err != nil {
		return result, err
	}

	if isBinary(localData) || isBinary(remoteData) {
		result.Binary = true
		return result, nil
	}

	result.Lines = lineDiff(string(localData), string(remoteData))
	return result, nil
}

// lineDiff computes a side-by-side line diff between local and remote text.
func lineDiff(local, remote string) []DiffLine {
	dmp := diffmatchpatch.New()
	a, b, lines := dmp.DiffLinesToChars(local, remote)
	diffs := dmp.DiffMain(a, b, false)
	diffs = dmp.DiffCharsToLines(diffs, lines)

	var result []DiffLine
	localNum := 1
	remoteNum := 1

	for _, d := range diffs {
		texts := splitLines(d.Text)
		switch d.Type {
		case diffmatchpatch.DiffEqual:
			for _, t := range texts {
				result = append(result, DiffLine{
					LocalLine: t, RemoteLine: t,
					Kind:     LineEqual,
					LocalNum: localNum, RemoteNum: remoteNum,
				})
				localNum++
				remoteNum++
			}
		case diffmatchpatch.DiffDelete:
			for _, t := range texts {
				result = append(result, DiffLine{
					LocalLine: t,
					Kind:      LineRemoved,
					LocalNum:  localNum,
				})
				localNum++
			}
		case diffmatchpatch.DiffInsert:
			for _, t := range texts {
				result = append(result, DiffLine{
					RemoteLine: t,
					Kind:       LineAdded,
					RemoteNum:  remoteNum,
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
