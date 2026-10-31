// Package textfield provides a single-line editable text input widget shared
// by drift's TUI forms (host form, project form).
package textfield

import (
	"fmt"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/WariKoda/drift/internal/styles"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/rivo/uniseg"
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
			previous := f.cursorBoundary(false)
			f.value = append(f.value[:previous], f.value[f.cursor:]...)
			f.cursor = previous
		}
	case "delete":
		if f.cursor < len(f.value) {
			next := f.cursorBoundary(true)
			f.value = append(f.value[:f.cursor], f.value[next:]...)
		}
	case "left":
		f.cursor = f.cursorBoundary(false)
	case "right":
		f.cursor = f.cursorBoundary(true)
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
			if unicode.IsControl(r) {
				continue
			}
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

func (f TextField) cursorBoundary(forward bool) int {
	previous, position := 0, 0
	clusters := uniseg.NewGraphemes(string(f.value))
	for clusters.Next() {
		position += utf8.RuneCountInString(clusters.Str())
		if forward && position > f.cursor {
			return position
		}
		if !forward && position >= f.cursor {
			return previous
		}
		previous = position
	}
	return len(f.value)
}

func (f *TextField) renderInner(width int) string {
	if width <= 0 {
		return ""
	}
	if len(f.value) == 0 && !f.Focused && f.Placeholder != "" {
		text := ansi.Truncate(styles.SafeText(f.Placeholder), width, "")
		return styles.Muted.Render(text + strings.Repeat(" ", width-ansi.StringWidth(text)))
	}

	type cell struct {
		text  string
		width int
	}
	var cells []cell
	cursor, position := 0, 0
	clusters := uniseg.NewGraphemes(string(f.value))
	for clusters.Next() {
		text := styles.SafeText(clusters.Str())
		count := utf8.RuneCountInString(clusters.Str())
		if f.Password {
			text = "•"
		}
		if position <= f.cursor {
			cursor = len(cells)
		}
		cells = append(cells, cell{text, ansi.StringWidth(text)})
		position += count
	}
	if f.cursor == len(f.value) {
		cursor = len(cells)
		cells = append(cells, cell{" ", 1})
	}

	start, occupied := 0, 0
	for i := 0; i <= cursor && i < len(cells); i++ {
		occupied += cells[i].width
	}
	for occupied > width && start < cursor {
		occupied -= cells[start].width
		start++
	}
	var out strings.Builder
	written := 0
	for i := start; i < len(cells); i++ {
		if written+cells[i].width > width {
			break
		}
		if f.Focused && i == cursor {
			out.WriteString(styles.InputCursor.Render(cells[i].text))
		} else {
			out.WriteString(cells[i].text)
		}
		written += cells[i].width
	}
	out.WriteString(strings.Repeat(" ", width-written))
	return out.String()
}
