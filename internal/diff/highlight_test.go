package diff

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"

	"github.com/charmbracelet/x/ansi"
	"github.com/rivo/uniseg"
)

func TestWordHighlightsDoNotChangeComparison(t *testing.T) {
	lines := lineDiff("timeout = 30\nname = café\n", "timeout = 60\nname = cafe\n")
	original := append([]DiffLine(nil), lines...)
	spans, err := WordHighlights(context.Background(), lines)
	if err != nil || len(spans) == 0 {
		t.Fatalf("spans=%v, err=%v", spans, err)
	}
	if !reflect.DeepEqual(lines, original) {
		t.Fatal("presentation changed comparison")
	}
	for index, ranges := range spans {
		for _, span := range ranges {
			if span.Start < 0 || span.End > len(lines[index].Text) || span.Start >= span.End {
				t.Fatalf("invalid range %+v", span)
			}
		}
	}
}

func TestWordHighlightsGraphemesAndTabs(t *testing.T) {
	lines := lineDiff("prefix\té 👩‍💻 end\n", "prefix\tè 👩‍🔬 end\n")
	spans, err := WordHighlights(context.Background(), lines)
	if err != nil {
		t.Fatal(err)
	}
	if len(spans) == 0 {
		t.Fatal("missing highlights")
	}
	for index, ranges := range spans {
		bounds := map[int]bool{0: true}
		g := uniseg.NewGraphemes(lines[index].Text)
		for g.Next() {
			_, end := g.Positions()
			bounds[end] = true
		}
		for _, span := range ranges {
			if !bounds[span.Start] || !bounds[span.End] {
				t.Fatalf("split grapheme: %+v", span)
			}
		}
	}
	result := &DiffResult{Lines: lines}
	for _, flip := range []bool{false, true} {
		for _, highlight := range []bool{false, true} {
			rows := Flatten(lines, DefaultContext, nil, flip)
			plain := RenderUnifiedRows(result, rows, 40, 0, len(rows), RenderOptions{Flip: flip, Highlight: highlight})
			painted := RenderUnifiedRows(result, rows, 40, 0, len(rows), RenderOptions{Flip: flip, Highlight: highlight, Spans: spans})
			for i := range rows {
				if ansi.Strip(plain[i]) != ansi.Strip(painted[i]) || ansi.StringWidth(painted[i]) > 40 {
					t.Fatal("highlight changed text or geometry")
				}
			}
		}
	}
}

func TestWordHighlightFallbackAndCancellation(t *testing.T) {
	for _, lines := range [][]DiffLine{
		lineDiff("old\n", "entirely different\n"),
		lineDiff("prefix old\n", "prefix new\nextra\n"),
		lineDiff(strings.Repeat("a", 10000), strings.Repeat("a", 9999)+"b"),
	} {
		spans, err := WordHighlights(context.Background(), lines)
		if err != nil || len(spans) != 0 {
			t.Fatalf("expected plain fallback: %v, %v", spans, err)
		}
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := WordHighlights(ctx, lineDiff("old\n", "new\n")); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancel=%v", err)
	}
}
