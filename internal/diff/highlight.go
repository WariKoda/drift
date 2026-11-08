package diff

import (
	"context"
	"time"
	"unicode/utf8"

	"github.com/rivo/uniseg"
	"github.com/sergi/go-diff/diffmatchpatch"
)

// TextSpan addresses original UTF-8 bytes, never terminal escape sequences.
type TextSpan struct {
	Start, End int
	Emphasis   bool
}

// WordHighlights is optional presentation data. Oversized or unrelated blocks
// keep whole-line highlighting; comparison results and source bytes are untouched.
func WordHighlights(ctx context.Context, lines []DiffLine) (map[int][]TextSpan, error) {
	spans := make(map[int][]TextSpan)
	deadline := time.Now().Add(50 * time.Millisecond)
	for start := 0; start < len(lines); {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		if time.Now().After(deadline) {
			break
		}
		if lines[start].Kind == LineEqual {
			start++
			continue
		}
		end := start
		var removed, added []int
		for end < len(lines) && lines[end].Kind != LineEqual {
			if end%1024 == 0 {
				if err := ctx.Err(); err != nil {
					return nil, err
				}
				if time.Now().After(deadline) {
					return spans, nil
				}
			}
			if lines[end].Kind == LineRemoved {
				if len(removed) < 65 {
					removed = append(removed, end)
				}
			} else {
				if len(added) < 65 {
					added = append(added, end)
				}
			}
			end++
		}
		if len(removed) == len(added) && len(removed) <= 64 {
			for i, old := range removed {
				if err := ctx.Err(); err != nil {
					return nil, err
				}
				if time.Now().After(deadline) {
					break
				}
				new := added[i]
				a, b := lines[old].Text, lines[new].Text
				if len(a)+len(b) > 8192 || !utf8.ValidString(a) || !utf8.ValidString(b) {
					continue
				}
				dmp := diffmatchpatch.New()
				dmp.DiffTimeout = 5 * time.Millisecond
				changes := dmp.DiffCleanupSemantic(dmp.DiffMain(a, b, false))
				equal := 0
				for _, change := range changes {
					if change.Type == diffmatchpatch.DiffEqual {
						equal += utf8.RuneCountInString(change.Text)
					}
				}
				if equal < 2 || equal*3 < max(utf8.RuneCountInString(a), utf8.RuneCountInString(b)) {
					continue
				}
				x, y := 0, 0
				for _, change := range changes {
					n := len(change.Text)
					switch change.Type {
					case diffmatchpatch.DiffEqual:
						x += n
						y += n
					case diffmatchpatch.DiffDelete:
						spans[old] = append(spans[old], TextSpan{Start: x, End: x + n, Emphasis: true})
						x += n
					case diffmatchpatch.DiffInsert:
						spans[new] = append(spans[new], TextSpan{Start: y, End: y + n, Emphasis: true})
						y += n
					}
				}
				// A mark belongs to the complete grapheme, including accents and ZWJ emoji.
				for _, index := range []int{old, new} {
					g := uniseg.NewGraphemes(lines[index].Text)
					for g.Next() {
						from, to := g.Positions()
						for j, span := range spans[index] {
							if from < span.End && to > span.Start {
								spans[index][j].Start = min(span.Start, from)
								spans[index][j].End = max(span.End, to)
							}
						}
					}
				}
			}
		}
		start = end
	}
	return spans, nil
}
