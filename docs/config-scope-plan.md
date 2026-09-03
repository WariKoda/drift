# Plan: Config-Schichten nach Zuständigkeit trennen

## Ziel

`.drift/config.toml` enthält heute zwei Arten von Daten, die verschiedenen Leuten
gehören:

| Gehört dem Projekt | Gehört dem Entwickler |
| --- | --- |
| `mappings` (spiegeln den Verzeichnisbaum) | `user`, `defaults.user` |
| `root_path`, `protocol`, `port` | `auth.type`, `auth.key_file` |
| `hostname` gemeinsamer Umgebungen | `insecure_tls` |
| | eigene Dev-Hosts / VMs |

Die linke Spalte ist eine Aussage über das Repository und gehört versioniert ins
Repository: verschiebt jemand `plugins/plugin1`, ist die Mapping-Änderung Teil
desselben Commits. Die rechte Spalte ist pro Person und pro Maschine verschieden.
Beides in einer geteilten Datei bedeutet Merge-Konflikte auf Zeilen, die niemand
committen wollte — und bei `insecure_tls` erbt das ganze Team ein
TLS-ohne-Prüfung, das jemand für seine Dev-Box gesetzt hat.

Nach der Trennung enthält die Projekt-Datei nichts mehr, was man verstecken
müsste. Damit entfällt der Grund für den Apparat aus 0.1.5-alpha
(`.drift/.gitignore` schreiben, Verzeichnisrechte, git-Erreichbarkeit prüfen).

## Zielbild

```
<projekt>/.drift.toml            im Repo, committbar, klein
  [[hosts]]  name, hostname, port, root_path, protocol, mappings
  [[mappings]]                   Projekt-Fallback

~/.config/drift/config.toml      globale Hosts (unverändert)
~/.config/drift/projects.toml    Registry (unverändert)
~/.config/drift/access.toml      pro Projekt + Host: user, auth, insecure_tls,
                                 password, passphrase — ersetzt secrets.toml
```

Regeln:

- Die Projekt-Datei definiert **Umgebungen**, nicht Zugänge.
- `access.toml` definiert **Zugänge**, keine Hosts. Ein Eintrag ohne passenden
  Host in der Projekt-Datei hätte weder `hostname` noch `port` noch `root_path`
  und wäre unbenutzbar. Der Fall, den das abdecken sollte — private VM, oder ein
  öffentliches Repo, in dem auch der Hostname nichts zu suchen hat — ist bereits
  durch einen globalen Host in `~/.config/drift/config.toml` abgedeckt.
- Gleicher Name in beiden Schichten: die Nutzer-Schicht gewinnt, sie ist die
  spezifischere.
- `$ENV_VAR`-Referenzen und `key_file`-Pfade sind keine Secrets, gehören aber
  zur Zugangsschicht: der Pfad meines Schlüssels ist nicht der Pfad deines
  Schlüssels. Beim **Schreiben** wandern sie deshalb mit. Beim **Migrieren**
  nicht, siehe unten — und was in der Projekt-Datei steht, gewinnt beim Lesen,
  damit ein Team eine `$DEPLOY_PASSWORD`-Konvention weiter teilen kann.
- Globale Hosts in `~/.config/drift/config.toml` bleiben wie sie sind. Sie sind
  Nutzer-Schicht ohne Projektbezug und brauchen keine Migration.

## Zwei Vorentscheidungen

### 1. `config.Host` bleibt der Laufzeit-Typ

Die Trennung betrifft **Speicherung und Bearbeitung**, nicht die Laufzeit.
`remote.Connect(ctx, host)`, `internal/sftp`, `internal/ftp` und `internal/sync`
sehen weiterhin einen vollständigen `config.Host` und ändern sich nicht.

Der Mechanismus dafür existiert bereits: `splitSecret` / `applySecret` in
`internal/config/secrets.go` schneiden heute schon Felder aus einem `Host` heraus
und setzen sie beim Laden wieder ein. Phase 1 verbreitert `applySecret` zu
`applyAccess` und stellt `splitSecret` ein zweites, breiteres `splitAccess` zur
Seite: `splitAccess` ist der Schreibpfad und schneidet alle Zugangsfelder heraus,
`splitSecret` bleibt der Migrationspfad und bewegt nur Lecks. Zwei neue Typen (`Environment`, `HostAccess`) wären die Alternative;
sie ziehen Umbenennungen durch `remote`, `sftp`, `ftp` und `diffview` nach sich,
ohne dass irgendwo ein Verhalten davon profitiert.

