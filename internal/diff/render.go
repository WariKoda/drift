package diff

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
)

// RenderUnified converts a DiffResult into a single column of rendered strings,
// each line padded to width columns. scrollOffset is the first DiffLine index
// to render; count is the number of rows.
//
// flip makes the colouring sync-direction-aware: with flip=false (download /
// neutral) removals are local-only lines and additions are remote-only; with
// flip=true (upload) local-only lines render as additions and remote-only as
// removals.
func RenderUnified(result *DiffResult, width, scrollOffset, count int, flip bool) []string {
	out := make([]string, 0, count)
	if result == nil {
		for i := 0; i < count; i++ {
			out = append(out, strings.Repeat(" ", width))
		}
		return out
	}

	if result.Binary {
		meta := fmt.Sprintf("  binary file  local %s %d bytes  remote %s %d bytes",
			result.ModLocal.Format("2006-01-02 15:04"), result.SizeLocal,
			result.ModRemote.Format("2006-01-02 15:04"), result.SizeRemote)
		out = append(out, pad(styles.Muted.Render(meta), width))
		for i := 1; i < count; i++ {
			out = append(out, strings.Repeat(" ", width))
		}
		return out
	}

	lines := result.Lines
	numWidth := 4 // characters per line-number column (local and remote)

	for i := 0; i < count; i++ {
		idx := scrollOffset + i
		if idx >= len(lines) {
			out = append(out, strings.Repeat(" ", width))
			continue
		}

		dl := lines[idx]
		// layout: localNum + " " + remoteNum + " " + marker + " " + content
		contentWidth := width - 2*numWidth - 4
		if contentWidth < 1 {
			contentWidth = 1
		}
		out = append(out, renderUnifiedLine(dl, flip, width, numWidth, contentWidth))
	}
	return out
}

// lineAct is how a unified diff line should be styled after applying sync direction.
type lineAct int

const (
	actEqual lineAct = iota
	actAdd
	actRemove
)

// unifiedAction resolves marker/colour for one DiffLine given sync direction.
// flip=false treats remote as the new state; flip=true treats local as new.
func unifiedAction(kind LineKind, flip bool) lineAct {
	switch kind {
	case LineRemoved:
		if flip {
			return actAdd
		}
		return actRemove
	case LineAdded:
		if flip {
			return actRemove
		}
		return actAdd
	default:
		return actEqual
	}
}

func formatLineNum(n, width int) string {
	if n <= 0 {
		return strings.Repeat(" ", width)
	}
	return fmt.Sprintf("%*d", width, n)
}

func renderUnifiedLine(dl DiffLine, flip bool, width, numWidth, contentWidth int) string {
	act := unifiedAction(dl.Kind, flip)

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
	default:
		textColor = styles.File
	}

	content := truncateRunes(dl.Text, contentWidth)
	content = content + strings.Repeat(" ", contentWidth-lipgloss.Width(content))

	localPart := styles.Muted.Render(formatLineNum(dl.LocalNum, numWidth))
	remotePart := styles.Muted.Render(formatLineNum(dl.RemoteNum, numWidth))
	markerPart := textColor.Bold(true).Render(marker)
	contentPart := textColor.Render(content)
	line := localPart + " " + remotePart + " " + markerPart + " " + contentPart

	if hasBg {
		line = lipgloss.NewStyle().Background(bgColor).Width(width).Render(line)
	} else {
		line = pad(line, width)
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
