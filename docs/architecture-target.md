# Zielarchitektur für drift

Diese Skizze beschreibt eine mittelfristige Zielarchitektur für `drift`, die die aktuelle Stärken des Projekts beibehält, aber die fachliche Sync-/Diff-Logik klarer von BubbleTea, Transport und Dateisystemzugriff trennt.

Sie ist bewusst evolutionär formuliert: kein Big-Bang-Rewrite, sondern eine realistische Leitplanke für die nächsten Entwicklungszyklen.

## Aktueller Stand

Bereits umgesetzt:
- `internal/pathmap` matcht Mapping-Prefixe segment-sicher
- `diff.Compare()` behandelt nur echte NotFound-Fälle als `LocalOnly` / `RemoteOnly`
- andere Stat-/Protokollfehler bleiben als Fehler sichtbar
- `diffview.nextDir()` lässt für fehlerhafte Sessions keine Action-Auswahl mehr zu
- Auto-Decision- und Action-Cycling-Logik wurde nach `internal/sync/policy.go` verschoben und dort getestet

Noch offen:
- deterministische Sortierung für Hosts und markierte Pfade
- Einführung der `internal/app`-Services für Session-Aufbau und Refresh
- Ersetzen der Sync-Ausführung in `diffview` durch `sync`-/`app`-Services

---

## Ziele

- UI, Anwendungsschicht und fachliche Sync-Logik sauber trennen
- Diff- und Sync-Workflows außerhalb von BubbleTea testbar machen
- neue Protokolle wie WebDAV oder rsync leichter ergänzen
- Konflikte, Deletes, Skip und Auto-Entscheidungen explizit modellieren
- Progress, Cancellation und Fehlerbehandlung sauber zentralisieren

---

## Leitprinzipien

### 1. TUI ist Orchestrator der Interaktion, nicht der Fachlogik

`internal/tui/*` soll:
- User-Eingaben verarbeiten
- Screens rendern
- typed messages austauschen
- Commands starten
- Ergebnisse anzeigen

Die TUI soll **nicht** selbst:
- Sessions zusammenbauen
- Sync-Pläne fachlich berechnen
- Transport-/Dateisystemdetails koordinieren
- Konfliktregeln definieren

### 2. Sync und Diff sind Application-/Domain-Logik

Die Kernfragen des Produkts sind fachlich, nicht UI-spezifisch:
- Welche lokalen Dateien gehören zu welchem Remote-Pfad?
- Welche Sessions existieren für eine Auswahl?
- Welche Datei ist nur lokal, nur remote oder konfliktbehaftet?
- Welche Aktion ist vorgeschlagen?
- Was passiert bei Upload, Download, Delete oder Skip?

Diese Logik soll in UI-unabhängigen Paketen liegen.

### 3. Transport ist austauschbar

SFTP, FTP, FTPS und später WebDAV oder rsync sollen an klaren Interfaces hängen.

### 4. Correctness vor Convenience

Pfadmapping, Existenzprüfung, Delete-Verhalten und Konfliktmodell müssen explizit und robust sein.

---

## Empfohlener Package-Zuschnitt

## Überblick

```text
internal/
  app/              # Application-Services / Use-Cases
  config/           # TOML-Konfiguration + Persistenz
  diff/             # fachliche Vergleichslogik + Diff-Ergebnisse
  fs/               # lokales Filesystem
  pathmap/          # local <-> remote Pfadübersetzung
  remote/           # transportagnostische Interfaces + Registry/Factory
  sync/             # Sync-Modell, Plan, Engine, Policies, Progress
  tui/              # BubbleTea Root + Screens + Presenter/ViewModel-Helfer

  ftp/              # FTP/FTPS Driver
  sftp/             # SFTP Driver
  ssh/              # SSH-Auth / known_hosts
```

---

## `internal/app`

Neue Schicht für Use-Cases bzw. Application Services.

### Verantwortung

Hier liegt der Ablauf über mehrere Subsysteme hinweg, z. B.:
- Auswahl -> Session-Liste erzeugen
- Host auswählen -> Verbindung aufbauen -> Diffs laden
- Actions anwenden -> Sync-Engine ausführen -> Ergebnis zurückgeben
- Sessions refreshen

### Empfohlene Services

#### `internal/app/session_service.go`

Beispielhafte API:

