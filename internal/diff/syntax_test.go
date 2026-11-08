package diff

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"sync"
	"testing"

	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

func TestSyntaxUsesBothCompleteLexicalContexts(t *testing.T) {
	lines := lineDiff("package demo\n/*\nretained\n*/\nconst value = 30\n", "package demo\n// opened\nretained\n// closed\nconst value = 60\n")
	original := append([]DiffLine(nil), lines...)
	cache := &SyntaxCache{}
	tokens, err := cache.Highlight(context.Background(), "sample.go", lines)
	if err != nil || tokens == nil {
		t.Fatalf("tokens=%v err=%v", tokens, err)
	}
	found := false
	for i, line := range lines {
		if line.Text == "retained" {
			found = true
			if line.Kind != LineEqual || len(tokens.Local[i]) != 1 || tokens.Local[i][0].Class != SyntaxComment || len(tokens.Remote[i]) != 0 {
				t.Fatalf("lost multiline context: local=%v remote=%v", tokens.Local[i], tokens.Remote[i])
			}
		}
	}
	if !found || !reflect.DeepEqual(lines, original) {
		t.Fatal("syntax modified or lost comparison text")
	}
	hit, err := cache.Highlight(context.Background(), "renamed.go", lines)
	if err != nil || hit != tokens {
		t.Fatal("identical content and language did not hit cache")
	}
	changed, err := cache.Highlight(context.Background(), "sample.go", lineDiff("const n=1\n", "const n=2\n"))
	if err != nil || changed == nil || changed == tokens {
		t.Fatal("changed content reused stale tokens")
	}
}

func TestSyntaxPreservesTextGeometryAndSanitization(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })
	lines := lineDiff("package demo\nconst text = \"日本\té 👩‍💻\x1b[2J\"\n", "package demo\nconst text = \"日本\tè 👩‍🔬\x1b[2J\"\n")
	tokens, err := (&SyntaxCache{}).Highlight(context.Background(), "sample.go", lines)
	if err != nil || tokens == nil {
		t.Fatalf("syntax %v %v", tokens, err)
	}
	spans, err := WordHighlights(context.Background(), lines)
	if err != nil {
		t.Fatal(err)
	}
	result := &DiffResult{Lines: lines}
	for _, flip := range []bool{false, true} {
		for _, highlight := range []bool{false, true} {
			for _, split := range []bool{false, true} {
				for _, offset := range []int{0, 8, 20} {
					rows := Flatten(lines, DefaultContext, nil, flip)
					if split {
						rows = SplitDisplayRows(lines, rows, flip)
					}
					options := RenderOptions{Flip: flip, Highlight: highlight, Split: split, Offset: offset, Spans: spans}
					plain := RenderUnifiedRows(result, rows, 80, 0, len(rows), options)
					options.Syntax = tokens
					painted := RenderUnifiedRows(result, rows, 80, 0, len(rows), options)
					for i := range painted {
						if ansi.Strip(plain[i]) != ansi.Strip(painted[i]) || ansi.StringWidth(painted[i]) != 80 || strings.Contains(painted[i], "\x1b[2J") {
							t.Fatalf("syntax changed safe display: %q", painted[i])
						}
					}
				}
			}
		}
	}
}

func TestSyntaxKeepsActionBackgroundAndWordEmphasis(t *testing.T) {
	profile := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(profile) })
	syntax := []SyntaxSpan{{Start: 0, End: 5, Class: SyntaxKeyword}, {Start: 8, End: 10, Class: SyntaxNumber}}
	spans := []TextSpan{{Start: 8, End: 10, Emphasis: true}}
	for _, base := range []lipgloss.Style{styles.DiffAdded, styles.DiffRemoved} {
		text := renderText("const n=30", spans, syntax, base, RenderOptions{Highlight: true}, 30)
		expected := styles.SyntaxNumber.Inherit(base).Inherit(styles.DiffEmphasis).Render("30")
		if !strings.Contains(text, expected) {
			t.Fatalf("lost syntax/action/word composition: %q; expected %q", text, expected)
		}
	}
}

func TestSyntaxPlainFallbacksAndCancellation(t *testing.T) {
	cache := &SyntaxCache{}
	for _, fixture := range []struct {
		name  string
		lines []DiffLine
	}{
		{"unknown.driftunknown", lineDiff("x\n", "y\n")},
		{"a.go", lineDiff(strings.Repeat("x", syntaxMaxLineBytes+1), "y")},
		{"a.go", lineDiff(strings.Repeat(strings.Repeat("x", 1024)+"\n", 130), "")},
		{"a.go", []DiffLine{{Text: "const n=1", Kind: LineEqual, LocalNum: 50, RemoteNum: 50}}},
		{"a.go", lineDiff("invalid\xff", "valid")},
	} {
		value, err := cache.Highlight(context.Background(), fixture.name, fixture.lines)
		if err != nil || value != nil {
			t.Fatalf("expected plain fallback, got %v, %v", value, err)
		}
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := cache.Highlight(ctx, "a.go", lineDiff("x", "y")); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancel=%v", err)
	}
}

func TestSyntaxCacheConcurrentRequests(t *testing.T) {
	cache := &SyntaxCache{}
	lines := lineDiff("package a\nconst n=1\n", "package a\nconst n=2\n")
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			value, err := cache.Highlight(context.Background(), "a.go", lines)
			if err != nil || value == nil {
				t.Errorf("highlight: %v %v", value, err)
			}
		}()
	}
	wg.Wait()
}

func BenchmarkSyntaxHighlights(b *testing.B) {
	lines := lineDiff(strings.Repeat("// comment\nconst value = \"hello\";\n", 300), strings.Repeat("// comment\nconst value = \"world\";\n", 300))
	b.Run("cold", func(b *testing.B) {
		b.ReportAllocs()
		for b.Loop() {
			cache := &SyntaxCache{}
			if _, err := cache.Highlight(context.Background(), "a.go", lines); err != nil {
				b.Fatal(err)
			}
		}
	})
	b.Run("cached", func(b *testing.B) {
		cache := &SyntaxCache{}
		cache.Highlight(context.Background(), "a.go", lines)
		b.ReportAllocs()
		for b.Loop() {
			if _, err := cache.Highlight(context.Background(), "a.go", lines); err != nil {
				b.Fatal(err)
			}
		}
	})
}
