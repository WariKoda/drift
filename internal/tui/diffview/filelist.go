package diffview

import (
	"fmt"
	"path"
	"path/filepath"
	"strings"

	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// fileListStep is how many columns [<] and [>] move the sidebar edge.
const fileListStep = 4

// fileListRow is one sidebar row: a directory heading (session -1) or a file.
type fileListRow struct {
	dir     string
	session int
}

// fileListing groups the sessions by directory. Sessions arrive sorted as a
// tree from app.Load; an unsorted list still renders, with repeated headings.
type fileListing struct {
	rows  []fileListRow
	names []string // base name per session
	rowOf []int    // sidebar row per session
}

func (m Model) fileList() *fileListing {
	if m.listing != nil && len(m.listing.names) == len(m.sessions) {
		return m.listing
	}
	return m.buildFileList()
}

func (m Model) buildFileList() *fileListing {
	list := &fileListing{names: make([]string, len(m.sessions)), rowOf: make([]int, len(m.sessions))}
	previous := ""
	for i := range m.sessions {
		name := m.displayPath(&m.sessions[i])
		dir := path.Dir(name)
		if i == 0 || dir != previous {
			list.rows = append(list.rows, fileListRow{dir: dir, session: -1})
			previous = dir
		}
		list.names[i] = path.Base(name)
		list.rowOf[i] = len(list.rows)
		list.rows = append(list.rows, fileListRow{session: i})
	}
	return list
}

// displayPath prefers the project-relative local path, then the path below the
// host root. Paths outside both stay absolute rather than being guessed.
func (m Model) displayPath(s *diff.Session) string {
	if base := m.root.Base(); base != "" && s.LocalPath != "" {
		if rel, err := filepath.Rel(base, s.LocalPath); err == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
			return filepath.ToSlash(rel)
		}
	}
	if s.RemotePath != "" && m.host.RootPath != "" {
		root := strings.TrimSuffix(path.Clean(m.host.RootPath), "/") + "/"
		if rel, ok := strings.CutPrefix(path.Clean(s.RemotePath), root); ok && rel != "" {
			return rel
		}
	}
	if s.LocalPath != "" {
		return filepath.ToSlash(s.LocalPath)
	}
	return s.RemotePath
}

// sessionFlip reports whether the session previews an upload: remote is then
// the before state and local the after state.
func (m Model) sessionFlip(index int) bool {
	if index < 0 || index >= len(m.syncDirs) {
		return false
	}
	return m.syncDirs[index] == DirUpload || m.syncDirs[index] == DirDeleteRemote
}

// displayCounts follows the session's before/after order, like its diff rows.
func (m Model) displayCounts(index int) (added, removed int) {
	result := m.sessions[index].Result
	if result == nil {
		return 0, 0
	}
	added, removed = result.Counts()
	if m.sessionFlip(index) {
		added, removed = removed, added
	}
	return added, removed
}

// fileStatus describes the comparison, never the planned sync action. A plain
// textual change has no label.
func fileStatus(s *diff.Session) (mark, label string, style lipgloss.Style) {
	switch {
	case s.Err != nil:
		return "!", "error", styles.Err
	case s.Result == nil:
		return "…", "loading", styles.Muted
	case s.Result.LocalOnly:
		return "L", "local only", styles.Warn
	case s.Result.RemoteOnly:
		return "R", "remote only", styles.Warn
	case s.Result.Binary:
		return "B", "binary", styles.Muted
	case !s.Result.HasDiff():
		return "=", "identical", styles.Muted
	default:
		return "~", "", styles.File
	}
}

func (m Model) countsLabel(index int) string {
	if index < 0 || index >= len(m.sessions) {
		return ""
	}
	added, removed := m.displayCounts(index)
	var parts []string
	if added > 0 {
		parts = append(parts, styles.DiffAddedCount.Render(fmt.Sprintf("+%d", added)))
	}
	if removed > 0 {
		parts = append(parts, styles.DiffRemovedCount.Render(fmt.Sprintf("-%d", removed)))
	}
	return strings.Join(parts, " ")
}

// renderFileListRows returns bodyHeight padded rows for the left sidebar.
func (m Model) renderFileListRows() []string {
	width, height := m.fileListWidth(), m.fileListHeight()
	list := m.fileList()
	rows := make([]string, 0, height)
	for r := max(0, m.fileListOffset); r < len(list.rows) && len(rows) < height; r++ {
		row := list.rows[r]
		if row.session < 0 {
			dir := row.dir + "/"
			if row.dir == "." {
				dir = "./"
			}
			rows = append(rows, pad(styles.Dir.Render("[/] "+truncLeft(styles.SafeText(dir), width-4)), width))
			continue
		}
		rows = append(rows, m.renderFileRow(row.session, list.names[row.session], width))
	}
	for len(rows) < height {
		rows = append(rows, strings.Repeat(" ", width))
	}
	return rows
}

func (m Model) renderFileRow(i int, name string, width int) string {
	cursor, nameStyle := "  ", styles.Muted
	if i == m.activeIdx {
		cursor, nameStyle = styles.Key.Render("▶")+" ", styles.File
	}
	status, _, statusStyle := fileStatus(&m.sessions[i])

	action := styles.Muted.Render("—")
	dir := DirNone
	if i < len(m.syncDirs) {
		dir = m.syncDirs[i]
	}
	switch dir {
	case DirUpload:
		action = styles.Marked.Render("↑")
	case DirDownload:
		action = styles.Dir.Render("↓")
	case DirDeleteLocal, DirDeleteRemote:
		action = styles.Err.Render("✗")
	default:
		if m.completed[i] {
			action = styles.Marked.Render("✓")
		}
	}

	// cursor(2) + status(1) + space + name + [space + counts] + space + action(1)
	counts := m.countsLabel(i)
	nameMax := width - 6
	if counts != "" {
		if nameMax-1-lipgloss.Width(counts) < 4 {
			counts = ""
		} else {
			nameMax -= 1 + lipgloss.Width(counts)
		}
	}
	name = ansi.Truncate(styles.SafeText(name), max(1, nameMax), "…")
	gap := max(1, width-5-lipgloss.Width(name)-lipgloss.Width(counts))
	if counts != "" {
		gap = max(1, gap-1)
	}
	line := cursor + statusStyle.Render(status) + " " + nameStyle.Render(name) + strings.Repeat(" ", gap)
	if counts != "" {
		line += counts + " "
	}
	return pad(line+action, width)
}

// sessionAtRow returns the session on a sidebar row, or -1 for a heading or a
// row past the end.
func (m Model) sessionAtRow(row int) int {
	list := m.fileList()
	if row < 0 || row >= len(list.rows) {
		return -1
	}
	return list.rows[row].session
}

// resizeFileList moves the sidebar edge for this session only.
func (m *Model) resizeFileList(delta int) {
	width := m.fileListWidth()
	if width == 0 {
		return
	}
	m.fileListColumns = width + delta
	m.fileListColumns = m.fileListWidth()
	m.SetSize(m.Width, m.Height)
}