```go
type SessionService interface {
    Build(ctx context.Context, req BuildSessionsRequest) (BuildSessionsResult, error)
    Refresh(ctx context.Context, req RefreshSessionsRequest) ([]diff.Session, error)
}
```

`BuildSessionsRequest` enthält z. B.:
- Host
- ProjectRoot
- ProjectMappings
- Auswahl / markierte Pfade

`BuildSessionsResult` enthält z. B.:
- `[]diff.Session`
- offene `remote.Client`-Verbindung, falls weiterverwendet

#### `internal/app/sync_service.go`

Beispielhafte API:

```go
type SyncService interface {
    BuildPlan(req sync.BuildPlanRequest) (sync.Plan, error)
    Run(ctx context.Context, client remote.Client, plan sync.Plan, progress sync.ProgressSink) (sync.RunResult, error)
}
```

### Nutzen

- BubbleTea-Screens werden dünner
- komplexe Flows werden isoliert testbar
- spätere CLI- oder Batch-Modi können dieselben Use-Cases nutzen

---

## `internal/diff`

`internal/diff` sollte die fachliche Vergleichslogik kapseln, nicht die UI.

### Verantwortung

- lokalen und Remote-Stand einer Datei vergleichen
- strukturiertes Ergebnis erzeugen
- Text/Binary unterscheiden
- Existenz-/Fehlerzustände explizit modellieren
- Renderer nur als Hilfskomponente für Textdarstellung behalten

### Empfohlene Modellschärfung

Statt implizit „Stat-Fehler = Datei fehlt“ sollte der Zustand explizit sein.

Beispiel:

```go
type Presence int

const (
    PresenceUnknown Presence = iota
    PresenceMissing
    PresenceExists
)

type SideState struct {
    Presence Presence
    Size     int64
    ModTime  time.Time
    Err      error
}

type CompareResult struct {
    Local      SideState
    Remote     SideState
    Binary     bool
    Lines      []DiffLine
    Difference DifferenceKind
}
```

Mögliche `DifferenceKind`:
- `DifferentNone`
- `DifferentLocalOnly`
- `DifferentRemoteOnly`
- `DifferentText`
- `DifferentBinary`
- `DifferentUnknown`

### Wichtig

`diff.Compare()` sollte nur dann „local only“ oder „remote only“ melden, wenn ein echter NotFound-Fall erkannt wurde. Permission-, Netzwerk- oder Protokollfehler müssen gesondert sichtbar bleiben.

Status: **teilweise umgesetzt**
- echte NotFound-Fälle werden getrennt behandelt
- FTP `550` wird als Missing erkannt
- andere Fehler bleiben sichtbar und werden nicht mehr implizit als „Datei fehlt“ interpretiert
- ein expliziteres Presence-Modell (`Presence`, `SideState`, `DifferenceKind`) ist weiterhin offen

---

## `internal/sync`

Dieses Paket sollte die eigentliche Sync-Domain werden. Aktuell ist es dafür angelegt, aber noch nicht ausgebaut.

### Verantwortung

- fachliche Sync-Aktionen modellieren
- Plan aus Sessions + User-Entscheidungen bauen
- Progress modellieren
- Sync-Engine ausführen
- Konflikt-Policies und Auto-Entscheidungen kapseln

### Empfohlene Typen

#### Aktionen

```go
type Action int

const (
    ActionNone Action = iota
    ActionUpload
    ActionDownload
    ActionDeleteLocal
    ActionDeleteRemote
    ActionConflict
    ActionManual
)
```

`ActionConflict` und `ActionManual` müssen nicht sofort operativ verwendet werden, sind aber als Zielmodell hilfreich.

#### Sessionzustand / Entscheidung

```go
type FileState int

const (
    StateIdentical FileState = iota
    StateLocalOnly
    StateRemoteOnly
    StateDifferentText
    StateDifferentBinary
    StateConflict
    StateUnknown
)
```

#### Policy

```go
type DecisionPolicy struct {
    PreferNewerMTime bool
    AmbiguousIsNone  bool
    DeleteEnabled    bool
}
```

#### Plan

```go
type PlanItem struct {
    LocalPath  string
    RemotePath string
    Action     Action
}

type Plan struct {
    Host  config.Host
    Items []PlanItem
}
```

### Engine

Empfohlene Struktur:

