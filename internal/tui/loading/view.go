package loading

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

var spinnerFrames = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

// Overlay dims base and places the activity modal in the terminal center.
func (m Model) Overlay(base string, width, height int) string {
	if !m.Visible() {
		return base
	}
	return OverlayCentered(base, m.modal(width), width, height)
}

// OverlayCentered dims base and draws modal in the terminal center.
func OverlayCentered(base, modal string, width, height int) string {
	if width <= 0 || height <= 0 || modal == "" {
		return base
	}

	modalLines := strings.Split(modal, "\n")
	modalWidth := 0
	for _, line := range modalLines {
		modalWidth = max(modalWidth, lipgloss.Width(line))
	}

	left := max(0, (width-modalWidth)/2)
	top := max(0, (height-len(modalLines))/2)
	plainBase := plainLines(base, width, height)
	dim := styles.LoadingBackdrop

	lines := make([]string, height)
	for row := range height {
		plain := plainBase[row]
		modalRow := row - top
		if modalRow < 0 || modalRow >= len(modalLines) {
			lines[row] = dim.Render(plain)
			continue
		}

		rightStart := min(width, left+modalWidth)
		leftPart := ansi.Cut(plain, 0, left)
		rightPart := ansi.Cut(plain, rightStart, width)
		middle := modalLines[modalRow]
		middle += strings.Repeat(" ", max(0, modalWidth-lipgloss.Width(middle)))
		lines[row] = dim.Render(leftPart) + middle + dim.Render(rightPart)
	}
	return strings.Join(lines, "\n")
}

func (m Model) modal(termWidth int) string {
	contentWidth := min(42, max(1, termWidth-6))
	phase := m.progress.Phase
	if phase == "" {
		phase = m.label
	}
	spinner := styles.Accent.Render(spinnerFrames[m.frame%len(spinnerFrames)])
	lines := []string{spinner + "  " + styles.Header.Render(truncate(phase, contentWidth-3))}

	if contentWidth >= 16 && m.progress.Total > 0 && !m.progress.Indeterminate {
		total := m.progress.Total
		done := min(m.progress.Done, total)
		barWidth := max(8, contentWidth-14)
		filled := 0
		if total > 0 {
			filled = done * barWidth / total
		}
		bar := styles.Marked.Render(strings.Repeat("█", filled)) +
			styles.Sep.Render(strings.Repeat("░", barWidth-filled))
		percent := 0
		if total > 0 {
			percent = done * 100 / total
		}
		lines = append(lines, fmt.Sprintf("[%s] %3d%%", bar, percent))
		lines = append(lines, styles.Muted.Render(fmt.Sprintf("%d/%d complete", done, total)))
	}

	lines = append(lines, "",
		styles.Key.Render("[Esc]")+styles.Muted.Render(" hide  ")+
			styles.Key.Render("[q]")+styles.Muted.Render(" cancel"))
	content := strings.Join(lines, "\n")
	return styles.LoadingBox.Width(contentWidth).Render(content)
}

func plainLines(view string, width, height int) []string {
	source := strings.Split(view, "\n")
	lines := make([]string, height)
	for i := range height {
		line := ""
		if i < len(source) {
			line = ansi.Strip(source[i])
		}
		line = ansi.Truncate(line, width, "")
		line += strings.Repeat(" ", max(0, width-lipgloss.Width(line)))
		lines[i] = line
	}
	return lines
}

func truncate(s string, width int) string {
	if width < 1 {
		return ""
	}
	if lipgloss.Width(s) <= width {
		return s
	}
	return ansi.Truncate(s, max(1, width-1), "") + "…"
}
