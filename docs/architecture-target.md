# Zielarchitektur für drift

Diese Skizze beschreibt eine mittelfristige Zielarchitektur für `drift`, die die aktuellen Stärken des Projekts beibehält, aber die fachliche Sync-/Diff-Logik klarer von BubbleTea, Transport und Dateisystemzugriff trennt.

Sie ist bewusst evolutionär formuliert: kein Big-Bang-Rewrite, sondern eine realistische Leitplanke für die nächsten Entwicklungszyklen.

## Aktueller Stand

Bereits umgesetzt:

- `internal/pathmap` matcht Mapping-Prefixe segment-sicher und erzwingt konfigurierte Mappings
- `diff.Compare()` trennt NotFound grundsätzlich von anderen Stat- und Protokollfehlern
- `diffview.nextDir()` lässt für fehlerhafte Sessions keine Action-Auswahl zu
- Auto-Decision- und Action-Cycling-Logik liegt in `internal/sync/policy.go` und ist getestet
- `internal/sync/plan.go` enthält erste Plan- und Progress-Typen, aber noch keine Engine
- Hosts und markierte Pfade werden deterministisch sortiert verarbeitet
- `fs.Root` begrenzt lokale Transfers und Änderungen auf das Projekt und schreibt Downloads atomar
- Remote-Clients verwenden Streams statt lokaler Pfade und überwachen ihre Verbindung selbst
- ein context-basierter Loading-Tracker unterstützt Fortschritt, Abbruch und das Verwerfen verspäteter Ergebnisse
- Mapping- und Keep-alive-Werte werden beim Laden und Schreiben validiert
- Projekte werden über `internal/project` registriert; projektbezogene Konfiguration liegt slug-basiert außerhalb des Arbeitsverzeichnisses
- FTPS-Zertifikatsvertrauen liegt in `internal/tlstrust` und wird vom Root-Modell verwaltet

Einschränkung:

- FTP-Status `550` wird derzeit als Missing interpretiert. Einige Server verwenden `550` auch für Zugriffsfehler. Die Erkennung ist daher nicht in jedem FTP-Fall eindeutig.

Noch offen:

- Einführung der `internal/app`-Services für Session-Aufbau und Refresh
- Ersetzen der Sync-Ausführung in `diffview` durch `sync`-/`app`-Services
- Verschieben der vorhandenen Progress- und Abbruchmechanik an eine UI-unabhängige Orchestrierungsgrenze
- expliziteres Diff-Zustandsmodell für Presence und Fehler

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
  app/              # Application-Services / Use-Cases, noch einzuführen
  config/           # TOML-Konfiguration + Persistenz
  diff/             # fachliche Vergleichslogik + Diff-Ergebnisse
  fs/               # lokales Filesystem mit projektgebundenem Root
  log/              # dateibasierte, standardmäßig deaktivierte Logs
  pathmap/           # local <-> remote Pfadübersetzung
  project/           # Projekt-Registry und Pfadauflösung
  remote/            # transportagnostisches Interface + Factory
  sync/              # Sync-Modell, Plan, Engine, Policies, Progress
  tlstrust/          # FTPS-Zertifikatsprüfung und Ausnahmen
  tui/               # BubbleTea Root + Screens + Presenter/ViewModel-Helfer

  ftp/               # FTP/FTPS Driver
  sftp/              # SFTP Driver
  ssh/               # SSH-Auth / known_hosts
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

Beispielhafte API eines zunächst konkreten Services:

```go
type SessionService struct { /* produktive Abhängigkeiten */ }

func (s *SessionService) Build(ctx context.Context, req BuildSessionsRequest) (BuildSessionsResult, error)
func (s *SessionService) Refresh(ctx context.Context, req RefreshSessionsRequest) ([]diff.Session, error)
```

`BuildSessionsRequest` enthält z. B.:

- Host
- ProjectRoot
- ProjectMappings
- Auswahl / markierte Pfade

`BuildSessionsResult` enthält z. B.:

- `[]diff.Session`
- die offene `remote.Client`-Verbindung, falls sie weiterverwendet wird
- den geöffneten `fs.Root`, solange die Diff-Ansicht ihn für sichere Transfers benötigt

