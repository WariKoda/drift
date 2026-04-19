# Plan: Mapping in `hostform` per TUI erleichtern

## Ziel

Das Bearbeiten von Host-Mappings soll in der bestehenden Host-Form deutlich einfacher werden, ohne direkt eine große neue Screen-Architektur einzuführen.

Empfohlener erster Schritt:

- bestehende `hostform`-Subscreens weiterverwenden
- `subMappingEdit` zu einem kleinen Wizard ausbauen
- lokalen Pfad per Projekt-Browser auswählbar machen
- Remote-Pfad automatisch vorschlagen
- Mapping-Preview und Validierung direkt anzeigen

---

## UX-Vorschlag

### Einstieg

In `internal/tui/hostform/view.go` bleibt die Zeile `Mappings` im Hauptformular erhalten.

`Enter` auf `Mappings` öffnet wie bisher die Mapping-Liste.

### Mapping-Liste

Die bestehende Liste bleibt erhalten, wird aber etwas informativer:

- Mapping-Zeile wie bisher: `local → remote`
- zusätzlich unten kurze Hilfe
- neuer Shortcut: `p` oder `b` für „neues Mapping per Picker“

Beispiel:

```text
Mappings — prod
────────────────────────────────────────────────────────────

▶ plugins/MyPlugin   → /var/www/html/custom/plugins/MyPlugin
  themes/MyTheme     → /var/www/html/custom/themes/MyTheme

[n]new  [p]pick  [e]edit  [d]delete  [Esc]back
```

### Mapping-Editor / Wizard

Der Editor soll zwei Modi unterstützen:

- `manual`: beide Felder frei editierbar
- `browse`: lokaler Pfad wird über Projekt-Browser gewählt

Ein pragmatischer MVP ist aber auch ohne expliziten Modus möglich:

- Feld `Local Path`
- Aktion `Browse local`
- Feld `Remote Path`
- Bereich `Suggested remote`
- Bereich `Preview`
- Bereich `Validation`

Beispiel:

```text
Edit Mapping
────────────────────────────────────────────────────────────

Local Path
plugins/MyPlugin                              [Browse]

Remote Path
/var/www/html/custom/plugins/MyPlugin

Suggested remote paths
▶ /var/www/html/custom/plugins/MyPlugin
  /srv/www/current/custom/plugins/MyPlugin

Preview
plugins/MyPlugin/src/Foo.php
→ /var/www/html/custom/plugins/MyPlugin/src/Foo.php

[Tab] next  [b] browse local  [Ctrl+S] save  [Esc] cancel
```

### Lokaler Picker

Ein zusätzlicher Subscreen listet Verzeichnisse unterhalb des Projekt-Roots.

Ziel:

- User wählt nicht selbst relative Pfade per Hand
- nur sinnvolle lokale Zielpfade werden angeboten
- Tippfehler werden vermieden

Beispiel:

```text
Choose Local Path
────────────────────────────────────────────────────────────

Project Root: /path/to/project

▶ plugins/
  themes/
  custom/
  src/
  public/

[Enter] open/select  [Backspace] up  [Esc] cancel
```

Empfehlung für den MVP:

- nur Verzeichnisse browsen, keine Dateien
- Auswahl setzt immer einen lokalen Mapping-Basisordner

---

## Architekturvorschlag

Kein neues eigenes Screen-Package im ersten Schritt.

Stattdessen in `internal/tui/hostform/` bleiben und die bestehende Subscreen-Logik erweitern.

### Neue Subscreens

In `internal/tui/hostform/model.go`:

- `subMain`
- `subMappingList`
- `subMappingEdit`
- `subMappingPickLocal` **neu**

Optional später:

- `subMappingPickRemote`

### Warum hier und nicht neues Package?

Vorteile:

- minimaler Eingriff in `internal/tui/app.go`
- passt zum bestehenden Muster von `hostform`
- schneller umsetzbar
- leicht rückbaubar oder später extrahierbar

