# Plan: Maus-Support für drift

## Ziel

Maus-Bedienung im TUI ergänzen, ohne die bestehende Tastatur-Bedienung zu verändern
oder die Render-Schicht umzubauen.

Kernaussage der Vorab-Analyse: der Aufwand ist **moderat**, weil drift keine
`bubbles`-Komponenten verwendet. Das gesamte Rendering ist handgeschrieben mit
explizit berechneter Geometrie (`viewportHeight()`, `paneWidths()`, `cursor`,
`offset`). Hit-Testing ist damit reine Arithmetik auf bereits vorhandenem State —
kein Umbau, sondern eine Ergänzung.

Grobschätzung:

| Phase | Inhalt | Aufwand |
|-------|--------|---------|
| 1 | Scrollrad | ~2 Std. |
| 2 | Klick zum Selektieren | ~1 Tag |
| 3 | Kür (Hover, Statusbar-Klicks, Drag) | ~0,5 Tag, optional |

Empfehlung: **Phase 1 separat und zuerst ausliefern.** Scrollrad ist das, was
Nutzer in einem TUI reflexartig versuchen, und es kostet fast nichts.

---

## Zwei Vorentscheidungen

### 1. Maus-Tracking ist opt-out-pflichtig

Mit aktiviertem Maus-Tracking verliert der Nutzer die **native Terminal-Textselektion**
(Markieren und Kopieren per Maus). Das ist der übliche Grund, warum TUIs Maus-Support
hinter einen Schalter legen.

Konsequenz für den Plan:

- Config-Option `mouse` (Default: `true`)
- CLI-Flag `--no-mouse` überschreibt die Config
- Env-Var `DRIFT_NO_MOUSE` als dritte Ebene (analog zu `DRIFT_LOG`/`DRIFT_DEBUG`)
- In der Hilfe (`?` im Browser) dokumentieren: **Shift+Klick** ist der terminalseitige
  Notausgang für Textselektion und funktioniert in den meisten Terminals auch bei
  aktivem Tracking.

### 2. Layout-Konstanten müssen aus den Kommentaren heraus

Aktuell sind die Zeilen-Offsets nur **Kommentare**:

```go
// internal/tui/browser/model.go
h := m.Height - 6 // header + 3 separators + pane labels + status bar

// internal/tui/diffview/model.go
// header(1) + sep(1) + fileList(fh) + sep(1) + colLabels(1) + sep(1) + content(vh) + sep(1) + status(1) = Height
h := m.Height - 7 - m.fileListHeight()
```

Sobald Hit-Testing von diesen Zahlen abhängt, wird jede Layout-Änderung zu einer
stillen Fehlerquelle: die View verschiebt sich, der Klick landet eine Zeile daneben,
und niemand merkt es bis zum Nutzer.

Deshalb ist der **erste Arbeitsschritt jeder Phase-2-Datei**: die Offsets in benannte
Konstanten ziehen, die View **und** Hit-Test gemeinsam verwenden. Das ist rund ein
Drittel des Phase-2-Aufwands und verhindert genau die Bug-Klasse, die sonst erst in
Produktion auffällt.

---

## Phase 1 — Scrollrad

### 1.1 `cmd/root.go`

`runProgram()` (Zeile ~142) erweitern:

```go
opts := []tea.ProgramOption{tea.WithAltScreen()}
if mouseEnabled() {
    opts = append(opts, tea.WithMouseCellMotion())
}
p := tea.NewProgram(app, opts...)
```

`tea.WithMouseCellMotion()` statt `WithMouseAllMotion()`: liefert Klicks, Wheel und
Motion **nur während gedrückter Taste**. `WithMouseAllMotion()` würde bei jeder
Cursor-Bewegung ein Event schicken — unnötige Last, solange es kein Hover gibt
(siehe Phase 3).

Neue Funktion `resolveMouseConfig()` analog zum bestehenden `resolveLogConfig()`,
mit der Präzedenz: CLI-Flag > Env-Var > Config > Default `true`.

### 1.2 `internal/tui/app.go`

In `Update()` einen `case tea.MouseMsg` ergänzen, der **wie `tea.KeyMsg`** an den
aktiven Screen delegiert. Wichtig ist die Reihenfolge relativ zum bestehenden
Loader-Gate am Anfang von `Update()`:

- Loader aktiv (`a.loader.Active()`) → Maus-Events verwerfen, analog zu
  `blocksQuitKey`/`blocksNetworkKey`
- Loader nur sichtbar → verwerfen
- `a.globalError` bei jedem Maus-Event zurücksetzen, genau wie bei `KeyMsg`

