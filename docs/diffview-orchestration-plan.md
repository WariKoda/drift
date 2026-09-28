# Implementierungsplan: Laden und Sync aus `diffview` lösen

Status: geplant, noch nicht begonnen.
Planungsstand: `main` nach `bd5190d` plus `chore/remove-dead-sync-state`.
Bezug: Phase 2 und 3 in `docs/architecture-target.md`.

## Ziel

`internal/tui/diffview` soll nur noch Interaktion und Darstellung enthalten.
Aufbau der Vergleichssitzungen, Refresh und Sync-Ausführung wandern in Pakete
ohne Bubble-Tea-Abhängigkeit. Danach lassen sich diese Abläufe direkt testen,
und ein späterer nicht-interaktiver Modus kann sie aufrufen.

Das Verhalten für Benutzer ändert sich nicht. Jeder Schritt ist ein reines
Verschieben mit angepassten Signaturen und bleibt einzeln releasefähig.

## Ausgangszustand

`internal/tui/diffview/model.go` hat 1.575 Zeilen. Davon gehören etwa 800
Zeilen nicht in einen Screen:

| Stelle | Inhalt |
|---|---|
| `model.go:744` bis `757` | vier Varianten von `LoadCmd` mit zehn oder elf Parametern |
| `model.go:757` bis `1078` | `loadCmdWithOptions`: Root öffnen, verbinden, Auswahl scannen, lokal und remote walken, Git-Klassifikation, Scope zählen |
| `model.go:1080` bis `1233` | Worker-Grenzen und `forEachCompare`, das für FTP selbst `remote.Connect` aufruft |
| `model.go:1235` bis `1302` | `loadDiffItems`: Vergleich ausführen, Zertifikatsfehler herausfiltern |
| `model.go:1327` bis `1364` | `uploadFile`, `downloadFile`, `syncOperationError` |
| `model.go:1367` bis `1497` | `uploadCmd`, `downloadCmd`, `bulkSyncCmd` mit der Sync-Schleife |
| `model.go:1501` bis `1575` | `refreshCmd` und `reloadSessionCmd` |
| `load_activity.go` | Leerlauf-Timeout, Ressourcenbesitz während des Ladens, `loadClient`-Wrapper |

`diffview` importiert dafür `pathmap`, `remote`, `tlstrust` und ruft
`fs.NewClassifier` auf. Der Fortschritts-Tracker `loading.Tracker` liegt in
einem TUI-Paket, das Bubble Tea importiert. Ein Service außerhalb der TUI kann
ihn deshalb nicht verwenden, ohne diese Abhängigkeit mitzuziehen.

## Entscheidungen

**Freie Funktionen statt Service-Typen.** `architecture-target.md` skizziert
`SessionService` und `SyncService` als Structs. Beide hätten keinen eigenen
Zustand, alle Abhängigkeiten kommen pro Aufruf. Deshalb werden es Funktionen
mit Request-Struct. Braucht später ein Aufruf langlebige Abhängigkeiten, wird
daraus ein Typ. Das Zieldokument wird entsprechend angepasst.

**Paketnamen folgen dem Zieldokument.** Laden und Refresh kommen nach
`internal/app`, die Sync-Ausführung nach `internal/sync`. Das Paket `sync`
verdeckt weiter die Standardbibliothek. Eine Umbenennung in `syncpolicy` ist
ein eigener Schritt und gehört nicht in diesen Plan.

**Der Tracker zieht in ein eigenes Paket.** `Tracker` und `Progress` wandern
unverändert nach `internal/progress`. `loading` behält nur Anzeige und
Tick-Logik. So nehmen die neuen Funktionen `*progress.Tracker` direkt entgegen.
Ein Interface nur für diese Grenze wäre eine Abstraktion mit genau einer
Implementierung.

**Messages bleiben in `diffview`.** `MsgDiffLoaded`, `MsgDiffError`,
`MsgRefreshed` und die Sync-Messages sind TUI-Verträge. Die neuen Funktionen
geben Werte und Fehler zurück, die dünnen `tea.Cmd`-Wrapper in `diffview`
übersetzen sie in Messages. `RequestID` bleibt eine reine TUI-Angelegenheit.