Der Service muss Besitz und Lebensdauer dieser Ressourcen eindeutig festlegen. Bei Fehlern schließt er selbst erzeugte Ressourcen. Bei Erfolg übernimmt die aufrufende Session deren Schließung. Bestehende Verbindungen dürfen nur für dasselbe Projekt und denselben Host wiederverwendet werden. Verbindungsaufbau erfolgt ausschließlich über `remote.Connect(ctx, host, trustManager, requiredChallenge)`.

#### `internal/app/sync_service.go`

Beispielhafte API:

```go
type SyncService struct { /* produktive Abhängigkeiten */ }

func (s *SyncService) BuildPlan(req sync.BuildPlanRequest) (sync.Plan, error)
func (s *SyncService) Run(ctx context.Context, client remote.Client, root *fs.Root, plan sync.Plan, progress sync.ProgressSink) (sync.RunResult, error)
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

- lokale `os.ErrNotExist`-Fälle werden getrennt behandelt
- andere lokale Fehler bleiben sichtbar und werden nicht als „Datei fehlt“ interpretiert
- FTP `550` wird als Missing erkannt, obwohl der Status je nach Server auch einen Zugriffsfehler bezeichnen kann
- große Dateien gleicher Größe werden per SHA-256 verglichen; Größe und identische mtime bilden einen Schnellpfad
- ein expliziteres Presence-Modell (`Presence`, `SideState`, `DifferenceKind`) und eine präzisere protokollspezifische Fehlerklassifikation bleiben offen

---

## `internal/sync`

Dieses Paket sollte die eigentliche Sync-Domain werden. `policy.go` enthält bereits Entscheidungen und Action-Cycling. `plan.go` enthält erste Transfer- und Progress-Typen, wird aber noch nicht von der TUI genutzt. Eine Engine fehlt.

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

`internal/tui/loading` liefert bereits einen threadsicheren Tracker mit `context.Context`. Das Ziel ist nicht ein zweites konkurrierendes Progress-Modell, sondern eine UI-unabhängige Quelle von Events, die der Tracker oder ein späterer `syncprogress`-Screen konsumiert.

```go
type ProgressEvent struct {
    ItemIndex  int
    Action     Action
    Status     ItemStatus
    BytesDone  int64
    BytesTotal int64
    Err        error
}

type ProgressSink interface {
    OnProgress(ProgressEvent)
}
```

Die Engine prüft den Context vor und während Operationen, soweit das Transport-Interface dies erlaubt. Ein Abbruch wiederholt oder rollt bereits abgeschlossene Transfers nicht zurück.

### Nutzen

- eine spätere Progress-Ansicht erhält strukturierte Events
- Bulk-Sync und Single-File-Sync nutzen dieselbe Engine
- verspätete Ergebnisse lassen sich wie heute über Request- und Verbindungsidentitäten ablehnen

---

## `internal/remote`

`internal/remote` ist bereits die richtige Boundary. Das bestehende Interface bildet außerdem eine wichtige Sicherheitsgrenze.

### Verantwortung

- transportagnostische Dateioperationen
- Verbindungsaufbau über eine zentrale Factory
- Überwachung des Verbindungszustands
- Übergabe von FTPS-Vertrauensentscheidungen an die Protokollimplementierung

### Bestehende Boundary

Remote-Clients erhalten keine lokalen Pfade. Uploads nehmen einen `io.Reader` entgegen, Downloads liefern einen `io.ReadCloser`. Nur `fs.Root` greift auf lokale Dateien zu.

```go
type Client interface {
    Stat(path string) (os.FileInfo, error)
    ReadDir(path string) ([]*fs.FileEntry, error)
    Open(path string) (io.ReadCloser, error)
    ReadFile(path string) ([]byte, error)
    Upload(remotePath string, src io.Reader) error
    WalkFiles(root string, fn func(string) error) error
    WalkFilesWithActivity(root string, fn func(string) error, activity func() error) error
    DeleteFile(path string) error
    Done() <-chan struct{}
    Err() error
    Close() error
}
```

`Done()` und `Err()` gehören zum Vertrag. Die Root-App beobachtet Verbindungsabbrüche unabhängig vom aktiven Screen und verwirft Meldungen veralteter Verbindungen oder Projekte.

Verbindungen werden ausschließlich so aufgebaut:

```go
remote.Connect(ctx, host, trustManager, requiredChallenge)
```

`requiredChallenge` darf nur beim ersten Retry nach einer FTPS-Zertifikatsabfrage gesetzt sein.

### Driver-Registry

Der aktuelle `switch` in `remote.Connect` unterstützt drei eng verwandte Protokollwerte und ist überschaubar. Eine Registry soll erst eingeführt werden, wenn ein weiteres Protokoll sie konkret benötigt. Sie darf Trust- und Lebenszyklusregeln nicht umgehen.

### Nutzen

- lokale Pfade bleiben außerhalb der Transportimplementierungen
- alle Protokolle haben dieselben Regeln für Schließen und Verbindungsverlust
- neue Protokolle können später ergänzt werden, ohne die Application Services zu ändern

---

## `internal/fs`

### Verantwortung

- lokales Lesen, Öffnen, Walken, Löschen und Schreiben
- Begrenzung transferierter und veränderter Pfade auf das geöffnete Projekt
- Schutz vor Pfadausbrüchen durch Symlinks
- atomisches Ersetzen heruntergeladener Dateien

### Bestehende Boundary

`fs.Root` ist die verbindliche Grenze für lokale Dateiinhalte und Änderungen. Application Services und die Sync-Engine reichen den geöffneten Root weiter, statt Transferpfade direkt mit `os.*` zu bearbeiten.

```go
type Root struct { /* projektgebundener os.Root */ }