Die Delegation folgt dem Muster von `baseView()`: `switch a.state.Screen`.

### 1.3 Wheel-Handling pro Screen

Bubble Tea v1.3.10 liefert Wheel als `tea.MouseMsg` mit
`Button == tea.MouseButtonWheelUp` / `WheelDown`.

Betroffen sind nur Screens mit Scroll-State:

| Screen | Scroll-State | Aktion |
|--------|-------------|--------|
| `browser` | `offset`, `remoteOffset` | Pane unter dem Cursor scrollen (x-Achse entscheidet) |
| `browser` (Finder) | Finder-Offset | Ergebnisliste scrollen |
| `browser` (Preview) | Preview-Scroll | Preview-Pane scrollen |
| `diffview` | `scroll`, `fileListOffset` | Zone unter dem Cursor scrollen (y-Achse entscheidet) |
| `hostmanager` | Cursor, `vh := m.Height - 4` | Cursor bewegen |

`dashboard`, `hostselector`, `hostform`, `projectform` rendern alle Einträge ohne
Scroll-Offset — dort ist Wheel in Phase 1 ein No-op.

**Designentscheidung:** Scrollrad bewegt den **Viewport**, nicht den Cursor
(`offset` ändern, `cursor` unverändert lassen). Das entspricht dem Verhalten, das
Nutzer von Editoren kennen. Ausnahme `hostmanager`: dort gibt es keinen separaten
Offset, also bewegt Wheel dort den Cursor.

Schrittweite: 3 Zeilen pro Wheel-Event (Terminal-Konvention).

Da Wheel `offset` ohne `cursor` verschiebt, dürfen die bestehenden `clampScroll()`-
Funktionen **nicht** unverändert danach laufen — sie ziehen den Offset zum Cursor
zurück. Nötig ist eine getrennte Klemmung, die nur `offset` gegen
`[0, len(entries)-vh]` prüft.

---

## Phase 2 — Klick zum Selektieren

### 2.1 Grundmuster

Pro Screen eine Methode:

```go
// hitTest maps terminal coordinates to a target within the screen.
func (m Model) hitTest(x, y int) hit
```

`hit` ist ein kleiner Struct mit Zone (welcher Bereich) und Index (welche Zeile).
Kein Interface, keine Registry — nach der Konvention „keine spekulativen
Abstraktionen" bleibt das pro Package lokal, solange es nur zwei bis drei
Implementierungen gibt.

Klick-Semantik einheitlich über alle Screens:

- **Einfachklick** → Cursor auf die Zeile setzen (bei zwei Panes zusätzlich das
  aktive Pane wechseln)
- **Doppelklick** → die Aktion auslösen, die `Enter` auslösen würde

Bubble Tea liefert **kein** Doppelklick-Event. Das muss selbst gebaut werden:
Zeitstempel und Position des letzten Klicks im Model halten, und bei einem zweiten
Klick auf derselben Zeile innerhalb ~400 ms als Doppelklick werten. Da das in
mehreren Screens gebraucht wird, gehört es in einen kleinen gemeinsamen Helper —
aber erst anlegen, wenn der dritte Nutzer existiert (Konvention: 3+ Stellen).

### 2.2 `internal/tui/browser` — der aufwendigste Screen

Zwei Panes nebeneinander, also y **und** x auswerten.

Vertikales Layout aus `view.go`:

```
y=0            Header
y=1            Separator
y=2            Pane-Labels
y=3            Separator
y=4 .. 3+vh    Einträge      ← vh = viewportHeight() = Height - 6
y=4+vh         Separator
y=5+vh         Statusbar
```

Daraus:

```go
const browserHeaderLines = 4 // header, sep, pane labels, sep
entryIndex := y - browserHeaderLines + offset  // offset je nach Pane
```

Horizontales Layout aus `paneWidths()`:

```go
left := (m.Width - 1) / 2   // min 10
right := m.Width - left - 1 // min 10
```

- `x < left` → lokales Pane, Index über `m.offset`
- `x == left` → Divider, Klick ignorieren
- `x > left` → Remote-Pane, Index über `m.remoteOffset`

Sonderfälle, die der Hit-Test kennen muss:

- **Preview aktiv** (`m.preview.active`): eines der beiden Panes zeigt die Preview
  statt einer Liste. Klick dorthin selektiert nichts, scrollt aber.
- **Finder aktiv** (`m.finder.active`): komplett anderes Layout mit
  `finderViewportHeight() = Height - 5`. Eigener Hit-Test-Zweig, ganz am Anfang —
  analog zu `View()`, das ebenfalls zuerst auf `finder.active` prüft.