- `planner.go` – baut Plan aus Sessions und gewählten Actions
- `policy.go` – berechnet Vorschläge / Auto-Entscheidungen
- `engine.go` – führt Plan aus
- `progress.go` – Events / Aggregation / Status

### Run-Modell

```go
type ProgressEvent struct {
    ItemIndex int
    Action    Action
    Status    ItemStatus
    BytesDone int64
    BytesTotal int64
    Err       error
}

type ProgressSink interface {
    OnProgress(ProgressEvent)
}
```

### Nutzen

- spätere Progress-Ansicht wird trivialer
- Cancellation über `context.Context` sauber möglich
- Bulk-Sync und Single-File-Sync nutzen dieselbe Engine

---

## `internal/remote`

`internal/remote` ist bereits die richtige Boundary, sollte aber leicht weiterentwickelt werden.

### Verantwortung

- transportagnostische Interfaces
- Verbindungsaufbau abstrahieren
- optional Registry für Protokoll-Driver

### Zielbild

#### Client

Das bestehende Interface ist ein guter Start. Langfristig könnte es leicht feiner werden, falls Protokolle stark variieren.

```go
type Client interface {
    Stat(path string) (os.FileInfo, error)
    ReadFile(path string) ([]byte, error)
    WriteFile(path string, data []byte) error
    UploadFile(local, remote string) error
    DownloadFile(remote, local string) error
    WalkFiles(root string, fn func(string) error) error
    DeleteFile(path string) error
    Close() error
}
```

### Driver-Registry statt hartem `switch`

Mittelfristig:

```go
type Driver interface {
    Connect(ctx context.Context, host config.Host) (Client, error)
}

func Register(protocol string, driver Driver)
func Connect(ctx context.Context, host config.Host) (Client, error)
```

### Nutzen

- neue Protokolle ohne zentrale Switch-Ausweitung
- bessere Trennung zwischen Factory und Protokollimplementierung

---

## `internal/fs`

### Verantwortung

- lokales Lesen / Walken / Löschen / Statten
- klar definierte lokale Filesystem-Boundary

### Zielbild

Wenn Testbarkeit priorisiert wird, kann ein kleines Interface helfen:

```go
type LocalFS interface {
    Stat(path string) (os.FileInfo, error)
    ReadFile(path string) ([]byte, error)
    Remove(path string) error
    WalkFiles(root string, fn func(string) error) error
}
```

Nicht überall nötig, aber an Orchestrierungsgrenzen sehr nützlich.

---

## `internal/pathmap`

Dieses Paket ist bereits konzeptionell gut positioniert und sollte ein zentraler fachlicher Baustein bleiben.

### Verantwortung

- lokaler absoluter Pfad -> Remote-Pfad
- Remote-Pfad -> lokaler absoluter Pfad
- Host-Mappings vs. Projekt-Mappings korrekt auflösen

### Wichtige Verbesserung

Prefix-Matching muss segment-sicher sein.

Beispielproblem:
- Mapping-Basis: `/project/foo`
- Datei: `/project/foobar/index.php`

Das darf nicht matchen.

Status: **umgesetzt**
- lokale und Remote-Pfade matchen nur noch bei exakter Gleichheit oder echtem Unterpfad
- Segmentgrenzen sind durch Tests abgesichert

---

## `internal/config`

Die aktuelle Struktur ist für den Stand des Projekts gut. Für mittelfristige Erweiterbarkeit sollte sie aber leicht vorbereitet werden.

### Aktuelle Stärken

- globale + projektbezogene Configs
- Host-Override per Name
- getrennte Mappings
- Auth-Konfiguration grundsätzlich klar

### Mittelfristige Verbesserungen

#### 1. Protokollspezifische Optionen vorbereiten

Aktuell steckt alles direkt in `config.Host`. Für künftige Protokolle kann das zu breit werden.

Mögliche Richtung:

```go
type Host struct {
    Name      string
    Protocol  string
    Hostname  string
    Port      int
    User      string
    RootPath  string
    Auth      Auth
    Mappings  []Mapping
    Options   map[string]string
}
```

Oder typisierter, wenn später nötig.

#### 2. Validation-Schicht ergänzen

Nicht UI-gebunden, sondern z. B.:
- ungültige Mapping-Pfade
- fehlende Credentials je Protokoll
- RootPath-Regeln
- inkompatible Kombinationen

---

## `internal/tui`

### Verantwortung