---

## Konkreter Umsetzungsplan pro Datei

## 1. `internal/tui/hostform/model.go`

### Ziel

State für Wizard, Local-Picker, Vorschläge und Preview ergänzen.

### Änderungen

#### a) `subScreen` erweitern

Ergänzen:

```go
subMappingPickLocal
```

#### b) Zusätzlicher State für Mapping-Editor

Vorschlag:

```go
type mappingEditMode int

const (
	mappingEditManual mappingEditMode = iota
	mappingEditBrowse
)
```

Im `Model` ergänzen:

```go
editMode mappingEditMode
editErr  string

suggestedRemote []string
suggestCursor    int

previewLocalSample  string
previewRemoteSample string
```

#### c) State für lokalen Picker

Im `Model` ergänzen:

```go
pickRoot    string
pickCurrent string
pickEntries []string
pickCursor  int
pickErr     string
```

Optional robuster:

```go
type pickerEntry struct {
	Name  string
	Path  string // relativ zum Projekt-Root
	IsDir bool
}
```

Dann:

```go
pickEntries []pickerEntry
```

Das wäre besser lesbar und später erweiterbar.

#### d) Hilfsfunktionen ergänzen

Neue Methoden in `model.go` oder ausgelagert in neue Datei `mapping_helpers.go`:

- `openMappingEdit(idx int)` erweitern
- `openLocalPicker()`
- `loadLocalPickerEntries()`
- `applyLocalSelection(rel string)`
- `updateSuggestedRemote()`
- `updateMappingPreview()`
- `validateMappingEdit() string`

### Verhalten

Wenn `Local Path` gesetzt oder geändert wird:

- Remote-Vorschläge neu berechnen
- Preview neu berechnen
- Validierung neu berechnen

Wenn ein Vorschlag gewählt wird:

- `Remote Path` direkt setzen
- Preview aktualisieren

---

## 2. `internal/tui/hostform/update.go`

### Ziel

Neue Zustände und Tastatursteuerung für Picker/Wizard ergänzen.

### Änderungen

#### a) `handleKey()` erweitern

Neue Route:

```go
case subMappingPickLocal:
	return m.handleMappingPickLocal(msg)
```

#### b) `handleMappingList()` erweitern

Neue Shortcuts:

- `p` / `b`: neues Mapping direkt mit Picker starten
- `e`: bestehendes Mapping im Editor öffnen
- `n`: neues leeres Mapping wie bisher

Vorschlag:

- `n` = manueller Editor
- `p` = Editor öffnen und sofort Picker starten

#### c) `handleMappingEdit()` erweitern

Neue Shortcuts:

- `b`: lokalen Picker öffnen
- `left/right`: ggf. Vorschlagsliste wechseln
- `enter`: je nach Fokus Feld weiter / Vorschlag übernehmen / speichern
- `ctrl+s`: speichern, aber nur wenn Validierung erfolgreich ist

Empfohlene Fokusreihenfolge im Editor:

1. `Local Path`
2. `Remote Path`
3. `Suggested remote` (virtuell)
4. `Save` (virtuell, optional)

Für den MVP kann es einfacher bleiben:

- 2 echte Textfelder
- `b` öffnet Picker
- `j/k` wechseln Vorschläge
- `tab` springt nur durch Textfelder
- `enter` auf Vorschlag übernimmt ihn nur, wenn Fokus dort ist

#### d) Neuer Handler `handleMappingPickLocal()`

Benötigte Tasten:

- `j/down`: runter
- `k/up`: hoch
- `enter`: Ordner öffnen oder auswählen
- `backspace`: eine Ebene hoch
- `esc`: zurück zum Editor

Pragmatisches Verhalten:

- `enter` auf Ordner öffnet ihn
- `ctrl+s` oder `a` übernimmt aktuellen Ordner als Mapping-Basis

Alternative UX:

- `enter` übernimmt direkt den markierten Ordner
- `l` oder `right` öffnet Ordner

Für Bubble Tea ist folgende Belegung meist angenehm:

- `enter/right` = öffnen
- `a` = aktuellen relativen Pfad übernehmen
- `backspace/left` = hoch

---

## 3. `internal/tui/hostform/view.go`

### Ziel

Die neue Picker-/Wizard-Ansicht visuell klar machen.

### Änderungen

#### a) `View()` erweitern

Zusätzlicher Fall:

```go
case subMappingPickLocal:
	return m.viewMappingPickLocal()
```

#### b) `viewMappingList()` leicht verbessern

Ergänzen:

- Hinweis auf neuen Picker-Shortcut
- optional aktuelle Mapping-Count-Badge

#### c) `viewMappingEdit()` deutlich ausbauen

Zusätzliche Bereiche:

- lokale Hilfe: „relative to project root“
- Remote-Hilfe: „absolute path on server“
- Vorschlagsliste
- Preview
- Validierung/Fehler

Wichtig:

- vorhandene Projektkonventionen respektieren
- keine Inline-Styles erfinden, sondern bestehende `internal/styles`-Styles nutzen

Falls nötig, neue Styles zentral ergänzen in:

- `internal/styles/styles.go`
- oder `internal/tui/styles.go`

#### d) Neue View `viewMappingPickLocal()`

Anzeige:

- aktueller relativer Pfad
- Liste der Verzeichnisse
- markierter Eintrag
- Footer mit Shortcuts

Optional:

- oberste Zeile `.` / `..`
- versteckte/verrauschte Ordner filtern

---

## 4. Neue Hilfsdatei: `internal/tui/hostform/mapping_helpers.go`

### Ziel

Nicht zu viel Business-/Path-Logik in `update.go` oder `view.go` unterbringen.

### Inhalt

Vorgeschlagene Hilfsfunktionen:

- `normalizeLocalMappingPath(projectRoot, value string) string`
- `normalizeRemoteMappingPath(value string) string`
- `suggestRemotePaths(rootPath, local string) []string`
- `samplePreviewPath(localBase, remoteBase string) (string, string)`
- `validateMapping(local, remote string) error`

### Hinweise

Die Regeln sollen einfach und nachvollziehbar bleiben.

Keine spekulative große Abstraktion; nur auslagern, was die `hostform`-Dateien wirklich entlastet.

---

## 5. Neue Hilfsdatei optional: `internal/tui/hostform/localpicker.go`

Falls der Picker mehr als ~50 Zeilen Logik bekommt, auslagern.

Mögliche Funktionen:

- `readLocalPickerEntries(projectRoot, currentRel string) ([]pickerEntry, error)`
- Sortierung von Verzeichnissen
- Filterung unerwünschter Ordner

### Filterempfehlung

Beim Picker die gleichen oder ähnliche Ausschlüsse wie beim lokalen File-Walker berücksichtigen:

- `.git`
- `.svn`
- `.hg`
- `node_modules`
- `.idea`
- `.vscode`

Damit wird die Navigation deutlich angenehmer.

---

## Remote-Vorschläge: Heuristik für den MVP

Die Vorschlagslogik darf simpel sein, soll aber in typischen Projekten sofort helfen.

### Eingabe

- `host.RootPath`
- `Local Path`

### Beispiele

Wenn `RootPath = /var/www/html`:

- `plugins/Foo` → `/var/www/html/custom/plugins/Foo`
- `themes/Bar` → `/var/www/html/custom/themes/Bar`
- `src/Baz` → `/var/www/html/src/Baz`
- `public/assets` → `/var/www/html/public/assets`
- sonst Fallback: `/var/www/html/<local>`

### Wichtige Regel

Mapping-`Remote` bleibt **absolut**.

Das muss in der UI explizit sichtbar sein, damit es keine Verwechslung mit `RootPath`-relativen Pfaden gibt.

---

## Validierung