- **Help aktiv** (`m.showHelp`): Klick schließt nur die Hilfe.
- **Leere Zeilen**: `renderLocalRow` gibt für `i >= len(entries)` Leerraum zurück.
  Der Hit-Test muss denselben Bereichs-Check machen, sonst springt der Cursor auf
  einen nicht existierenden Eintrag.
- Klick auf **Pane-Label** (y=2) → dieses Pane aktivieren, Cursor unverändert.

### 2.3 `internal/tui/diffview`

Zwei Zonen übereinander, plus zwei Spalten.

Vertikales Layout aus `view.go`:

```
y=0                Header
y=1                Separator
y=2 .. 1+fh        Dateiliste     ← fh = fileListHeight(), max 5
y=2+fh             Separator
y=3+fh             Spalten-Labels
y=4+fh             Separator
y=5+fh .. 4+fh+vh  Diff-Inhalt    ← vh = viewportHeight() = Height - 7 - fh
y=5+fh+vh          Separator
y=6+fh+vh          Statusbar
```

Zwei getrennte Trefferzonen:

- **Dateiliste**: `fileIndex = y - 2 + m.fileListOffset` → setzt `activeIdx`,
  danach `clampFileList()`
- **Diff-Inhalt**: `lineIndex = y - (5 + fh) + m.scroll` → für Phase 2 reicht es,
  hier nichts zu selektieren; die Zone wird erst für Wheel (Phase 1) und
  eventuelles Zeilen-Highlighting (Phase 3) gebraucht

Horizontal über `paneWidth() = (m.Width - 1) / 2`, Divider bei `x == paneWidth()`.

**Wichtig:** `m.showErrors` schaltet den Inhaltsbereich auf `renderErrorList(vh)` um,
und bei `s.Err != nil` / `s.Result == nil` steht dort nur eine Meldung. Der Hit-Test
muss diese Zustände wie den Dateilisten-Fall unterscheiden, sonst werden Klicks auf
Fehlermeldungen als Diff-Zeilen interpretiert.

### 2.4 `internal/tui/hostmanager`

Einfachster Fall: eine Liste mit zwei Kopfzeilen (Titel, Separator), danach die
Einträge; `vh := m.Height - 4` in `view.go:23`. Kein Scroll-Offset vorhanden — der
Index ist direkt `y - 2`.

Eine Besonderheit: `m.entries` enthält neben Hosts auch **Gruppen-Kopfzeilen**
(`e.isHeader`, gerendert über `renderHeader(e.scope)`). Ein Klick auf eine solche
Zeile muss ignoriert werden — der Cursor darf dort nicht landen. Der Hit-Test prüft
also nicht nur den Bereich, sondern zusätzlich `!m.entries[i].isHeader`.

Deshalb ist `hostmanager` trotz des einfachen Layouts ein guter erster Screen: er
zwingt das Muster von Anfang an dazu, „Zeile getroffen" und „Zeile ist ein gültiges
Ziel" zu trennen.

### 2.5 `dashboard`, `hostselector`

Beide rendern ohne Scroll-Offset. `hostselector` wird zusätzlich über
`lipgloss.Place(...)` **zentriert** in `app.go:baseView()` gerendert. Das heißt: die
Koordinaten, die der Screen von der Maus bekommt, sind Terminal-Koordinaten, nicht
Screen-lokale.

Das ist der einzige Punkt, an dem `app.go` mehr tun muss als durchreichen: für
zentrierte Modals muss der Offset (`(TermWidth - modalWidth) / 2` bzw. analog für y)
vom Event abgezogen werden, bevor es den Screen erreicht. Alternativ — und
einfacher — bekommt `hostselector` in Phase 2 gar keinen Klick-Support, nur Wheel.
Empfehlung: **auslassen**, bis jemand danach fragt.

---

## Phase 3 — Kür (optional)

Nur umsetzen, wenn Phase 1 und 2 sich im Alltag bewährt haben.

- **Hover-Highlighting**: braucht `tea.WithMouseAllMotion()` statt `CellMotion` und
  damit deutlich mehr Events. Vorher prüfen, ob `Update()` das ohne spürbare Latenz
  verkraftet.
- **Klickbare Statusbar**: die Hinweise wie `[@] select host` in klickbare Regionen
  verwandeln. Erfordert, dass die Statusbar ihre Segment-Breiten kennt — aktuell
  baut sie nur einen String.
- **Drag-Scrolling** an einer Scrollbar. Setzt voraus, dass es überhaupt eine
  sichtbare Scrollbar gibt — die gibt es derzeit nicht.

---

## Tests

