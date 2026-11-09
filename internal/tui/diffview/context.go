package diffview

import "github.com/WariKoda/drift/internal/diff"

// contextSteps are the choices for [+] and [-]. The zero contextShift selects
// diff.DefaultContext, so a model built without New behaves as before.
var contextSteps = []int{0, 1, diff.DefaultContext, 5, 10, 25}

const defaultContextStep = 2

// revealStep is how many lines [L] adds on each side of a fold that borders a
// change.
const revealStep = 10

func (m Model) context() int {
	return contextSteps[min(max(defaultContextStep+m.contextShift, 0), len(contextSteps)-1)]
}

// shiftContext changes the context lines of every file for this session.
func (m *Model) shiftContext(delta int) {
	step := min(max(defaultContextStep+m.contextShift+delta, 0), len(contextSteps)-1)
	if step-defaultContextStep == m.contextShift {
		return
	}
	anchor := m.topSourceLine()
	m.contextShift = step - defaultContextStep
	m.foldRevision++
	m.remapScrollToSource(anchor)
}

func (m *Model) revealGap(session, gapID int) {
	if m.revealed == nil {
		m.revealed = make(map[int]map[int]int)
	}
	if m.revealed[session] == nil {
		m.revealed[session] = make(map[int]int)
	}
	m.revealed[session][gapID] += revealStep
	m.foldRevision++
}

// revealFirstVisibleFold shows revealStep more lines of the first fold on screen.
func (m *Model) revealFirstVisibleFold() {
	if m.continuous() {
		if session, row, ok := m.firstVisibleStreamFold(); ok {
			m.revealGap(session, row.GapID)
			m.prepareDisplayRows()
		}
		return
	}
	row, ok := m.firstVisibleFold()
	if !ok {
		return
	}
	anchor := m.topSourceLine()
	m.revealGap(m.activeIdx, row.GapID)
	m.remapScrollToSource(anchor)
}
