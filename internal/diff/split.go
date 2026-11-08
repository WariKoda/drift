package diff

// SplitDisplayRows pairs the before/after rows of a change block by display
// order. This is alignment for reading, not a claim that lines are equivalent.
func SplitDisplayRows(lines []DiffLine, rows []DisplayRow, flip bool) []DisplayRow {
	out := make([]DisplayRow, 0, len(rows))
	for i := 0; i < len(rows); {
		row := rows[i]
		if row.Kind != DisplayLine || lines[row.LineIndex].Kind == LineEqual {
			out = append(out, row)
			i++
			continue
		}
		var before, after []int
		for i < len(rows) && rows[i].Kind == DisplayLine && lines[rows[i].LineIndex].Kind != LineEqual {
			index := rows[i].LineIndex
			if unifiedAction(lines[index].Kind, flip) == actRemove {
				before = append(before, index)
			} else {
				after = append(after, index)
			}
			i++
		}
		for j := 0; j < max(len(before), len(after)); j++ {
			pair := DisplayRow{Kind: DisplayPair, LineIndex: -1, PairIndex: -1}
			if j < len(before) {
				pair.LineIndex = before[j]
			}
			if j < len(after) {
				pair.PairIndex = after[j]
			}
			out = append(out, pair)
		}
	}
	return out
}