func OpenRoot(projectRoot string) (*Root, error)
func (r *Root) Open(absPath string) (*os.File, error)
func (r *Root) Stat(absPath string) (os.FileInfo, error)
func (r *Root) ReadFile(absPath string) ([]byte, error)
func (r *Root) Remove(absPath string) error
func (r *Root) WriteAtomic(absPath string, src io.ReadCloser) error
func (r *Root) Close() error
```

Verzeichnisansicht und rekursiver Scan liegen derzeit noch in den Paketfunktionen `fs.ReadDir` und `fs.WalkFiles`. Der SessionService darf Pfade daraus erst nach erfolgreichem Mapping verwenden. Wenn er Scans selbst übernimmt, sollte `fs.Root` um eine sichere Walk-Methode erweitert werden, statt ungebundene `os.*`-Zugriffe in `internal/app` einzuführen.

Ein zusätzliches `LocalFS`-Interface ist derzeit nicht nötig. Es soll nur entstehen, wenn mindestens eine zweite konkrete Implementierung gebraucht wird.

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

Die aktuelle Struktur ist für den Stand des Projekts passend.

### Aktuelle Stärken

- globale Hosts in `config.toml`
- projektbezogene Hosts und Mappings in `projects/<slug>.toml`
- Projektzuordnung über die Registry `projects.toml`
- Host-Override per Name
- Host-Mappings mit Vorrang vor Projekt-Mappings
- Auth-Werte mit Expansion von Umgebungsvariablen beim Verbindungsaufbau
- Validation für Mappings und Keep-alive-Intervalle beim Laden und Schreiben
- atomisches Schreiben der TOML-Dateien mit restriktiven Rechten

Im Arbeitsverzeichnis wird keine drift-Datei angelegt. Application Services erhalten `ProjectRoot` und `ProjectSlug` aus der bereits aufgelösten `MergedConfig`; sie suchen nicht selbst in der Registry.

### Mittelfristige Verbesserungen

Noch sinnvoll sind protokollspezifische Prüfungen, etwa fehlende Credentials, ungültige `RootPath`-Werte oder inkompatible Feldkombinationen.

Generische `Options map[string]string` sollen nicht vorsorglich eingeführt werden. Wenn ein neues Protokoll zusätzliche Werte benötigt, werden sie anhand des konkreten Falls typisiert modelliert.

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

- `dashboard` und `projectform`
- `projectselector`
- `browser`
- `hostselector`
- `hostmanager` und `hostform`
- `certtrust`
- `diffview`
- `loading`
- später bei Bedarf ein eigener `syncprogress`-Screen

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
  -> fs.OpenRoot(projectRoot)
  -> remote.Connect(ctx, host, trustManager, requiredChallenge)
  -> pathmap.Mapper
  -> lokaler und Remote-Walk
  -> diff.Compare(...) pro Session
  -> Sessions, Client und Root zurück

App
  -> Verbindungsbeobachter registrieren
  -> diffview.New(sessions, host, conn, root)
```

