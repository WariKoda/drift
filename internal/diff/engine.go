package diff

import (
	"os"
	"strings"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/yourusername/drift/internal/sftp"
)

const maxTextSize = 2 * 1024 * 1024 // 2 MB

// Compare produces a DiffResult for a local/remote file pair.
// sftpClient may be nil only when the remote file is expected to not exist.
func Compare(localPath, remotePath string, client *sftp.Client) (*DiffResult, error) {
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

	localExists := localErr == nil
	remoteExists := remoteErr == nil

	if !localExists && !remoteExists {
		return result, nil
	}
	if !localExists {
		result.RemoteOnly = true
		// populate lines from remote content for display
		if data, err := client.ReadFile(remotePath); err == nil {
			for i, line := range splitLines(string(data)) {
				result.Lines = append(result.Lines, DiffLine{
					RemoteLine: line, Kind: LineAdded, RemoteNum: i + 1,
				})
			}
		}
		return result, nil
	}
	if !remoteExists {
		result.LocalOnly = true
		if data, err := os.ReadFile(localPath); err == nil {
			for i, line := range splitLines(string(data)) {
				result.Lines = append(result.Lines, DiffLine{
					LocalLine: line, Kind: LineRemoved, LocalNum: i + 1,
				})
			}
		}
		return result, nil
	}

	// Both exist — read content
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
					Kind:      LineEqual,
					LocalNum:  localNum, RemoteNum: remoteNum,
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
