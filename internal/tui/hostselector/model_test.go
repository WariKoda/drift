package hostselector

import (
	"testing"

	"github.com/WariKoda/drift/internal/config"
)

func TestViewDoesNotPanicInNarrowTerminal(t *testing.T) {
	for width := 0; width <= 5; width++ {
		t.Run(itoa(width), func(t *testing.T) {
			m := New(&config.MergedConfig{}, width, 10)
			_ = m.View()
		})
	}
}
