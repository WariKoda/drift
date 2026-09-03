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
- `access.toml` definiert **Zugänge** und darf eigene Hosts enthalten, die in der
  Projekt-Datei nicht vorkommen (private VM, oder ein öffentliches Repo, in dem
  auch der Hostname nichts zu suchen hat).
- Gleicher Name in beiden Schichten: die Nutzer-Schicht gewinnt, sie ist die
  spezifischere.
- `$ENV_VAR`-Referenzen und `key_file`-Pfade sind keine Secrets, wandern aber
  mit, weil sie zur Zugangsschicht gehören: der Pfad meines Schlüssels ist nicht
  der Pfad deines Schlüssels.
- Globale Hosts in `~/.config/drift/config.toml` bleiben wie sie sind. Sie sind
  Nutzer-Schicht ohne Projektbezug und brauchen keine Migration.

## Zwei Vorentscheidungen

### 1. `config.Host` bleibt der Laufzeit-Typ

Die Trennung betrifft **Speicherung und Bearbeitung**, nicht die Laufzeit.
`remote.Connect(ctx, host)`, `internal/sftp`, `internal/ftp` und `internal/sync`
sehen weiterhin einen vollständigen `config.Host` und ändern sich nicht.

Der Mechanismus dafür existiert bereits: `splitSecret` / `applySecret` in
`internal/config/secrets.go` schneiden heute schon Felder aus einem `Host` heraus
und setzen sie beim Laden wieder ein. Phase 1 verbreitert genau diese beiden
Funktionen. Zwei neue Typen (`Environment`, `HostAccess`) wären die Alternative;
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

## Phasen

| Phase | Inhalt | Eigenständig auslieferbar |
| --- | --- | --- |
| 1 | Zugangsfelder in die Nutzer-Schicht, `secrets.toml` → `access.toml` | ja |
| 2 | `.drift/config.toml` → `.drift.toml`, `.drift/` abbauen | ja |
| 3 | UI zeigt, in welche Schicht ein Feld geht | ja |
| 4 | Projekte ohne Projekt-Datei (optional) | ja |

Phase 1 trägt die Substanz. Phase 2 ist Kosmetik mit Aufräum-Effekt. Phase 3 ist
der Teil, ohne den Nutzer die Trennung nicht verstehen. Phase 4 ist offen, ob
gewünscht.

---

## Phase 1 — Zugangsfelder in die Nutzer-Schicht

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
3. `applyProjectSecrets` → `applyProjectAccess`, zusätzlich: Einträge in
   `access.toml`, deren Host-Name in der Projekt-Datei **nicht** vorkommt, werden
   als eigene Projekt-Hosts ergänzt (der Fall „private VM").
4. `writer.go`: `strippedHosts` nutzt `splitHost`. `SaveProjectHost` und
   `DeleteProjectHost` bleiben strukturell, schreiben nur mehr Felder in die
   Nutzer-Schicht.
5. `loader.go`: `literalSecretHosts` → `hostsWithAccessInFile` — Hosts, deren
   `user`/`auth`/`insecure_tls` noch aus der Projekt-Datei kamen. Feld auf
   `MergedConfig` entsprechend umbenennen (`ProjectSecretsInFile` →
   `ProjectAccessInFile`).
6. Migration: `MigrateProjectSecrets` → `MigrateProjectAccess(projectRoot)`.
   Zusätzlich zum heutigen Verhalten: bestehende `secrets.toml` einlesen und ihre
   Einträge in `access.toml` überführen, danach `secrets.toml` löschen.
   Idempotenz und „liest von Platte, teilt keinen State mit der Session"
   beibehalten — der Aufruf läuft weiter in einem `tea.Cmd`.
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
4. Migration, im selben Durchgang wie Phase 1: existiert `.drift/config.toml`,
   wird der Projekt-Teil nach `.drift.toml` geschrieben, die alte Datei gelöscht
   und `.drift/` **nur dann** entfernt, wenn danach höchstens noch drift's eigenes
   `.gitignore` darin liegt (byte-identisch mit `projectGitignore`). Ein `.drift/`
   mit fremdem Inhalt bleibt unangetastet.
5. Sonderfall: `.drift.toml` existiert schon (vom Teamkollegen gepullt) **und**
   `.drift/config.toml` liegt noch da. Dann gewinnt `.drift.toml` für die
   Projekt-Schicht, die Zugangsfelder der alten Datei gehen in `access.toml`, die
   alte Datei wird gelöscht.
6. `gitguard.go`: `projectConfigRel` zeigt auf die **alte** Datei, weil nur die
   je Credentials enthielt. Die Datei ist damit reines Migrations-Zubehör und kann
   verschwinden, sobald die Migration ausläuft — als Kommentar festhalten.

### Tests

- Walk-up findet `.drift.toml` aus einem Unterverzeichnis
- Migration löscht `.drift/` mit nur drift's `.gitignore`
- Migration lässt `.drift/` mit fremder Datei stehen
- Kollisionsfall aus Schritt 5
- `HasProjectContext` für beide Dateinamen während der Übergangszeit

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
