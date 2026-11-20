package diffview

import (
	"image"

	"github.com/WariKoda/drift/internal/app"
	"github.com/WariKoda/drift/internal/diff"
	"github.com/WariKoda/drift/internal/imagefile"
	"github.com/WariKoda/drift/internal/log"
	"github.com/WariKoda/drift/internal/styles"
	"github.com/WariKoda/drift/internal/termimage"
	tea "github.com/charmbracelet/bubbletea"
)

// diffImageSide bounds each decoded side; one pane never needs more pixels.
const diffImageSide = 2048

// imageHeaderRows precede the images: the summary's status, local and remote
// lines, a blank line and the side labels.
const imageHeaderRows = 5

// diffPicture is the image comparison of the active file. Only one is kept:
// moving to another file frees it in the terminal.
type diffPicture struct {
	result  *diff.DiffResult
	loading bool
	pair    app.ImagePair

	// The cell area last requested from the terminal; token rejects results
	// of superseded placements.
	areaCols, areaRows int
	token              uint64
	local, remote      termimage.Placement
	localRows          []string
	remoteRows         []string
	err                error
}

// MsgImagesLoaded carries both decoded sides of an image comparison.
type MsgImagesLoaded struct {
	result *diff.DiffResult
	pair   app.ImagePair
}

// MsgImagesPlaced reports the terminal placements of both sides. The root
// routes it here on every screen, so stale placements are freed.
type MsgImagesPlaced struct {
	result        *diff.DiffResult
	token         uint64
	local, remote termimage.Placement
	err           error
}

// SetImages provides the terminal output that image comparisons draw with.
func (m *Model) SetImages(images *termimage.Output) { m.images = images }

// isImageComparison reports binary comparisons that involve a raster image.
// Files above the text limit carry no header, so their name decides.
func isImageComparison(session *diff.Session) bool {
	r := session.Result
	if session.Err != nil || r == nil || !r.Binary {
		return false
	}
	return r.LocalImage != nil || r.RemoteImage != nil ||
		imagefile.HasImageExtension(session.LocalPath) || imagefile.HasImageExtension(session.RemotePath)
}

// showsImages reports whether the active file is drawn as images.
func (m Model) showsImages() bool {
	s := m.activeSession()
	return m.images.Enabled() && !m.continuous() && s != nil && m.picture != nil && m.picture.result == s.Result
}

// startImages loads the active file's images, or places them again after a
// resize. Bulk and quick syncs keep the connection busy, so loading waits.
func (m *Model) startImages() tea.Cmd {
	if !m.images.Enabled() || m.closed {
		return nil
	}
	s := m.activeSession()
	if s == nil || m.continuous() || !isImageComparison(s) {
		return m.dropImages()
	}
	if m.picture != nil && m.picture.result == s.Result {
		return m.placeImages()
	}
	if m.remoteBusy() {
		return m.dropImages()
	}
	drop := m.dropImages()
	result := s.Result
	m.picture = &diffPicture{result: result, loading: true}
	root, conn := m.root, m.conn
	localPath, remotePath := s.LocalPath, s.RemotePath
	localMissing, remoteMissing := result.RemoteOnly, result.LocalOnly
	load := m.trackCommand(func() tea.Msg {
		pair := app.LoadImagePair(root, conn, localPath, remotePath, localMissing, remoteMissing, diffImageSide)
		return MsgImagesLoaded{result: result, pair: pair}
	})
	return tea.Batch(drop, load)
}

func (m *Model) acceptImages(msg MsgImagesLoaded) tea.Cmd {
	if m.picture == nil || m.picture.result != msg.result {
		return nil
	}
	m.picture.loading = false
	m.picture.pair = msg.pair
	for _, side := range []struct {
		name string
		err  error
	}{{"local", msg.pair.Local.Err}, {"remote", msg.pair.Remote.Err}} {
		if side.err != nil {
			log.Error("load image for diff", "side", side.name, "path", sessionName(m.activeSession()), "err", side.err)
		}
	}
	return m.placeImages()
}

// imageArea is the cell area available to each side.
func (m Model) imageArea() (int, int) {
	return (m.diffWidth() - 4) / 2, m.viewportHeight() - imageHeaderRows
}