- Screen-State
- BubbleTea-Update-/View-Logik
- User-Interaktion
- typed messages
- Starten von Commands gegen `app`-Services

### Zielbild

#### Root App

`internal/tui/app.go` bleibt Root-Router.

Sie sollte aber möglichst nur noch:
- aktive Screens halten
- Cross-Screen-Messages verarbeiten
- Services injizieren / referenzieren
- Ergebnisse weiterreichen

Nicht mehr:
- selbst Sync-/Diff-Abläufe implementieren

#### Screen-Pakete

Die heutige Paketaufteilung ist gut und sollte beibehalten werden:
- `browser`
- `hostselector`
- `hostmanager`
- `hostform`
- `diffview`
- später `syncprogress`

#### Presenter-/Formatter-Helfer

Wenn Status-/Badge-/Summary-Logik wächst, lieber kleine formatter helpers nutzen statt View-Dateien aufzublähen.

---

## Empfohlener Datenfluss

## 1. Auswahl -> Diff-Ansicht

```text
browser.Model
  -> MsgSyncRequested(selection)

App
  -> hostselector
  -> MsgHostChosen(host)
  -> SessionService.Build(...)

SessionService
  -> remote.Connect(host)
  -> pathmap.Mapper
  -> fs/local walk
  -> remote walk
  -> diff.Compare(...) pro Session
  -> []diff.Session zurück

App
  -> diffview.New(sessions, host, conn)
```

### Wichtig

Die Session-Erzeugung gehört in `app`/Service-Schicht, nicht in `diffview`.

---

## 2. Diff-Ansicht -> Plan -> Sync

```text
diffview.Model
  -> User setzt gewünschte Action je Datei
  -> MsgSyncRequested(plan-input)

App / SyncService
  -> sync.BuildPlan(sessions, selected actions)
  -> sync.Run(ctx, client, plan, progressSink)
  -> Progress-Events / Ergebnis

App
  -> diffview oder syncprogress updaten
```

### Wichtig

Die TUI soll nicht selbst Upload/Download/Delete-Schleifen besitzen.

---

## 3. Refresh

```text
diffview.Model
  -> MsgRefreshRequested

App / SessionService
  -> Refresh(...)
  -> diff.Compare(...) erneut

App
  -> neue Sessions ins Model geben
```

---

## Vorschlag für konkrete Verantwortlichkeiten je Paket

## `internal/tui/diffview`

Soll langfristig nur noch enthalten:
- aktueller Cursor
- Scrollstate
- User-gewählte Aktionen je Session
- Rendering
- BubbleTea-Keymapping

Soll **nicht** enthalten:
- `remote.Connect`
- `pathmap.New`
- lokale / Remote-Walks
- Upload-/Download-/Delete-Schleifen
- Auto-Decision-Policy

---

## `internal/app/session_service.go`

Soll enthalten:
- Verbindung aufbauen
- markierte Pfade deterministisch sortieren
- lokale Dateien expandieren
- Remote-only-Dateien einsammeln
- Sessions erzeugen
- Refresh erneut ausführen

---

## `internal/sync/policy.go`

Soll enthalten:
- Auto-Vorschlag aus DiffResult + Policy
- Action-Cycling je Dateizustand
- Regeln für Delete-Freigaben
- Umgang mit Ambiguität bei mtime

Status: **teilweise umgesetzt**
- `AutoDecision(...)` und `NextDecision(...)` liegen bereits in `internal/sync/policy.go`
- erste Policy-Tests existieren
- Delete-Policies und ein breiteres Action-/State-Modell sind noch offen

---

## `internal/sync/engine.go`

Soll enthalten:
- Ausführung eines Plans
- Progress-Events
- Fehleraggregation
- Kontextabbruch
- optional serial / parallel strategy

---

## Teststrategie im Zielbild

## Leicht testbar werden sollen

### `internal/pathmap`
- Mapping-Korrektheit
- Segmentgrenzen
- Host- vs. Projekt-Mappings

### `internal/diff`
- Textdiff
- Binary-Erkennung
- Presence-/Error-Modell
- Umgang mit NotFound vs. Permission-Fehler

### `internal/sync`
- Auto-Entscheidungen
- Konfliktfälle
- Action-Cycling
- Plan-Building
- Engine-Verhalten bei Fehlern und Cancellation