**Befehlslebensdauer bleibt in `diffview`.** `commandLifetime`,
`trackCommand` und `Close` regeln, wann ein Screen seine Ressourcen freigibt.
Das ist Screen-Lebenszyklus und bleibt, wo es ist.

## Zielschnitt

### `internal/progress`

```go
type Progress struct {
	Phase         string
	Done          int
	Total         int
	Indeterminate bool
}

type Tracker struct { /* unverändert aus loading */ }

func NewTracker(phase string) *Tracker
func (t *Tracker) Context() context.Context
func (t *Tracker) Cancel()
func (t *Tracker) Canceled() bool
func (t *Tracker) Set(phase string, done, total int, indeterminate bool)
func (t *Tracker) Inc()
func (t *Tracker) Finish()
func (t *Tracker) Snapshot() (Progress, bool)
```

`loading.IsCanceled` wandert mit. Alle Methoden bleiben nil-sicher.

### `internal/app`

```go
type LoadRequest struct {
	Host        config.Host
	Config      *config.MergedConfig
	Local       *fs.SelectionState
	Remote      *fs.SelectionState
	Options     sync.ScopeOptions
	Conn        remote.Client        // optional, Besitz geht an Load über
	Trust       *tlstrust.Manager
	Required    *tlstrust.Challenge  // nur beim ersten Retry nach Zertifikatsabfrage
	IdleTimeout time.Duration        // 0 bedeutet 60 Sekunden
}

type LoadResult struct {
	Sessions []diff.Session
	Conn     remote.Client // Aufrufer schließt
	Root     *fs.Root      // Aufrufer schließt
	Scope    sync.ScopeSummary
}

var ErrIdleTimeout = errors.New("diff comparison inactivity timeout")

func Load(ctx context.Context, req LoadRequest, prog *progress.Tracker) (LoadResult, error)

type RefreshRequest struct {
	Host     config.Host
	Conn     remote.Client
	Root     *fs.Root
	Sessions []diff.Session
	Trust    *tlstrust.Manager
}

// Refresh behält Reihenfolge und Anzahl der Sessions bei, auch identische.
func Refresh(ctx context.Context, req RefreshRequest, prog *progress.Tracker) ([]diff.Session, error)
```

Besitzregeln für `Load`:

- Bei Erfolg gehören `Conn` und `Root` dem Aufrufer.
- Bei jedem Fehler schließt `Load` alles, was es besitzt. Dazu zählt eine über
  `req.Conn` übergebene Verbindung, genau wie heute `activity.own(existingConn)`.
- Ein `tlstrust.VerificationError` kommt unverändert als Fehler zurück, damit
  die Root-App weiter `certtrust` öffnen kann.
- Fehler pro Datei bleiben `diff.Session.Err`. Nur Verbindungs-, Abbruch-,
  Timeout- und Zertifikatsfehler beenden `Load`.

`Refresh` übernimmt keinen Besitz. Ein Verbindungsfehler kommt als Fehler
zurück, zusammen mit den bis dahin berechneten Sessions, wie heute in
`MsgRefreshed`. Die Signatur wird dafür `([]diff.Session, error)` mit
gefülltem Slice auch im Fehlerfall.

Interne Dateien in `internal/app`:

| Datei | Inhalt | Herkunft |
|---|---|---|
| `load.go` | `LoadRequest`, `LoadResult`, `Load` | `loadCmdWithOptions` |
| `scan.go` | Auswahl expandieren, Paare sammeln, klassifizieren, Scope zählen | Rumpf von `loadCmdWithOptions` |
| `compare.go` | `forEachCompare`, `loadDiffItems`, Worker-Grenzen | `model.go:1080` bis `1302` |
| `refresh.go` | `Refresh` | `refreshCmd` |
| `activity.go` | `loadActivity`, `loadClient`, `loadReader`, lokaler Walk | `load_activity.go` |

### `internal/sync`

```go
type Item struct {
	LocalPath  string
	RemotePath string
	Decision   Decision
}

type Failure struct {
	Operation string
	Path      string
	Reason    string
	Err       error
}

type Result struct {
	Completed []int     // Indizes in items mit bestätigtem Erfolg
	Failures  []Failure
	Err       error     // Abbruch oder terminaler Verbindungsfehler
}

func Run(ctx context.Context, conn remote.Client, root *fs.Root, items []Item, prog *progress.Tracker) Result
```

