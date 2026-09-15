package textfield

import (
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func TestHandleKeyDropsPastedControlCharacters(t *testing.T) {
	field := TextField{}
	field.HandleKey(tea.KeyMsg{
		Type:  tea.KeyRunes,
		Runes: []rune{'a', '\n', 'b', '\r', '\t', 0, 'c'},
	})

	if got := field.Value(); got != "abc" {
		t.Fatalf("Value = %q, want %q", got, "abc")
	}
	if field.cursor != 3 {
		t.Fatalf("cursor = %d, want 3", field.cursor)
	}
}
