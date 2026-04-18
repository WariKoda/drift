package hostform

import (
	"fmt"
	"strings"

	"github.com/WariKoda/drift/internal/styles"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// TextField is a single-line editable text input widget.
type TextField struct {
	Label       string
	value       []rune
	cursor      int
	Focused     bool
	Width       int  // display width of the input box (characters)
	Password    bool // show • instead of real characters
	Placeholder string
	MaxLen      int // 0 = unlimited
}

// Value returns the current string value.
func (f *TextField) Value() string { return string(f.value) }

// SetValue sets the value and moves the cursor to the end.
func (f *TextField) SetValue(s string) {
	f.value = []rune(s)
	f.cursor = len(f.value)
}

// HandleKey processes a key message, mutating the field state.
func (f *TextField) HandleKey(msg tea.KeyMsg) {
	switch msg.String() {
	case "backspace", "ctrl+h":
		if f.cursor > 0 {
			f.value = append(f.value[:f.cursor-1], f.value[f.cursor:]...)
			f.cursor--
		}
	case "delete":
		if f.cursor < len(f.value) {
			f.value = append(f.value[:f.cursor], f.value[f.cursor+1:]...)
		}
	case "left":
		if f.cursor > 0 {
			f.cursor--
		}
	case "right":
		if f.cursor < len(f.value) {
			f.cursor++
		}
	case "home", "ctrl+a":
		f.cursor = 0
	case "end", "ctrl+e":
		f.cursor = len(f.value)
	case "ctrl+w":
		// delete word before cursor
		for f.cursor > 0 && f.value[f.cursor-1] == ' ' {
			f.value = append(f.value[:f.cursor-1], f.value[f.cursor:]...)
			f.cursor--
		}
		for f.cursor > 0 && f.value[f.cursor-1] != ' ' {
			f.value = append(f.value[:f.cursor-1], f.value[f.cursor:]...)
			f.cursor--
		}
	default:
		for _, r := range msg.Runes {
			if f.MaxLen > 0 && len(f.value) >= f.MaxLen {
				break
			}
			ins := []rune{r}
			f.value = append(f.value[:f.cursor], append(ins, f.value[f.cursor:]...)...)
			f.cursor++
		}
	}
}

// View renders:  "  Label         [value_with_cursor______]"
func (f *TextField) View() string {
	boxWidth := f.Width
	if boxWidth < 12 {
		boxWidth = 12
	}

	var labelStyle lipgloss.Style
	if f.Focused {
		labelStyle = styles.File
	} else {
		labelStyle = styles.Muted
	}
	label := labelStyle.Render(fmt.Sprintf("%-14s", f.Label))

	open := styles.Sep.Render("[")
	close := styles.Sep.Render("]")
	inner := f.renderInner(boxWidth)

	return "  " + label + " " + open + inner + close
}

func (f *TextField) renderInner(width int) string {
	// Show placeholder when empty and not focused
	if len(f.value) == 0 && !f.Focused && f.Placeholder != "" {
		ph := f.Placeholder
		r := []rune(ph)
		if len(r) > width {
			ph = string(r[:width])
		}
		return styles.Muted.Render(ph + strings.Repeat(" ", width-len([]rune(ph))))
	}

	// Build display chars (masked for password fields)
	display := make([]rune, len(f.value))
	copy(display, f.value)
	if f.Password {
		for i := range display {
			display[i] = '•'
		}
	}

	// Scroll window so cursor stays visible
	start := 0
	if f.cursor >= width {
		start = f.cursor - width + 1
	}

	cursorStyle := lipgloss.NewStyle().
		Background(styles.ColorDir).
		Foreground(styles.ColorBadgeFg)

	var sb strings.Builder
	written := 0

	for i := start; i < len(display) && written < width; i++ {
		ch := string(display[i])
		if f.Focused && i == f.cursor {
			sb.WriteString(cursorStyle.Render(ch))
		} else {
			sb.WriteString(ch)
		}
		written++
	}

	// Cursor at end of value
	if f.Focused && f.cursor == len(f.value) && written < width {
		sb.WriteString(cursorStyle.Render(" "))
		written++
	}

	// Pad remaining space
	for written < width {
		sb.WriteRune(' ')
		written++
	}

	return sb.String()
}