### 2. Der Schlüssel der Nutzer-Schicht bleibt der Projekt-Pfad

`secrets.toml` keyed heute auf absoluten Projekt-Pfad plus Host-Name. Das bleibt
so, und zwar bewusst mit dem bekannten Preis: verschiebt man ein Projekt,
verwaisen die Einträge und müssen neu eingegeben werden.

Der Slug aus der Registry wäre stabiler gegen Verschieben, würde aber eine
Registrierung erzwingen, die drift bewusst nicht verlangt — ein bloßes
`.drift.toml` im Verzeichnis muss ohne `projects.toml` funktionieren.

Kompromiss für später, nicht Teil dieses Plans: beim Schreiben zusätzlich den
Slug ablegen, beim Lesen erst über den Pfad und dann über den Slug auflösen.
Damit ließe sich ein verschobenes, registriertes Projekt wiederfinden, ohne den
Pfad als primären Schlüssel aufzugeben.

---

## Migration bestehender Dateien

### Quellzustände

Vier Formen existieren draußen, und die Migration muss alle vier treffen:

| # | Zustand | Enthält |
| --- | --- | --- |
| a | vor 0.1.6-alpha | `.drift/config.toml` mit Passwort/Passphrase im Klartext |
| b | 0.1.6-alpha | `.drift/config.toml` ohne Credentials + `secrets.toml` |
| c | handgeschrieben | beliebige Mischung, evtl. `$ENV`-Referenzen, evtl. ohne `[defaults]` |
| d | nach Phase 1 | `.drift/config.toml` nur mit Projekt-Feldern + `access.toml` |

**Die Migration bewegt nur Lecks.** Ein literales Passwort oder eine literale
Passphrase muss raus. `user`, `auth.type`, `key_file` und `$ENV`-Referenzen
bleiben liegen, wo sie sind — eine Projekt-Config kann eine Datei sein, die ein
Team pflegt, und Zeilen daraus stillschweigend zu löschen wäre schlimmer als sie
stehen zu lassen: der Nächste, der pullt, verliert einen Wert, den drift für ihn
nie gespeichert hat. Diese Felder wandern von selbst raus, sobald der Host das
nächste Mal gespeichert wird.

Preis dieser Entscheidung: der `insecure_tls`-Footgun ist für neu geschriebene
und bearbeitete Hosts behoben, für einen bereits committeten nicht rückwirkend.

Zustand (b) unterscheidet sich für die Migration nur darin, dass zusätzlich eine
`secrets.toml` einzulesen und nach dem Schreiben von `access.toml` zu löschen
ist.

### Reihenfolge der Schreibvorgänge

Bei mehreren Dateien entscheidet die Reihenfolge, was ein Abbruch kostet:

1. `access.toml` schreiben — die Nutzer-Schicht zuerst, weil ein Abbruch danach
   höchstens ein doppelt vorhandenes Credential kostet, die andere Reihenfolge
   dagegen verliert es.
2. `.drift.toml` schreiben (Phase 2) bzw. `.drift/config.toml` neu schreiben
   (Phase 1).
3. Alte Datei löschen: `secrets.toml`, und in Phase 2 `.drift/config.toml`.
4. `.drift/` entfernen — **nur** wenn danach höchstens noch drift's eigenes
   `.gitignore` darin liegt, byte-identisch mit der `projectGitignore`-Konstante.
   Ein `.drift/` mit fremdem Inhalt bleibt unangetastet.

### Atomar schreiben

`writeToml` benutzt heute `os.WriteFile`. Ein Abbruch mitten im Schreiben
hinterlässt eine abgeschnittene Datei — bei `access.toml` heißt das: die Zugänge
**aller** Projekte sind weg, nicht nur die des migrierten. Das ist heute schon so
und wird mit jedem Feld schlimmer, das in die Datei wandert.

Phase 1 schreibt deshalb über temporäre Datei plus `os.Rename` im selben
Verzeichnis — umgesetzt in `writeToml`. Das deckt zugleich den Fall ab, dass zwei drift-Instanzen in zwei
Projekten gleichzeitig migrieren: `access.toml` ist eine globale Datei, und
Read-modify-write ohne Rename verliert dabei den Eintrag des jeweils anderen.

### Kollision: neue und alte Datei gleichzeitig

`.drift.toml` liegt schon da, weil ein Teamkollege sie gepullt hat, und
`.drift/config.toml` ist noch nicht migriert. Auflösung: `.drift.toml` gewinnt
für die Projekt-Schicht, die Zugangsfelder der alten Datei gehen in
`access.toml`, die alte Datei wird gelöscht. Kein Merge der Projekt-Felder — die
gepullte Datei ist die Wahrheit des Teams.

