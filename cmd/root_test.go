package cmd

import "testing"

func TestShouldShowDashboard(t *testing.T) {
	tests := []struct {
		name          string
		dash, noDash  bool
		hasProjectCtx bool
		regCount      int
		want          bool
	}{
		{"no flags, outside project, has projects", false, false, false, 2, true},
		{"no flags, outside project, no projects", false, false, false, 0, false},
		{"no flags, inside project", false, false, true, 5, false},
		{"force dashboard inside project", true, false, true, 0, true},
		{"force dashboard with no projects", true, false, false, 0, true},
		{"no-dashboard wins over context", false, true, false, 3, false},
		{"no-dashboard wins over dashboard flag", true, true, false, 3, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := shouldShowDashboard(tt.dash, tt.noDash, tt.hasProjectCtx, tt.regCount); got != tt.want {
				t.Fatalf("shouldShowDashboard(%v,%v,%v,%d) = %v, want %v",
					tt.dash, tt.noDash, tt.hasProjectCtx, tt.regCount, got, tt.want)
			}
		})
	}
}
