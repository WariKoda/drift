package diffview

import "testing"

func TestSyncProgressLabel(t *testing.T) {
	tests := []struct {
		name string
		done int
		tot  int
		want string
	}{
		{"no total falls back", 0, 0, "syncing…"},
		{"start", 0, 10, "syncing [░░░░░░░░░░] 0/10"},
		{"partway", 4, 10, "syncing [████░░░░░░] 4/10"},
		{"complete", 10, 10, "syncing [██████████] 10/10"},
		{"single file done", 1, 1, "syncing [██████████] 1/1"},
		{"overshoot clamped", 12, 10, "syncing [██████████] 12/10"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := Model{syncDone: tt.done, syncTotal: tt.tot}
			if got := m.syncProgressLabel(); got != tt.want {
				t.Fatalf("syncProgressLabel() = %q, want %q", got, tt.want)
			}
		})
	}
}