### Einbahnstraße, und was das für Teams heißt

Die Migration ist nicht rückwärts fahrbar. Für eine getrackte Datei hat git die
alte Fassung, für eine ungetrackte ist sie weg — die Daten stecken dann in
`access.toml`, die eine ältere drift-Version nicht liest.

Daraus folgt der unangenehme Teil, und er betrifft nur **Phase 2**: der
Dateiname ist geteilter Zustand. Sobald jemand `.drift.toml` committet, findet
ein Teamkollege mit älterem drift kein Projekt mehr, weil dessen Walk-up nach
`.drift/config.toml` sucht. Das ist ein Breaking Change für gemeinsame
Repositories, kein internes Detail.

Konsequenzen, die vor Phase 2 zu entscheiden sind:

- Phase 2 als eigener Release mit dem Hinweis im CHANGELOG, dass im Team alle
  aktualisieren müssen, bevor die neue Datei committet wird.
- Der Walk-up liest während einer Übergangszeit **beide** Namen, damit die
  Migration überhaupt greifen kann. Wie lange „Übergangszeit" heißt, gehört in
  den CHANGELOG-Eintrag, nicht in ein Kommentar.
- Phase 1 ist davon frei: sie entfernt nur Felder aus einer Datei, die ältere
  Versionen weiterhin lesen. Eine ältere Version verbindet dann ohne `user` —
  auffällig, aber kein Datenverlust.

Wer diesen Preis nicht zahlen will, kann Phase 2 streichen, ohne Phase 1
anzufassen. Das Verzeichnis bleibt dann bestehen, mit einer einzigen Datei darin
und einem `.gitignore`, das nichts mehr schützt.

### Wo die Migration läuft

Unverändert im TUI: `App.Init` und `App.openProject`, als `tea.Cmd`, über einen
Projekt-Root von Platte gelesen statt über den geladenen `MergedConfig` — so
teilt der Aufruf keinen State mit der laufenden Session. Alle `config.Load`-Pfade
(`cmd/root.go`, `cmd/projects.go`) münden im TUI, es gibt keinen zweiten
Einstieg, der eine eigene Migration bräuchte.

Ein Fehlschlag darf den Start **nicht** blockieren: Nur-Lese-Dateisystem, kein
Schreibrecht auf `~/.config`, kaputte TOML-Datei. Das Verhalten von heute
(Fehler als `globalError` in der Statuszeile, `log.Error`, Session läuft weiter
mit den Werten aus dem Speicher) bleibt.

### Tests

- Migration aus jedem der vier Quellzustände (a)–(d), inklusive: (d) schreibt
  nichts
- ein Lauf, nicht zwei: (a) landet in einem Durchgang vollständig in `access.toml`
- Idempotenz: zweiter Lauf lässt beide Dateien unangetastet
- `$ENV`-Referenz bleibt in der Projekt-Datei, erzeugt keinen Access-Eintrag
- Kollisionsfall neue + alte Datei
- `.drift/` mit nur drift's `.gitignore` wird entfernt, mit fremder Datei nicht
- abgebrochener Schreibvorgang: nach `os.Rename`-Umstellung ist entweder die alte
  oder die neue Datei vollständig da
- zwei Projekte hintereinander migrieren, ohne sich die Einträge zu überschreiben
- Migration bei nicht schreibbarem Config-Verzeichnis: Fehler sichtbar, Start
  läuft weiter

---

## Phasen

| Phase | Inhalt | Eigenständig auslieferbar |
| --- | --- | --- |
| 1 | Zugangsfelder in die Nutzer-Schicht, `secrets.toml` → `access.toml` | ja — umgesetzt |
| 2 | `.drift/config.toml` → `.drift.toml`, `.drift/` abbauen | ja, aber Breaking Change fürs Team |
| 3 | UI zeigt, in welche Schicht ein Feld geht | ja |
| 4 | Projekte ohne Projekt-Datei (optional) | ja |