`Run` enthält die heutige Schleife aus `bulkSyncCmd` samt `uploadFile`,
`downloadFile` und `syncOperationError`. `Failure` ersetzt
`diffview.SyncFailure`, auch in `tui.App.pendingDiffErrors` und
`MsgScopeReloadRequested`.

## Umsetzungsschritte

Jeder Schritt ist ein eigener Branch und PR. Vor jedem Merge laufen
`go test ./...`, `go test -race ./...`, `go vet ./...` und `go build ./...`.

### Schritt 1: Tracker nach `internal/progress`

Branch `refactor/progress-tracker`.

1. `Tracker`, `Progress` und `IsCanceled` nach `internal/progress` verschieben.
2. `loading` importiert `progress` für seine Anzeige.
3. Alle Aufrufer umstellen: `browser`, `hostmanager`, `diffview`, `app.go`,
   `app_certtrust.go`.
4. `diffview.LoadProgressTracker` und `NewLoadProgressTracker` entfernen, die
   Aufrufer nutzen `progress.NewTracker("Connecting…")` direkt.

Rein mechanisch, keine Verhaltensänderung.

### Schritt 2: Laden nach `internal/app`

Branch `refactor/app-load`.

1. `internal/app` mit `activity.go`, `compare.go`, `scan.go` und `load.go`
   anlegen. Code zunächst 1:1 übernehmen, nur Signaturen anpassen.
2. `loadCmdWithOptions` wird ein Wrapper von etwa 20 Zeilen: `app.Load`
   aufrufen, Ergebnis in `MsgDiffLoaded` oder Fehler in `MsgDiffError`
   übersetzen.
3. Die vier `LoadCmd`-Varianten zu einer zusammenfassen:
   `LoadCmd(requestID uint64, req app.LoadRequest, prog *progress.Tracker) tea.Cmd`.
   Die Testvariante mit `idleTimeout` entfällt, Tests setzen
   `LoadRequest.IdleTimeout`.
4. `ErrDiffIdleTimeout` wird `app.ErrIdleTimeout`.
5. Tests verschieben:
   - `load_activity_test.go`, `load_activity_irregular_test.go`,
     `scope_test.go`, `ftp_folder_test.go` nach `internal/app`
   - aus `model_test.go` `TestLoadDiffItemsUsesSingleFTPSession` und
     `TestForEachCompareAddsExtraFTPConnections`
   - aus `connection_test.go` `TestForEachComparePropagatesExtraWorkerTerminalFailure`
     und `TestCompareCancellationClosesExtraWorker`
   Tests, die bisher über `loadCmd(...)()` eine Message auspacken, rufen
   `app.Load` direkt auf und prüfen Wert und Fehler.

Nach diesem Schritt importiert `diffview` weder `pathmap` noch ruft es
`remote.Connect` oder `fs.NewClassifier` auf.

### Schritt 3: Refresh nach `internal/app`

Branch `refactor/app-refresh`.

1. `app.Refresh` aus `refreshCmd` bauen.
2. `refreshCmd` und `reloadSessionCmd` werden Wrapper. `reloadSessionCmd`
   ruft `app.Refresh` mit genau einer Session auf. `forEachCompare` begrenzt
   die Worker auf die Anzahl der Jobs, eine zusätzliche FTP-Verbindung
   entsteht dadurch nicht.
3. Die Prüfung "Verbindung schon tot" vor dem Start bleibt im Wrapper, weil
   sie `Model.connectionError` mit dem gespeicherten `disconnected`-Zustand
   braucht.

### Schritt 4: Sync-Ausführung nach `internal/sync`

Branch `refactor/sync-run`.

1. `sync.Item`, `sync.Failure`, `sync.Result` und `sync.Run` anlegen.
2. `bulkSyncCmd` baut aus `indices` und `syncDirs` eine `[]sync.Item`, merkt
   sich die Zuordnung Item-Index zu Session-Index und ruft `sync.Run` auf.
3. `uploadCmd` und `downloadCmd` rufen `sync.Run` mit einem Item auf und
   übersetzen `Result` in `MsgSynced` oder `MsgSyncError`.
