# Terminal Images

drift displays image previews and side-by-side image diffs as ordinary terminal
text rows via `internal/termimage` and `internal/imagefile`.

## Rendering modes and detection

`termimage.Detect(getenv, trueColor)` selects a mode from the environment and
the termenv colour profile. There is no configuration key in `config.toml`.

| Mode | When used | Mechanism |
| --- | --- | --- |
| `kitty` | Kitty or Ghostty (outside tmux), or `DRIFT_IMAGES=kitty` | Kitty graphics protocol with Unicode placeholders |
| `blocks` | Other terminals with true colour, or `DRIFT_IMAGES=blocks` | Half-block text rows (`▀`/`▄`, two pixels per cell) |
| `off` | No true colour, or `DRIFT_IMAGES=off` (`0`, `false`, `no`) | Images disabled |

Inside `tmux`, the outer terminal cannot be detected reliably, so `Detect` falls
back to half blocks unless `DRIFT_IMAGES=kitty` is set (which requires
`allow-passthrough on` in tmux). When `tmux` is active, Kitty escape sequences
are wrapped with `ansi.TmuxPassthrough`.

Users can verify terminal support with `drift image-test [image]`.

## Frame safety and `termimage.Output`

- `cmd` creates a single `*termimage.Output`, passes it to Bubble Tea via
  `tea.WithOutput`, and hands it to `tui.New`.
- `Output` serializes all terminal writes under a mutex so Kitty graphics
  transmissions never interleave with or split a Bubble Tea frame.
- Other escape sequences written outside `View` — such as OSC 52 clipboard copy
  — must write through the same `Output`.

## Screen contract and placement lifecycle

- **Never put image data in `View()`.** Call `Output.Show(img, maxCols, maxRows)`
  only from a `tea.Cmd` goroutine.
- In `kitty` mode, `Output.Show` scales the image to fit the cell box (up to
  `maxUpscale = 2`, capped at 256 cells per axis), encodes straight-alpha RGBA,
  zlib-compresses and base64-chunks the payload, transmits it with a virtual
  placement (`a=p`, `U=1`), and records the image ID.
- In `blocks` mode, `Output.Show` computes the coloured half-block rows in
  memory and does not write to the terminal.
- `View()` renders only the rows returned by `termimage.Rows(placement)`. For
  Kitty placements, each row consists of Unicode placeholder characters whose
  foreground RGB encodes the image ID and whose combining diacritics encode the
  row and column, so the line renderer can clip and layout them like normal text.
- **Always free placements.** Call `Output.Delete(id)` whenever a screen moves
  on, replaces an image, or closes — including stale placements whose `tea.Cmd`
  finished after the selection or screen already changed. `Output.Release()`
  frees all remaining Kitty image IDs when the program exits the alternate
  screen.
- `Output.CommonBoxes` computes a single shared scale factor across multiple
  images (such as local and remote sides in the diff view) so relative sizes
  remain comparable.

## Decoding and diff integration

- `internal/imagefile` recognizes, decodes, and scales images from bytes already
  read by the caller (`imagefile.MaxBytes`, `imagefile.Decode`, `imagefile.Fit`,
  `imagefile.Stretch`).
- The diff view loads both sides through `app.LoadImagePair` rather than reading
  files or calling remote clients directly from `internal/tui/diffview`.