Beim Speichern eines Mappings sollten mindestens diese Regeln gelten:

### `Local`

- darf nicht leer sein
- muss relativ sein
- sollte auf einen Pfad unterhalb des Projekt-Roots zeigen
- sollte normalisiert werden (`filepath.Clean`)

### `Remote`

- darf nicht leer sein
- muss absolut sein
- sollte normalisiert werden

### Warnungen statt Hard Errors

Optional sinnvoll:

- überlappende Mappings erkennen
- doppelten lokalen Basisordner erkennen
- doppelten Remote-Basisordner erkennen

Beispiele:

- `plugins`
- `plugins/MyPlugin`

Das kann erlaubt bleiben, sollte aber sichtbar sein.

---

## Preview

Ein großes UX-Plus bei wenig Aufwand.

### Idee

Für das aktuelle Mapping immer ein Beispiel anzeigen.

Wenn `Local = plugins/MyPlugin` und `Remote = /var/www/html/custom/plugins/MyPlugin`:

```text
Preview
plugins/MyPlugin/src/Example.php
→ /var/www/html/custom/plugins/MyPlugin/src/Example.php
```

Falls noch kein sinnvoller Sample bekannt ist, reicht auch:

```text
plugins/MyPlugin/...
→ /var/www/html/custom/plugins/MyPlugin/...
```

Das macht die Übersetzungslogik sofort verständlich.

---

## Tests

Da hier TUI-State plus Pfadlogik zusammenkommt, würde ich nicht versuchen, alles über UI-Tests abzudecken.

### Stattdessen gezielt Logik testen

Neue Tests für Helper-Funktionen, z. B.:

- `suggestRemotePaths()`
- `validateMapping()`
- Normalisierung lokaler Pfade
- Preview-Erzeugung

Möglicher Ort:

- `internal/tui/hostform/mapping_helpers_test.go`

### Beispiele

- relativer lokaler Pfad wird akzeptiert
- absoluter lokaler Pfad wird abgelehnt
- relativer Remote-Pfad wird abgelehnt
- Plugin-/Theme-Heuristik liefert sinnvolle Vorschläge

---

## Inkrementelle Umsetzung

### Phase 1 — kleiner, sicherer MVP

- `subMappingPickLocal` ergänzen
- lokalen Verzeichnis-Picker bauen
- `viewMappingEdit()` um `Browse`, `Preview`, `Validation` erweitern
- Remote-Vorschläge aus lokalem Pfad + `RootPath`

### Phase 2 — UX-Feinschliff

- bessere Vorschlagsliste
- Warnungen bei Mapping-Konflikten
- besseres Inline-Feedback in der Mapping-Liste

### Phase 3 — optional später

- Remote-Verzeichnis-Browser
- echtes Zwei-Spalten-Mapping
- Test-/Probeauflösung gegen echte Remote-Struktur

---

## Empfohlene Umsetzung für `drift`

Für den ersten Wurf würde ich bewusst **keinen** kompletten Split-View mit Remote-Browser bauen.

Stattdessen:

1. bestehende `hostform`-Architektur behalten
2. lokalen Picker ergänzen
3. Remote-Vorschläge auf Basis von `RootPath` hinzufügen
4. Preview + Validierung im Editor anzeigen

Das ist klein genug für einen sauberen PR und bringt schon sehr viel UX-Gewinn.

---

## Betroffene Dateien

Geplante Änderungen:

- `internal/tui/hostform/model.go`
- `internal/tui/hostform/update.go`
- `internal/tui/hostform/view.go`
- `internal/tui/hostform/mapping_helpers.go` **neu**
- `internal/tui/hostform/mapping_helpers_test.go` **neu**
- optional `internal/tui/hostform/localpicker.go` **neu**
- optional zentrale Styles in `internal/styles/styles.go` oder `internal/tui/styles.go`

Dokumentation dieses Plans:

- `docs/mapping-picker-plan.md`