### Wichtig

Die Session-Erzeugung gehört in die `app`-Schicht, nicht in `diffview`. Der Service muss lokale Zugriffe über `fs.Root` ausführen, Mapping-Grenzen einhalten, Verbindungs- und Trust-Fehler typisiert zurückgeben und selbst erzeugte Ressourcen bei einem Fehler schließen.

---

## 2. Diff-Ansicht -> Plan -> Sync

```text
diffview.Model
  -> User setzt gewünschte Action je Datei
  -> MsgSyncRequested(plan-input)

App / SyncService
  -> sync.BuildPlan(sessions, selected actions)
  -> sync.Run(ctx, client, root, plan, progressSink)
  -> Progress-Events / Ergebnis

App
  -> Verbindungsidentität prüfen
  -> diffview oder syncprogress aktualisieren
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

- `fs.Root` öffnen und seine Übergabe oder Schließung eindeutig regeln
- Verbindungen ausschließlich über `remote.Connect` aufbauen
- Trust-Manager und einmalige FTPS-Retry-Challenge durchreichen
- markierte Pfade deterministisch sortieren
- lokale Dateien expandieren
- Remote-only-Dateien einsammeln
- Mapping-Grenzen erzwingen
- Sessions erzeugen
- Refresh erneut ausführen
- Fehler mit fachlichem Kontext protokollieren und typisiert zurückgeben

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

- Ausführung eines Plans über `remote.Client` und `fs.Root`
- Upload über `root.Open` und `client.Upload`
- Download über `client.Open` und `root.WriteAtomic`
- lokale und entfernte Deletes über die jeweilige Boundary
- strukturierte Progress-Events
- Fehleraggregation ohne Verschlucken einzelner Fehler
- Prüfung von Context und Verbindungszustand

Die erste Version bleibt seriell. Parallelisierung soll erst nach einem konkreten Bedarf und einer Prüfung der Protokollgrenzen erfolgen. Ein Transfer wird nach Verbindungsverlust nicht automatisch wiederholt.

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

## Testgrenzen statt Test-Doubles

Das Projekt verwendet keine Mocks. Neue Interfaces werden daher nicht allein für Tests eingeführt.

- lokale Tests verwenden einen echten temporären Projektbaum und `fs.Root`
- Protokolltests verwenden echte Verbindungen oder werden übersprungen, wenn die Umgebung fehlt
- reine Auswahl-, Mapping-, Plan- und Policy-Logik wird als deterministische Funktion ohne I/O getestet
- Application Services werden so zerlegt, dass I/O-Aufbau und reine Session-Erzeugung getrennt prüfbar sind
- `remote.Client` bleibt das gemeinsame produktive Protokoll-Interface; ein zusätzliches Factory- oder LocalFS-Interface ist erst bei einer zweiten produktiven Implementierung gerechtfertigt

Der `SessionService` darf zunächst ein konkreter Typ sein. Ein Interface entsteht erst, wenn tatsächlich mehrere Aufrufer oder Implementierungen unterschiedliche Bindungen brauchen.

---

## Erweiterbarkeit im Zielbild

## Neue Protokolle

### WebDAV

Benötigt vor allem:

- einen neuen Driver unter `internal/webdav`
- einen neuen Fall in `remote.Connect` oder, wenn dadurch ein konkreter Nutzen entsteht, eine Driver-Registry
- typisierte WebDAV-Auth- und Optionsfelder
- dieselben Trust-, Lebenszyklus- und Stream-Regeln wie bestehende Clients

Weil Orchestrierung und Sync-Engine transportagnostisch sind, bleibt der Rest weitgehend stabil.

### rsync

rsync passt nicht perfekt auf das bestehende Dateioperationen-Interface, weil es eher ein Sync-Mechanismus als ein File-API-Client ist.

Dafür gibt es zwei Wege:

1. `remote.Client` erweitern oder abstrahieren
2. rsync als alternativen `sync.Engine`-Backend betrachten

Für rsync ist Variante 2 meist architektonisch sauberer.

---

## Weitere Diff-Strategien

Bereits vorhanden sind zeilenbasierte Textdiffs, Binärerkennung, ein Größe-/mtime-Schnellpfad und ein SHA-256-Vergleich für große Dateien gleicher Größe.

Mögliche Erweiterungen:

- Ignore-whitespace diff
- wortbasierte Darstellung
- erweiterter Vergleich binärer Metadaten

Diese Strategien sollten in `internal/diff` oder als Option im `app`-Service sitzen, nicht in der TUI.

---

## Ignore-Regeln

Sinnvolle Zielarchitektur:

- lokale Standard-Ignores in `internal/fs`
- projektbezogene Ignore-Regeln aus Config
- Anwendung in Session-Building / Plan-Building, nicht erst in der View

---

## Empfohlene Migrationsreihenfolge

## Phase 1: sichere Grundlagen

1. ~~deterministische Sortierung für Hosts und markierte Pfade~~
2. ~~`pathmap` segment-sicher machen und Mapping-Abdeckung erzwingen~~
3. ~~`diff.Compare()` für lokale NotFound- und andere Fehler schärfen~~
4. ~~`autoDir()` und `nextDir()` aus `tui/diffview` nach `internal/sync` verschieben~~
5. ~~lokale Dateioperationen über `fs.Root` absichern und Downloads atomar schreiben~~
6. ~~Verbindungsüberwachung, Keep-alive und FTPS-Trust zentral anbinden~~
7. ~~Loading-Tracker mit Context-Abbruch und Schutz vor verspäteten Ergebnissen einführen~~
8. ~~Mapping- und Keep-alive-Validation außerhalb der UI einführen~~

Phase 1 ist umgesetzt. Bei FTP bleibt die Mehrdeutigkeit von Status `550` als bekannte Einschränkung bestehen.

## Phase 2: Session-Orchestrierung entkoppeln

1. `internal/app/session_service.go` als konkreten Service einführen
2. Ownership von `remote.Client` und `fs.Root` im Service-Ergebnis festlegen
3. `diffview.LoadCmd()` auf den SessionService umstellen
4. `refreshCmd()` und den Reload einer einzelnen Session auf den Service umstellen
5. Lade-, Trust- und Verbindungsfehler weiterhin als bestehende typed messages an die Root-App geben

Als Nächstes sollte `internal/app/session_service.go` entstehen, ohne gleichzeitig die Sync-Ausführung umzubauen.

## Phase 3: Sync-Domain vervollständigen

1. bestehendes `sync.Plan` mit dem Decision-Modell zusammenführen und Deletes abbilden
2. serielle `sync.Engine` für Upload, Download und Delete über `remote.Client` und `fs.Root` implementieren
3. Fehleraggregation und strukturierte Progress-Events ergänzen
4. Single-File- und Bulk-Sync aus `diffview` durch SyncService und Engine ersetzen

## Phase 4: Progress und Cancellation aus der TUI lösen

1. vorhandenen `loading.Tracker` an Engine-Events anbinden oder einen kleinen Adapter ergänzen
2. Context-Abbruch bis an SessionService und Engine durchreichen
3. einen eigenen `syncprogress`-Screen nur einführen, wenn die bestehende Overlay-Darstellung nicht ausreicht

## Phase 5: Diff- und Config-Modell schärfen

1. Presence- und Difference-Zustände explizit modellieren
2. protokollspezifische Fehlerklassifikation verbessern, insbesondere FTP `550`
3. fehlende protokollspezifische Config-Prüfungen ergänzen
4. neue Protokolloptionen erst mit dem jeweiligen Driver typisiert hinzufügen
5. eine Driver-Registry nur einführen, wenn der zentrale `switch` tatsächlich zum Wartungsproblem wird

---

## Zielbild in einem Satz

`drift` sollte architektonisch auf ein Modell zulaufen, in dem **BubbleTea nur Interaktion und Darstellung übernimmt**, während **Session-Aufbau, Diff-Entscheidung, Sync-Planung und Sync-Ausführung in klar testbaren, UI-unabhängigen Paketen** liegen.