func (m *Model) placeImages() tea.Cmd {
	picture := m.picture
	if picture == nil || picture.loading {
		return nil
	}
	cols, rows := m.imageArea()
	if cols < 1 || rows < 1 || cols == picture.areaCols && rows == picture.areaRows {
		return nil
	}
	picture.areaCols, picture.areaRows = cols, rows
	picture.token++
	images, result, token := m.images, picture.result, picture.token
	local, remote := picture.pair.Local.Image, picture.pair.Remote.Image
	if local == nil && remote == nil {
		return nil
	}
	// Size both sides from their original dimensions with one scale, so a
	// changed image size stays visible.
	sizes := []image.Point{infoSize(picture.pair.Local), infoSize(picture.pair.Remote)}
	return func() tea.Msg {
		msg := MsgImagesPlaced{result: result, token: token}
		boxes := images.CommonBoxes(sizes, cols, rows)
		if local != nil {
			msg.local, msg.err = images.Show(local, boxes[0].X, boxes[0].Y)
		}
		if remote != nil && msg.err == nil {
			msg.remote, msg.err = images.Show(remote, boxes[1].X, boxes[1].Y)
		}
		if msg.err != nil {
			log.Error("show diff image", "err", msg.err)
		}
		return msg
	}
}

// infoSize is the original pixel size of a decoded side, or zero.
func infoSize(side app.ImageSide) image.Point {
	if side.Image == nil {
		return image.Point{}
	}
	return image.Pt(side.Info.Width, side.Info.Height)
}

func (m *Model) acceptPlacements(msg MsgImagesPlaced) tea.Cmd {
	picture := m.picture
	if m.closed || picture == nil || picture.result != msg.result || picture.token != msg.token {
		return m.deleteImages(msg.local.ID, msg.remote.ID)
	}
	previous := []uint32{picture.local.ID, picture.remote.ID}
	if msg.err != nil {
		picture.err = msg.err
		return m.deleteImages(append(previous, msg.local.ID, msg.remote.ID)...)
	}
	picture.local, picture.remote = msg.local, msg.remote
	picture.localRows, picture.remoteRows = nil, nil
	if msg.local.ID != 0 {
		picture.localRows = termimage.Rows(msg.local)
	}
	if msg.remote.ID != 0 {
		picture.remoteRows = termimage.Rows(msg.remote)
	}
	return m.deleteImages(previous...)
}

// dropImages forgets the current comparison's images and frees them.
func (m *Model) dropImages() tea.Cmd {
	picture := m.picture
	m.picture = nil
	if picture == nil {
		return nil
	}
	return m.deleteImages(picture.local.ID, picture.remote.ID)
}

func (m Model) deleteImages(ids ...uint32) tea.Cmd {
	images := m.images
	var pending []uint32
	for _, id := range ids {
		if id != 0 {
			pending = append(pending, id)
		}
	}
	if len(pending) == 0 || !images.Enabled() {
		return nil
	}
	return func() tea.Msg {
		for _, id := range pending {
			if err := images.Delete(id); err != nil {
				log.Error("free terminal image", "err", err)
			}
		}
		return nil
	}
}

// renderImageRows draws the summary, then before and after side by side,
// in the same order as the diff rows.
func (m Model) renderImageRows(r *diff.DiffResult, height, width int) []string {
	picture := m.picture
	localInfo, remoteInfo := r.LocalImage, r.RemoteImage
	if info := picture.pair.Local.Info; info.Width > 0 {
		localInfo = &info
	}
	if info := picture.pair.Remote.Info; info.Width > 0 {
		remoteInfo = &info
	}
	lines := imageSummaryLines(r, localInfo, remoteInfo)
	if picture.pair.SamePixels {
		lines[0] = "  " + styles.Warn.Render("Same pixels, different bytes")
	}
	switch {
	case picture.err != nil:
		lines = append(lines, "", "  "+styles.Err.Render("Images cannot be shown: "+styles.SafeText(picture.err.Error())))
		return paddedContentRows(lines, height, width)
	case picture.loading:
		lines = append(lines, "", "  "+styles.Muted.Render("Loading images…"))
		return paddedContentRows(lines, height, width)
	}
	for len(lines) < imageHeaderRows-2 {
		lines = append(lines, "")
	}
	lines = lines[:imageHeaderRows-2]

	type column struct {
		label string
		side  app.ImageSide
		rows  []string
	}
	left := column{"local", picture.pair.Local, picture.localRows}
	right := column{"remote", picture.pair.Remote, picture.remoteRows}
	if m.remoteBeforeLocal() {
		left, right = right, left
	}
	half := max(0, (width-4)/2)
	cell := func(c column, row int) string {
		switch {
		case row == 0:
			return styles.Header.Render(c.label)
		case row != 1:
			if row-2 < len(c.rows) {
				return c.rows[row-2]
			}
			return ""
		case c.side.Missing:
			return styles.Muted.Render("not present")
		case c.side.Err != nil:
			return styles.Err.Render("cannot show: " + styles.SafeText(c.side.Err.Error()))
		}
		return ""
	}
	// Row 1 holds a side's error; its image starts one row lower.
	for row := 0; len(lines) < height; row++ {
		lines = append(lines, "  "+pad(cell(left, row), half)+"  "+pad(cell(right, row), half))
	}
	return paddedContentRows(lines, height, width)
}
