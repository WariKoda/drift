package diff

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

// RenderPanes converts a DiffResult into two parallel slices of rendered strings
// (left = local, right = remote), each line padded to paneWidth columns.
// scrollOffset is the first DiffLine index to render; count is the number of rows.
//
// flip makes the colouring sync-direction-aware: with flip=false (download /
// neutral) the local side is treated as old and the remote side as new; with
// flip=true (upload) local is the new content being pushed, so local additions
// turn green/+ and remote-only lines turn red/-.
func RenderPanes(result *DiffResult, paneWidth, scrollOffset, count int, flip bool) (left, right []string) {
	if result == nil {
		for i := 0; i < count; i++ {
			left = append(left, strings.Repeat(" ", paneWidth))
			right = append(right, strings.Repeat(" ", paneWidth))
		}
		return
	}

	if result.Binary {
		localMeta := fmt.Sprintf("  binary file  %s  %d bytes", result.ModLocal.Format("2006-01-02 15:04"), result.SizeLocal)
		remoteMeta := fmt.Sprintf("  binary file  %s  %d bytes", result.ModRemote.Format("2006-01-02 15:04"), result.SizeRemote)
		left = append(left, pad(styles.Muted.Render(localMeta), paneWidth))
		right = append(right, pad(styles.Muted.Render(remoteMeta), paneWidth))
		for i := 1; i < count; i++ {
			left = append(left, strings.Repeat(" ", paneWidth))
			right = append(right, strings.Repeat(" ", paneWidth))
		}
		return
	}

	lines := result.Lines
	numWidth := 4 // characters for line number column

	for i := 0; i < count; i++ {
		idx := scrollOffset + i
		if idx >= len(lines) {
			left = append(left, strings.Repeat(" ", paneWidth))
			right = append(right, strings.Repeat(" ", paneWidth))
			continue
		}

		dl := lines[idx]
		// layout per cell: marker(1) + " " + lineNum + " " + content
		contentWidth := paneWidth - numWidth - 3
		if contentWidth < 1 {
			contentWidth = 1
		}

		left = append(left, renderSide(dl.LocalLine, dl.LocalNum, dl.Kind, true, flip, paneWidth, numWidth, contentWidth))
		right = append(right, renderSide(dl.RemoteLine, dl.RemoteNum, dl.Kind, false, flip, paneWidth, numWidth, contentWidth))
	}
	return
}

// sideAct is what a single pane cell represents after applying sync direction.
type sideAct int

const (
	actEqual sideAct = iota
	actAdd
	actRemove
	actBlank
)

// sideAction resolves how one side of a diff line should be rendered, taking the
// sync direction (flip) into account. flip=false treats remote as the new state.
func sideAction(kind LineKind, isLocal, flip bool) sideAct {
	switch kind {
	case LineRemoved:
		if !isLocal {
			return actBlank
		}
		if flip {
			return actAdd
		}
		return actRemove
	case LineAdded:
		if isLocal {
			return actBlank
		}
		if flip {
			return actRemove
		}
		return actAdd
	case LineModified:
		// local = old, remote = new (download / neutral); flipped for upload.
		if isLocal == flip {
			return actAdd
		}
		return actRemove
	default: // LineEqual
		return actEqual
	}
}

func renderSide(text string, lineNum int, kind LineKind, isLocal, flip bool, paneWidth, numWidth, contentWidth int) string {
	act := sideAction(kind, isLocal, flip)

	// Line number (blank when this side has no content).
	var numStr string
	if act == actBlank || lineNum <= 0 {
		numStr = strings.Repeat(" ", numWidth)
	} else {
		numStr = fmt.Sprintf("%*d", numWidth, lineNum)
	}

	// marker is the git-style gutter sign: "-" removed, "+" added, " " otherwise.
	var bgColor lipgloss.TerminalColor
	var hasBg bool
	var textColor lipgloss.Style
	marker := " "

	switch act {
	case actAdd:
		bgColor, hasBg = styles.ColorDiffAddedBg, true
		textColor = lipgloss.NewStyle().Foreground(styles.ColorDiffAddedText)
		marker = "+"
	case actRemove:
		bgColor, hasBg = styles.ColorDiffRemovedBg, true
		textColor = lipgloss.NewStyle().Foreground(styles.ColorDiffRemovedText)
		marker = "-"
	case actBlank:
		bgColor, hasBg = styles.ColorDiffMissingBg, true
		textColor = styles.Muted
		text = ""
	default: // actEqual
		textColor = styles.File
	}

	// Truncate/pad content
	content := truncateRunes(text, contentWidth)
	content = content + strings.Repeat(" ", contentWidth-lipgloss.Width(content))

	// Compose line: marker + line number + content
	markerPart := textColor.Bold(true).Render(marker)
	numPart := styles.Muted.Render(numStr)
	contentPart := textColor.Render(content)
	line := markerPart + " " + numPart + " " + contentPart

	if hasBg {
		line = lipgloss.NewStyle().Background(bgColor).Width(paneWidth).Render(line)
	} else {
		line = pad(line, paneWidth)
	}
	return line
}

func pad(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return s
	}
	return s + strings.Repeat(" ", width-w)
}

func truncateRunes(s string, n int) string {
	r := []rune(s)
	// expand tabs
	expanded := expandTabs(r, 4)
	if len(expanded) <= n {
		return string(expanded)
	}
	return string(expanded[:n])
}

func expandTabs(r []rune, tabWidth int) []rune {
	var out []rune
	col := 0
	for _, c := range r {
		if c == '\t' {
			spaces := tabWidth - (col % tabWidth)
			for i := 0; i < spaces; i++ {
				out = append(out, ' ')
				col++
			}
		} else {
			out = append(out, c)
			col++
		}
	}
	return out
}
