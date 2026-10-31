package textfield

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func TestInputFitsTerminalColumns(t *testing.T) {
	for _, value := range []string{"", strings.Repeat("界", 12), strings.Repeat("é", 12), strings.Repeat("👩‍💻", 12), "a\x1b\u202e界"} {
		for _, focused := range []bool{false, true} {
			for _, password := range []bool{false, true} {
				field := TextField{Focused: focused, Password: password}
				field.SetValue(value)
				for _, width := range []int{0, 1, 2, 12, 20} {
					for cursor := 0; cursor <= len(field.value); cursor++ {
						field.cursor = cursor
						if cells := ansi.StringWidth(field.renderInner(width)); cells != width {
							t.Fatalf("value=%q cursor=%d password=%v: width %d rendered %d columns", value, cursor, password, width, cells)
						}
					}
				}
			}
		}
	}
}

func TestPlaceholderFitsTerminalColumns(t *testing.T) {
	field := TextField{Placeholder: "界界界界👩‍💻\u202e"}
	for width := 0; width < 20; width++ {
		if got := ansi.StringWidth(field.renderInner(width)); got != width {
			t.Fatalf("placeholder width=%d rendered %d columns", width, got)
		}
	}
}

func TestEditingPreservesGraphemeClusters(t *testing.T) {
	field := TextField{}
	field.SetValue("a👩‍💻é")
	field.HandleKey(tea.KeyMsg{Type: tea.KeyLeft})
	if field.cursor != 4 {
		t.Fatal("Left split a combining sequence")
	}
	field.HandleKey(tea.KeyMsg{Type: tea.KeyBackspace})
	if field.Value() != "aé" || field.cursor != 1 {
		t.Fatal("Backspace split an emoji sequence")
	}
	field.HandleKey(tea.KeyMsg{Type: tea.KeyDelete})
	if field.Value() != "a" {
		t.Fatal("Delete split a combining sequence")
	}
	field.SetValue("a👩‍💻é")
	field.HandleKey(tea.KeyMsg{Type: tea.KeyHome})
	field.HandleKey(tea.KeyMsg{Type: tea.KeyRight})
	field.HandleKey(tea.KeyMsg{Type: tea.KeyRight})
	if field.cursor != 4 {
		t.Fatal("Right split an emoji sequence")
	}
}
