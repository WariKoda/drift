package diff

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/yourusername/drift/internal/styles"
)

// addedBg / removedBg / missingBg are row backgrounds for the diff panes.
var (
	addedBg   = lipgloss.AdaptiveColor{Light: "#E8F5E9", Dark: "#1B4332"}
	removedBg = lipgloss.AdaptiveColor{Light: "#FFEBEE", Dark: "#4A1010"}
	missingBg = lipgloss.AdaptiveColor{Light: "#F5F5F5", Dark: "#1E1E1E"}
)

// RenderPanes converts a DiffResult into two parallel slices of rendered strings
// (left = local, right = remote), each line padded to paneWidth columns.
// scrollOffset is the first DiffLine index to render; count is the number of rows.
func RenderPanes(result *DiffResult, paneWidth, scrollOffset, count int) (left, right []string) {
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
		contentWidth := paneWidth - numWidth - 2 // 2 for " " padding
		if contentWidth < 1 {
			contentWidth = 1
		}

		left = append(left, renderSide(dl.LocalLine, dl.LocalNum, dl.Kind, true, paneWidth, numWidth, contentWidth))
		right = append(right, renderSide(dl.RemoteLine, dl.RemoteNum, dl.Kind, false, paneWidth, numWidth, contentWidth))
	}
	return
}

func renderSide(text string, lineNum int, kind LineKind, isLocal bool, paneWidth, numWidth, contentWidth int) string {
	// Line number
	var numStr string
	if lineNum > 0 {
		numStr = fmt.Sprintf("%*d", numWidth, lineNum)
	} else {
		numStr = strings.Repeat(" ", numWidth)
	}

	// Determine which side this cell represents and what color to use
	var bgColor lipgloss.AdaptiveColor
	var hasBg bool
	var textColor lipgloss.Style

	switch kind {
	case LineRemoved:
		if isLocal {
			bgColor = removedBg
			hasBg = true
			textColor = lipgloss.NewStyle().Foreground(styles.ColorError)
		} else {
			// remote side is blank for removed lines
			bgColor = missingBg
			hasBg = true
			textColor = styles.Muted
			text = ""
			lineNum = 0
			numStr = strings.Repeat(" ", numWidth)
		}
	case LineAdded:
		if !isLocal {
			bgColor = addedBg
			hasBg = true
			textColor = lipgloss.NewStyle().Foreground(lipgloss.AdaptiveColor{Light: "#1B5E20", Dark: "#A6E3A1"})
		} else {
			// local side is blank for added lines
			bgColor = missingBg
			hasBg = true
			textColor = styles.Muted
			text = ""
			lineNum = 0
			numStr = strings.Repeat(" ", numWidth)
		}
	default:
		textColor = styles.File
	}

	// Truncate/pad content
	content := truncateRunes(text, contentWidth)
	content = content + strings.Repeat(" ", contentWidth-lipgloss.Width(content))

	// Compose line
	numPart := styles.Muted.Render(numStr)
	contentPart := textColor.Render(content)
	line := numPart + "  " + contentPart

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