Konvention im Projekt: keine Mocks. Für Maus-Support passt das gut, weil der
interessante Teil reine Arithmetik ohne I/O ist.

**Tabellentests pro Screen für die Koordinaten-Abbildung.** Das ist der eigentliche
Wert der Test-Arbeit — genau hier entstehen die Off-by-one-Fehler:

```go
// internal/tui/browser/mouse_test.go
tests := []struct {
    name       string
    width      int
    height     int
    offset     int
    entryCount int
    x, y       int
    wantZone   zone
    wantIndex  int
}{
    {"first entry, local pane", 80, 24, 0, 10, 5, 4, zoneLocal, 0},
    {"scrolled local pane",     80, 24, 7, 20, 5, 4, zoneLocal, 7},
    {"divider is ignored",      80, 24, 0, 10, 39, 4, zoneNone, -1},
    {"remote pane",             80, 24, 0, 10, 50, 4, zoneRemote, 0},
    {"below last entry",        80, 24, 0, 2,  5, 10, zoneNone, -1},
    {"header row",              80, 24, 0, 10, 5, 0, zoneNone, -1},
    {"status bar",              80, 24, 0, 10, 5, 23, zoneNone, -1},
}
```

Zusätzlich ein **Konsistenztest**, der Hit-Test und View aneinander bindet: `View()`
rendern, die Zeilen zählen, und prüfen, dass die als „erste Eintragszeile"
berechnete Konstante wirklich auf die erste Eintragszeile fällt. Dieser Test ist der
Grund, warum Layout-Änderungen dann laut scheitern statt still danebenzugreifen.

Für `Update()` genügen wenige Tests, die eine `tea.MouseMsg` einspeisen und den
resultierenden Cursor prüfen — das Muster existiert bereits in den vorhandenen
`update_test.go`-Dateien für `tea.KeyMsg`.

---

## Betroffene Dateien

### Phase 1

| Datei | Änderung |
|-------|----------|
| `cmd/root.go` | `tea.WithMouseCellMotion()`, `resolveMouseConfig()`, Flag `--no-mouse` |
| `internal/config/config.go` | Feld `Mouse *bool` in `Defaults` (Pointer, um „nicht gesetzt" von `false` zu unterscheiden) |
| `internal/tui/app.go` | `case tea.MouseMsg` + Delegation, Loader-Gate |
| `internal/tui/browser/update.go` | Wheel → `offset` / `remoteOffset` / Preview-Scroll |
| `internal/tui/diffview/update.go` | Wheel → `scroll` / `fileListOffset` |
| `internal/tui/hostmanager/update.go` | Wheel → Cursor |

### Phase 2

| Datei | Änderung |
|-------|----------|
| `internal/tui/browser/mouse.go` | **neu** — `hitTest()`, Zonen-Konstanten |
| `internal/tui/browser/view.go` | Layout-Konstanten statt Magic Numbers |
| `internal/tui/browser/model.go` | `viewportHeight()` auf Konstanten umstellen |
| `internal/tui/browser/update.go` | Klick- und Doppelklick-Behandlung |
| `internal/tui/diffview/mouse.go` | **neu** — `hitTest()` für Dateiliste und Inhalt |
| `internal/tui/diffview/view.go` | Layout-Konstanten statt Magic Numbers |
| `internal/tui/diffview/model.go` | `viewportHeight()` auf Konstanten umstellen |
| `internal/tui/hostmanager/mouse.go` | **neu** — einfacher Listen-Hit-Test |
| `internal/tui/*/mouse_test.go` | **neu** — Tabellentests |
| `internal/tui/browser/view.go` (Help) | Maus-Bedienung in der Hilfe dokumentieren |
| `README.md`, `CHANGELOG.md` | Maus-Support und `--no-mouse` erwähnen |

---

## Reihenfolge der Umsetzung

1. **Branch `feature/mouse-wheel`** — Phase 1 komplett, inklusive Config-Schalter
   und Doku. Eigenständig auslieferbar und für sich nützlich.
2. **Branch `feature/mouse-layout-constants`** — nur die Refactoring-Vorarbeit aus
   Abschnitt 2 der Vorentscheidungen, mit dem Konsistenztest. Ändert kein Verhalten,
   ist also risikoarm zu mergen und macht den nächsten Schritt klein.
3. **Branch `feature/mouse-click`** — Hit-Testing und Klick-Behandlung, Screen für
   Screen: erst `hostmanager` (einfachster Fall, etabliert das Muster), dann
   `diffview`, zuletzt `browser`.
4. Phase 3 nur bei konkretem Bedarf.

Vor jedem Merge: `go test ./...`, `go vet ./...`, `go build ./...`.