4. `diffview.SyncFailure` durch `sync.Failure` ersetzen.
5. Die heutige Abbruchprüfung `m.connectionError()` in der Schleife ersetzt
   `Run` durch `conn.Err()`, `conn.Done()` und `ctx.Err()`. Das ist
   gleichwertig: `Model.ConnectionLost` setzt `disconnected` und bricht
   zugleich den Tracker ab, dessen Context `Run` bekommt.
6. Tests für die Schleife nach `internal/sync` verschieben oder dort neu
   schreiben, gegen echte lokale Server wie in den bestehenden
   Transporttests.

### Schritt 5: Aufräumen und Doku

Branch `docs/diffview-orchestration`.

1. `AGENTS.md`: Architekturabschnitt um `internal/app` und `internal/progress`
   ergänzen, Key Types um `app.LoadRequest` und `sync.Run` erweitern.
2. `docs/architecture-target.md`: Phase 2 und die ersten Punkte von Phase 3 als
   umgesetzt markieren, Service-Structs durch die Funktions-APIs ersetzen.
3. Diesen Plan auf Status "umgesetzt" setzen.

## Verhalten, das erhalten bleiben muss

Diese Punkte sind heute durch Tests abgesichert oder ergeben sich aus
`AGENTS.md`. Nach jedem Schritt müssen sie weiter gelten.

- Leerlauf-Timeout von 60 Sekunden während des Ladens. Aktive Transfers
  verlängern ihn, ein hängender Transfer bricht ab.
- Abbruch oder Timeout schließen die primäre und alle zusätzlichen
  FTP-Verbindungen.
- Eine erfolgreiche Übergabe überlebt einen späten Abbruch
  (`TestLoadActivitySuccessfulHandoffSurvivesLateCancellation`).
- Eine abgelehnte zusätzliche FTP-Anmeldung senkt nur die Parallelität. Ein
  Zertifikatsfehler dabei beendet den Vergleich.
- Ein `tlstrust.VerificationError` aus Scan, Vergleich oder Refresh erreicht
  die Root-App als Fehler und öffnet `certtrust`.
- Verspätete Ergebnisse eines abgebrochenen Requests werden verworfen, ihre
  Ressourcen geschlossen. Das bleibt in `app.go` über `RequestID`.
- Bestätigte Sync-Erfolge bleiben auch bei Abbruch oder Verbindungsverlust
  erhalten (`TestLateSyncCompletionKeepsConfirmedOutcomes`).
- Nach Verbindungsverlust wird kein Transfer automatisch wiederholt. Ein
  Schreibfehler mit terminaler Verbindung meldet "outcome unknown".
- Lokale Zugriffe laufen ausschließlich über `fs.Root`.
- Fehler werden über `internal/log` protokolliert, in den neuen Paketen an
  denselben Stellen wie heute.

## Abnahme

- `wc -l internal/tui/diffview/model.go` liegt unter 850 Zeilen.
- `grep -rn "remote.Connect\|pathmap\.\|fs.NewClassifier" internal/tui/diffview`
  findet nur Tests oder nichts.
- `internal/app` und `internal/sync` importieren weder `bubbletea` noch ein
  Paket unter `internal/tui`.
- Alle verschobenen Tests laufen im neuen Paket, keiner wurde gelöscht, ohne
  dass ein gleichwertiger Test existiert.
- `go test -race ./...` ist grün.
- Manuelle Prüfung gegen einen SFTP- und einen FTPS-Host: Vergleich laden,
  Refresh, Einzel-Upload, Bulk-Sync mit einem Löschvorgang, Abbruch mit `Esc`
  während eines langen Vergleichs, Zertifikatsabfrage bei neuem FTPS-Host.

## Nicht im Umfang

- `diffview.SyncDir` durch `sync.Decision` ersetzen. Die Umrechnung
  `syncDirFromDecision` und `decisionFromSyncDir` wäre danach überflüssig, das
  ist aber ein eigener Schritt mit Auswirkungen auf View und Tests.
- `scan.go` in kleinere Funktionen zerlegen. Erst verschieben, dann
  umbauen, damit die Reviews lesbar bleiben.
- Umbenennung von `internal/sync` in `syncpolicy`.
- Wiederverwendung zusätzlicher FTP-Verbindungen über mehrere Vergleiche.
- Ein expliziteres Presence-Modell in `internal/diff` (Phase 5 im Zieldokument).
- `App.Update` in weitere Dateien aufteilen.
