package diff

import (
	"path"
	"strings"
	"unicode/utf8"
)

// maxLabelBytes bounds the heading text kept for one hunk header.
const maxLabelBytes = 200

// gitHeading follows Git's default funcname rule: a line that starts with a
// letter, an underscore or a dollar sign.
func gitHeading(text string) bool {
	if text == "" {
		return false
	}
	c := text[0]
	return c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c == '_' || c == '$'
}

func markdownHeading(text string) bool {
	level := len(text) - len(strings.TrimLeft(text, "#"))
	return level >= 1 && level <= 6 && len(text) > level && (text[level] == ' ' || text[level] == '\t')
}

func headingRule(name string) func(string) bool {
	switch strings.ToLower(path.Ext(name)) {
	case ".md", ".markdown", ".mdx":
		return markdownHeading
	default:
		return gitHeading
	}
}

// LabelHunks sets the Label of each hunk header to the nearest line before the
// hunk, on the before side, that the heading rule for name accepts: a Markdown
// heading for Markdown files, Git's default funcname rule otherwise. Headers
// without such a line keep an empty label. rows must come from Flatten with the
// same flip; lines are scanned once.
func LabelHunks(lines []DiffLine, rows []DisplayRow, flip bool, name string) {
	isHeading := headingRule(name)
	before := LineRemoved
	if flip {
		before = LineAdded
	}
	last, scanned := "", 0
	for i := range rows {
		if rows[i].Kind != DisplayHunkHeader {
			continue
		}
		start := rows[i].LineIndex
		if i+1 < len(rows) && rows[i+1].Kind == DisplayLine {
			start = min(start, rows[i+1].LineIndex)
		}
		for ; scanned < start && scanned < len(lines); scanned++ {
			line := lines[scanned]
			if (line.Kind == LineEqual || line.Kind == before) && isHeading(line.Text) {
				last = line.Text
			}
		}
		rows[i].Label = trimLabel(last)
	}
}

func trimLabel(text string) string {
	text = strings.TrimRight(text, " \t\r\n")
	if len(text) <= maxLabelBytes {
		return text
	}
	cut := maxLabelBytes
	for cut > 0 && !utf8.RuneStart(text[cut]) {
		cut--
	}
	return text[:cut]
}