### `internal/app`
- Session-Aufbau aus Selektion + Mapping + Remote-Walk
- Refresh-Flows
- deterministische Reihenfolge

## Eher dünn testbar

### `internal/tui/*`
- Fokus auf Update-Logik / Message-Flows
- keine tiefen Netzwerk-/Filesystem-Tests nötig

---

## Minimale Interfaces für bessere Testbarkeit

Wichtig: keine unnötige Abstraktionsflut. Nur an den Orchestrierungsgrenzen.

### ClientFactory

```go
type ClientFactory interface {
    Connect(ctx context.Context, host config.Host) (remote.Client, error)
}
```

### LocalFS

```go
type LocalFS interface {
    Stat(path string) (os.FileInfo, error)
    ReadFile(path string) ([]byte, error)
    Remove(path string) error
    WalkFiles(root string, fn func(string) error) error
}
```

### SessionLoader / SessionService

```go
type SessionService interface {
    Build(ctx context.Context, req BuildSessionsRequest) (BuildSessionsResult, error)
    Refresh(ctx context.Context, req RefreshSessionsRequest) ([]diff.Session, error)
}
```

So bleibt die Produktionsimplementierung einfach, aber Tests werden viel leichter.

---

## Erweiterbarkeit im Zielbild

## Neue Protokolle

### WebDAV

Benötigt vor allem:
- neuen Driver unter `internal/webdav`
- Registrierung im `remote`-Layer
- evtl. WebDAV-spezifische Auth-/Optionsfelder

Weil Orchestrierung und Sync-Engine transportagnostisch sind, bleibt der Rest weitgehend stabil.

### rsync

rsync passt nicht perfekt auf das bestehende Dateioperationen-Interface, weil es eher ein Sync-Mechanismus als ein File-API-Client ist.

Dafür gibt es zwei Wege:

1. `remote.Client` erweitern oder abstrahieren
2. rsync als alternativen `sync.Engine`-Backend betrachten

Für rsync ist Variante 2 meist architektonisch sauberer.

---

## Weitere Diff-Strategien

Mögliche Erweiterungen:
- Plain text diff
- Ignore-whitespace diff
- line-based vs. word-based diff
- binary metadata compare
- hash-basierter quick compare

Diese Strategien sollten in `internal/diff` oder als Option im `app`-Service sitzen, nicht in der TUI.

---

## Ignore-Regeln

Sinnvolle Zielarchitektur:
- lokale Standard-Ignores in `internal/fs`
- projektbezogene Ignore-Regeln aus Config
- Anwendung in Session-Building / Plan-Building, nicht erst in der View

---

## Empfohlene Migrationsreihenfolge

## Phase 1 – sichere, kleine Schritte

1. deterministische Sortierung für Hosts und markierte Pfade
2. ~~`pathmap` segment-sicher machen~~ ✅
3. ~~`diff.Compare()` für NotFound vs. andere Fehler schärfen~~ ✅
4. ~~`autoDir()` / `nextDir()` aus `tui/diffview` nach `internal/sync` verschieben~~ ✅

**Empfohlener nächster Schritt:** Punkt 1, also deterministische Sortierung für Hosts und markierte Pfade.

## Phase 2 – Orchestrierung entkoppeln

5. `internal/app/session_service.go` einführen
6. `diffview.LoadCmd()` auf SessionService umstellen
7. `refreshCmd()` auf Service umstellen

## Phase 3 – Sync-Domain vervollständigen

8. `sync.Plan` + `Action` sauber modellieren
9. `sync.Engine` für Upload/Download/Delete implementieren
10. `bulkSyncCmd()` durch SyncService/Engine ersetzen

## Phase 4 – Progress und Cancellation

11. `syncprogress`-Screen einführen
12. Progress-Events aus Engine einspeisen
13. Abbruch via `context.Context` und UI-Keybinding

## Phase 5 – Protokoll- und Config-Erweiterbarkeit

14. Driver-Registry in `internal/remote`
15. Config-Validation ergänzen
16. protokollspezifische Optionen vorbereiten

---

## Zielbild in einem Satz

`drift` sollte architektonisch auf ein Modell zulaufen, in dem **BubbleTea nur Interaktion und Darstellung übernimmt**, während **Session-Aufbau, Diff-Entscheidung, Sync-Planung und Sync-Ausführung in klar testbaren, UI-unabhängigen Paketen** liegen.