Phase 1 trägt die Substanz. Phase 2 ist Kosmetik mit Aufräum-Effekt und dem
einzigen Kompatibilitätsbruch des Plans (siehe
[Einbahnstraße](#einbahnstraße-und-was-das-für-teams-heißt)) — sie ist
streichbar, ohne Phase 1 anzufassen. Phase 3 ist
der Teil, ohne den Nutzer die Trennung nicht verstehen. Phase 4 ist offen, ob
gewünscht.

---

## Phase 1 — Zugangsfelder in die Nutzer-Schicht

> Umgesetzt. Zwei Abweichungen gegenüber der ersten Fassung dieses Plans sind
> oben eingearbeitet: die Migration bewegt nur Lecks, und `access.toml`
> definiert keine eigenen Hosts. Dazu kamen zwei Defekte, die beim Umsetzen
> auffielen und in derselben Änderung behoben sind: Schreiben ging vom
> zusammengeführten Speicherabbild aus statt von der Datei (und buk damit
> `[defaults]` und gespeicherte Zugänge in fremde Host-Records), und `writeToml`
> schrieb nicht atomar.

### Format

`~/.config/drift/access.toml` (Mode `600`, Verzeichnis `700`):

```toml
[[access]]
  project = "/home/you/work/myshop"
  host = "staging"
  user = "webuser"
  insecure_tls = false
  [access.auth]
    type = "keyfile"
    key_file = "~/.ssh/id_ed25519"
    passphrase = "…"
```

`.drift.toml` bzw. noch `.drift/config.toml` behält:

```toml
[[hosts]]
  name = "staging"
  hostname = "shopdev.example.com"
  port = 21
  root_path = "/var/www"
  protocol = "ftp"

  [[hosts.mappings]]
    local = "plugins/plugin1"
    remote = "html/custom/plugins/plugin1"
```

Das Tabellen-Element heißt weiter `[[hosts]]`. Ein Rename auf `[[environments]]`
bricht jede bestehende Datei für nichts als Wortwahl.

### Schritte

1. `internal/config/secrets.go` → `internal/config/access.go`. `hostSecret` wird
   `hostAccess` und bekommt `User`, `InsecureTLS`, `Auth`. Das TOML-Array heißt
   `access`.
2. `splitSecret` → `splitHost(h Host, projectRoot string) (Host, hostAccess)`:
   schneidet `User`, `Auth` komplett und `InsecureTLS` heraus.
   `applySecret` → `applyAccess`, füllt sie zurück; ein Wert, der in der
   Projekt-Datei steht, gewinnt weiterhin, damit handgeschriebene Dateien
   funktionieren.
3. `applyProjectSecrets` → `applyProjectAccess`.
4. `writer.go`: `strippedHosts` nutzt `splitHost`. `SaveProjectHost` und
   `DeleteProjectHost` bleiben strukturell, schreiben nur mehr Felder in die
   Nutzer-Schicht.
5. `loader.go`: `literalSecretHosts` → `hostsWithAccessInFile` — Hosts, deren
   `user`/`auth`/`insecure_tls` noch aus der Projekt-Datei kamen. Feld auf
   `MergedConfig` entsprechend umbenennen (`ProjectSecretsInFile` →
   `ProjectAccessInFile`).
6. Migration: `MigrateProjectSecrets` → `MigrateProjectAccess(projectRoot)`.
   Details in [Migration bestehender Dateien](#migration-bestehender-dateien).
7. `internal/tui/app.go`: Nachricht und Notice-Text anpassen. Die
   git-Erreichbarkeitsprüfung bleibt genau hier sinnvoll: eine getrackte alte
   Config hat das Passwort in der History.

### Tests

- `splitHost` / `applyAccess` inkl. Vorrang der Projekt-Datei
- Host, der nur in `access.toml` existiert, erscheint als Projekt-Host
- Migration einer 0.1.6-`secrets.toml` plus Config mit `user`/`auth`
- Idempotenz, zweiter Lauf schreibt nichts
- Rename und Delete räumen den Access-Eintrag auf (analog zu heute)
- zwei Projekte kollidieren nicht

---

## Phase 2 — `.drift.toml` statt `.drift/config.toml`

`.drift/` enthält genau eine Datei, die drift schreibt, plus das `.gitignore`,
das nur existiert, um die erste zu schützen. Nach Phase 1 ist das zirkulär: es
gibt nichts mehr zu schützen. Ein Verzeichnis lädt außerdem dazu ein, dass später
Cache und Laufzeit-Zustand darin landen, die dann wieder ignoriert werden müssen.

### Schritte

1. Dateiname als Konstante an **einer** Stelle (`internal/config/loader.go`).
   Heute liegt `.drift/config.toml` als Literal in `loader.go` (dreimal),
   `writer.go` und `gitguard.go`.
2. `loadProject`, `decodeProjectConfig`, `HasProjectContext`: Walk-up sucht
   `.drift.toml`. Der Marker bleibt das Rooting-Verfahren, nur der Name ändert
   sich — `Load(startDir)` verhält sich sonst identisch.
3. `writeProject` schreibt `<root>/.drift.toml` mit Mode `600`.
   `ensureProjectGitignore` und die `0700`-Verzeichnisrechte fallen weg.
4. Migration inklusive Aufräumen von `.drift/`: siehe
   [Migration bestehender Dateien](#migration-bestehender-dateien).
5. `gitguard.go`: `projectConfigRel` zeigt auf die **alte** Datei, weil nur die
   je Credentials enthielt. Die Datei ist damit reines Migrations-Zubehör und kann
   verschwinden, sobald die Migration ausläuft — als Kommentar festhalten.

### Tests

- Walk-up findet `.drift.toml` aus einem Unterverzeichnis
- `HasProjectContext` für beide Dateinamen während der Übergangszeit
- Migrationsfälle: siehe [Migration bestehender Dateien](#migration-bestehender-dateien)

---

## Phase 3 — UI zeigt die Schichten

Ohne diesen Teil ist die Trennung unsichtbar und wirkt wie Datenverlust („mein
`user` ist aus der Config verschwunden").

- `internal/tui/hostform`: die Feldliste bleibt, bekommt aber zwei Gruppen mit
  Überschrift — *Shared with the team* (hostname, port, root path, protocol,
  mappings) und *Only on this machine* (user, auth, insecure TLS). Der
  Scope-Toggle Global/Projekt bleibt daneben bestehen, er beantwortet eine andere
  Frage.
- `internal/tui/hostmanager`: Sektionen `Project (shared)` / `This machine` /
  `Global`. `HostScope` braucht dafür einen dritten Wert oder eine zweite
  Dimension — beim Umsetzen entscheiden, was weniger Verzweigungen erzeugt.
- Herkunft sichtbar machen: eine Legendenzeile genügt, kein Feld-für-Feld-Badge.

### Tests

Bestehende `hostform`/`hostmanager`-Tests auf die neue Zeilen-/Sektionsstruktur
ziehen (`visibleRows()`, Sektions-Header-Zählung — die Zeilenbudget-Falle aus
0.1.4-alpha nicht wieder aufreißen).

---

## Phase 4 — Projekte ohne Projekt-Datei (optional)

drift legt ein Verzeichnis in ein fremdes Repo, und das ist die Entscheidung des
Repo-Eigners, nicht die des Tool-Nutzers. Ein Projekt sollte vollständig
funktionieren, ohne dass eine Datei im Baum landet: Hosts und Mappings können
komplett in der Nutzer-Schicht liegen.

Offene Frage dabei ist nur das Rooting. Ohne Marker muss die Registry es leisten:
längster Pfad-Präfix-Treffer über die registrierten Projekte in `projects.toml`.
Konsequenz: ohne Registrierung findet drift ein solches Projekt nicht — was
konsistent ist, weil es dann auch keine Einträge in der Nutzer-Schicht hätte.

Umsetzung nur, wenn jemand es tatsächlich verlangt. Der Rest des Plans ist davon
unabhängig.

---

## Nicht Teil dieses Plans

- **Keyring / verschlüsselte Ablage.** Auf Servern und in SSH-Sessions gibt es
  keinen Secret Service, der Datei-Fallback bleibt also in jedem Fall nötig.
  Erst den Fallback richtig haben, Keyring höchstens später darauf.
- **`Host` in zwei Typen aufspalten.** Siehe Vorentscheidung 1.
- **Slug als Schlüssel.** Siehe Vorentscheidung 2.
- **`.drift/` als Cache-Verzeichnis.** Laufzeit-Zustand gehört nach
  `config.Dir()`, nicht ins Projekt.
- **Öffentliche Repos absichern.** Dass Hostnames und Usernames von
  Produktionsumgebungen Aufklärungsmaterial sind, gehört als Hinweis in den
  README-Abschnitt zur Projekt-Datei, nicht in Code. Wer das nicht teilen will,
  definiert seine Hosts in der Nutzer-Schicht — das kann er nach Phase 1.

## Begleitend pro Phase

- `CHANGELOG.md` unter `[Unreleased]`
- `README.md`: Abschnitt „Where credentials live" wird zu „Which file holds what",
  Konfigurationsbeispiele, Config-Locations
- `AGENTS.md`: Tabelle „Config locations" und der Abschnitt „Credentials"
- `go build ./... && go vet ./... && go test ./...` vor jedem PR
