package styles

import "testing"

func TestSafeText(t *testing.T) {
	for _, tc := range []struct{ input, want string }{
		{"é界 👩‍💻 /a/[q]", "é界 👩‍💻 /a/[q]"},
		{"a\nb\rc\td", `a\nb\rc\td`},
		{"\x1b]52;c;SGVsbG8=\a", `\x1b]52;c;SGVsbG8=\a`},
		{"\u009b31m\u2028\u2029\u202e", `\u009b31m\u2028\u2029\u202e`},
		{"bad\xff", "bad�"},
	} {
		if got := SafeText(tc.input); got != tc.want {
			t.Errorf("SafeText(%q) = %q, want %q", tc.input, got, tc.want)
		}
	}
}
