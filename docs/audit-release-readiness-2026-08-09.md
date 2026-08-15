# GitHub-Release-Readiness-Audit

**Stand:** 9. August 2026  
**Lokaler HEAD:** `82b48e3`  
**Bewerteter Umfang:** gesamte Codebasis einschließlich Build, Tests, CI, Architektur, Dokumentation, Sicherheit, Abhängigkeiten, Lizenzierung, Repository-Metadaten und Release-Prozess

## Gesamtbewertung

**Der aktuelle Stand ist nicht bereit für eine weitere öffentliche GitHub-Veröffentlichung.**

Build, Tests und grundlegende Architektur sind solide, und mehrere frühere kritische Sync-Probleme wurden überzeugend verbessert. Es bestehen jedoch weiterhin release-blockierende Korrektheits- und Sicherheitsprobleme: falsche Dateiauswahl bei aktivem Filter, ein unsicherer Diff-Fastpath, Symlink-Ausbrüche, erreichbare SSH-Schwachstellen und Terminal-Escape-Injection.

Für eine **Source-only Alpha** wäre das Projekt nach Behebung dieser Blocker grundsätzlich geeignet. Für einen stabilen oder binären Multiplattform-Release fehlen zusätzlich Release-Automation und breitere Integrationstests.

Die Analyse erfolgte ohne Änderungen an der Codebasis. Prüfungen erzeugten nur übliche Go-Caches und temporäre Dateien außerhalb des Repository-Arbeitsbaums.

---

## Readiness-Score: 58/100

| Bereich | Gewicht | Punkte | Begründung |
|---|---:|---:|---|
| Build, Tests, CI | 20 | 17 | Alle lokalen Prüfungen und GitHub-CI grün; Race-Test erfolgreich. Kritische Protokollpfade bleiben schwach getestet. |
| Codequalität, Architektur | 20 | 10 | Sinnvolle Paketgrenzen und typed Messages, aber Sync-Orchestrierung bleibt in sehr großen TUI-Dateien; mehrere offene Korrektheitsfehler. |
| Dokumentation, Onboarding | 15 | 11 | README ist ausführlich und ehrlich als Alpha gekennzeichnet; öffentliche Planungsdokumente sind teilweise widersprüchlich oder veraltet. |
| Sicherheit, Datenschutz | 20 | 7 | Gute Secret-Hinweise und opt-in Logging, aber relevante Dependency-, Symlink-, TOFU- und Terminal-Injection-Risiken. |
| Abhängigkeiten, Lizenz, Metadaten | 10 | 5 | MIT korrekt erkannt; `govulncheck` meldet sieben erreichbare Schwachstellen. Repository-Sicherheitsoptionen sind unvollständig. |
| Release, Installation, Nutzung | 10 | 5 | Source-Installation plausibel und CLI funktionsfähig; kein Release-Workflow, keine Assets oder Checksummen, uneindeutiger Release-Commit. |
| Vollständigkeit, Hygiene | 5 | 3 | Keine TODO- oder Debug-Reste, aber tote Typen, nicht implementierte Help-Funktion und leeres Unreleased-Changelog. |
| **Gesamt** | **100** | **58** | P1-Blocker verhindern trotz grüner Builds ein Release-Go. |

---

## Durchgeführte Verifikation

### Erfolgreich

- `go test -count=1 ./...`
- `go test -count=1 -race ./...`
- `go vet ./...`
- `go build ./...`
- `go test -count=1 -cover ./...`
- Tests mit Mindestversion `GOTOOLCHAIN=go1.25.0`
- Cross-Build für Linux ARM64 sowie Darwin AMD64/ARM64
- `go mod verify`
- `go mod tidy -diff`
- `gofmt`-Prüfung aller Go-Dateien
- `git diff --check`
- `go run . version`
- `go run . --help`
- OSV-Modulabfrage
- `govulncheck ./...` über `go run`
- GitHub-API-Prüfung von CI, Releases und Repository-Metadaten

GitHub CI war erfolgreich für:

- `origin/main` auf `64dd8bc`
- Feature-HEAD `82b48e3`
- zusätzlichen CodeQL-Lauf auf `main`

Die CI-Konfiguration selbst führt Vet, Test und Build aus: `.github/workflows/ci.yml:20-27`.

### Nicht vollständig ausführbar oder nicht geprüft

- Interaktive TUI-End-to-End-Nutzung
- Produktive FTP-, FTPS- und SFTP-Server verschiedener Hersteller
- Netzwerkabbrüche während realer Transfers
- macOS-Laufzeitverhalten; nur Cross-Build verifiziert
- Drittanbieter-Lizenzscanner und SBOM
- spezialisierter Secret-Scanner; durchgeführt wurde nur eine Signatur- und Historienprüfung
- `make release-build`, weil es `./drift` im Repository erzeugt
- vollständige Exploitierbarkeit aller Dependency-Warnungen; `govulncheck` bestätigt die Aufrufbarkeit, nicht zwangsläufig jeden realen Angriffspfad

---

## Kritische Release-Blocker

### P1.1 – Filter kann die falsche Datei markieren oder öffnen

Die View rendert die gefilterte Liste:

- `internal/tui/browser/view.go:23`
- `internal/tui/browser/tree.go:71-81`

Navigation, Expandieren und Auswahl verwenden dagegen weiterhin `m.entries`:

- `internal/tui/browser/model.go:161-171`
- `internal/tui/browser/update.go:196`
- `internal/tui/browser/update.go:209`
- `internal/tui/browser/update.go:249-260`

Beispiel: Zeigt der Filter nur `visible.txt`, kann Space bei Cursor 0 trotzdem die erste Datei der ungefilterten Liste markieren. Diese Datei kann anschließend hochgeladen oder nach Richtungswechsel gelöscht werden.

**Status:** statisch verifiziert; kein passender Test vorhanden.

### P1.2 – Gleiche Größe und Mtime werden ungeprüft als identisch behandelt

`diff.Compare` beendet den Vergleich ohne Inhaltsprüfung, sobald Größe und Mtime exakt übereinstimmen:

- `internal/diff/engine.go:106-109`

Damit verschwinden Dateien mit geändertem Inhalt, aber erhaltener Mtime und gleicher Größe aus der Diff-Session. Das ist insbesondere bei grober FTP-Zeitauflösung, kopierten Metadaten oder reproduzierbaren Deployments realistisch.

Die Binär- und Großdateitests umgehen diesen Pfad durch absichtlich abweichende Mtimes:

- `internal/diff/engine_test.go:143-148`
- `internal/diff/engine_test.go:190-195`

**Folge:** Der Status „P1.1 behoben“ im alten Audit ist zu umfassend.

### P1.3 – Symlinks umgehen Projekt- und Remote-Root-Grenzen

Mappings werden nur lexikalisch geprüft:

- `internal/pathmap/mapper.go:47-95`
- `internal/pathmap/mapper.go:101-143`

Symlinks können direkt markiert werden:

- `internal/tui/browser/update.go:249-260`

Danach folgen `os.Stat` und Upload der Verlinkung:

- `internal/tui/diffview/model.go:364`
- `internal/ftp/client.go:219`
- `internal/sftp/client.go:230`

Ein Projektlink auf `~/.ssh/id_ed25519` könnte dadurch unter einem lexikalisch gültigen Projektpfad hochgeladen werden. Auch Symlinks in Elternkomponenten von Downloadzielen werden nicht abgewehrt:

- `internal/ftp/client.go:264-285`
- `internal/sftp/client.go:384-405`

Direkte Symlink-Ziele werden zwar abgelehnt, aber nicht Symlink-Eltern. Analog können serverseitige Links den konfigurierten Remote-Root verlassen.

### P1.4 – Sieben erreichbare SSH-Schwachstellen

`go.mod:15` verwendet:

```text
golang.org/x/crypto v0.49.0
```

`govulncheck` meldet **sieben erreichbare Schwachstellen**, alle ab `v0.52.0` behoben. Besonders relevant:

- `GO-2026-5021`: `@revoked` in `known_hosts` wird nicht durchgesetzt
- `GO-2026-5020`: mögliche Endlosschleife bei SSH-Channel-Writes
- `GO-2026-5018`: DoS durch pathologische RSA- oder DSA-Parameter
- `GO-2026-5017`: Client-Deadlock bei unerwarteten Antworten
- `GO-2026-5013`: Panic durch arithmetischen Underflow

Konkrete Aufrufpfade:

- `internal/ssh/knownhosts.go:36-42`
- `internal/sftp/client.go:71-78`
- `internal/ssh/auth.go:51-53`

Zusätzlich führt `.github/workflows/ci.yml:15-17` über `go.mod:3` exakt Go `1.25.0` aus. Diese Patchversion besitzt zahlreiche bekannte stdlib-Warnungen. Nicht alle sind für Drift erreichbar, aber FTPS nutzt unter anderem `crypto/tls` und `crypto/x509`.

### P1.5 – Terminal-Escape-Injection

Die Binärerkennung sucht nur nach NUL-Bytes:

- `internal/diff/engine.go:266-274`

ESC-, OSC- und andere Steuerzeichen gelangen deshalb als Text in den Renderer:

- `internal/diff/engine.go:142`
- `internal/diff/render.go:140-146`

Auch Namen und Fehler werden teilweise ungefiltert dargestellt:

- `internal/tui/browser/view.go:204-224`
- `internal/tui/hostselector/model.go:150-151`
- `internal/tui/diffview/view.go:56`
- `internal/tui/diffview/view.go:181-197`

Ein präparierter Remote-Dateiname oder Dateiinhalt kann je nach Terminal beispielsweise Bildschirm, Titel oder Clipboard manipulieren.

Die Preview implementiert bereits eine geeignete Bereinigung:

- `internal/tui/browser/preview.go:326-360`

Dieser Schutz wird jedoch nicht zentral für Diff, Namen und Fehler verwendet.

### P1.6 – Release-Commit und Changelog sind nicht eindeutig

Der Arbeitsstand vor Anlage dieses Audit-Branches war:

```text
main...origin/main [ahead 2]
```

Lokales `main` enthielt zwei Commits, die auf Remote-Feature-Branches liegen, aber nicht auf `origin/main`. Das widerspricht dem vorgesehenen Release-von-`main`-Ablauf in `AGENTS.md:107-134`.

Gleichzeitig ist `[Unreleased]` leer:

- `CHANGELOG.md:5`

Seit `v0.1.3-alpha` kamen jedoch unter anderem Preview, Loading-UI und First-Difference-Navigation hinzu. Vor einem Tag muss festgelegt werden, ob `64dd8bc` oder `82b48e3` veröffentlicht werden soll, und das Changelog muss diesen Stand abbilden.

---

## Weitere Befunde

### P2 – Remote-Operationen ohne Cancellation oder Deadline

`remote.Client` besitzt keine Context-Parameter:

- `internal/remote/client.go:19-29`

Die vorhandenen 30-Sekunden-Contexts begrenzen nur den Verbindungsaufbau:

- `internal/tui/diffview/model.go:329`
- `internal/tui/browser/remote.go:57`

`LIST`, `RETR`, `STOR`, Hashing und Sync können unbegrenzt hängen. Teilweise wird `Close()` sogar synchron aus dem TUI-Updatepfad aufgerufen, beispielsweise über `internal/tui/app.go:452-453`.

### P2 – FTP-Walker kann deadlocken

- begrenzter Queue-Kanal: `internal/ftp/client.go:365`
- dieselben Worker konsumieren und befüllen ihn: `internal/ftp/client.go:410-441`

Bei einem breit verzweigten Baum können alle Worker gleichzeitig beim Senden blockieren.

### P2 – Lokaler Walker verschluckt Fehler und akzeptiert Spezialdateien

- `internal/fs/local.go:26-40`
- Finder verwirft zusätzlich den Gesamtfehler: `internal/tui/browser/finder.go:44-47`

FIFOs, Sockets oder Devices können in den Diff gelangen und I/O blockieren. `internal/fs` besitzt keine Tests.

### P2 – FTP-Stat bleibt fehlerhaft

Nach fehlgeschlagenem `SIZE` wird jedes erfolgreich listbare Ziel als Verzeichnis behandelt:

- `internal/ftp/client.go:82-94`

Jeder FTP-550-Fehler gilt außerdem als „nicht vorhanden“:

- `internal/diff/engine.go:280-289`

550 kann serverabhängig auch „permission denied“ bedeuten.

### P2 – FTP ohne Port erhält Port 22

- `internal/config/loader.go:87-94`

Damit wird der korrekte Port-21-Fallback in `internal/ftp/client.go:35-39` nie erreicht.

### P2 – Quick-Sync-Fehler können die falsche Session treffen

- `MsgSyncError` ohne Index: `internal/tui/diffview/model.go:82-83`
- Navigation während Sync möglich: `internal/tui/diffview/update.go:108-110`
- Fehler wird aktiver Session zugeordnet: `internal/tui/diffview/update.go:78-83`

### P2 – SSH-Agent-Ressourcenleaks

- verworfener Closer beim Keyfile-Fallback: `internal/ssh/auth.go:35-40`
- nicht geschlossene Closer auf SFTP-Fehlerpfaden: `internal/sftp/client.go:37-84`

### P2 – Nicht atomare Config- und Registry-Persistenz

Laufzeitmodelle werden vor erfolgreichem Schreiben verändert:

- `internal/config/writer.go:15-21`
- `internal/config/writer.go:33-42`
- `internal/tui/app.go:107-120`

Die Dateien werden direkt trunciert:

- `internal/config/writer.go:123-128`
- `internal/project/store.go:44-52`

Ein Abbruch oder voller Datenträger kann TOML-Dateien beschädigen und Laufzeit- und Dateistand auseinanderlaufen lassen.

### P2 – Zu offene Dateirechte

- Logs werden als `0644` angelegt: `internal/log/log.go:35`
- neue Downloads standardmäßig mit `0666`, typischerweise also `0644`: `internal/ftp/client.go:274-285`, `internal/sftp/client.go:394-405`
- vorhandene Config-Dateien werden durch `os.WriteFile(..., 0600)` nicht zwingend auf `0600` gehärtet

Logs enthalten vollständige lokale und entfernte Pfade sowie Hostnamen, siehe `README.md:194-210`.

### P2 – Unvollständige Hostvalidierung

Beim Laden werden nur Mappings validiert:

- `internal/config/loader.go:14-38`

Doppelte Namen werden still überschrieben; unbekannte Protokolle fallen still auf SFTP zurück:

- `internal/config/loader.go:104,123`
- `internal/remote/client.go:34-40`

### P3 – Wartbarkeit und UX

- Sync-Aufbau, Workerpool, Transfers und Refresh liegen weiterhin in `internal/tui/diffview/model.go:323-783`, entgegen der Zielarchitektur in `docs/architecture-target.md:93-139`.
- Ungenutzte Plan- und Progress-Typen: `internal/sync/plan.go:22-51`, `internal/tui/state.go:69-70`.
- `ScreenSyncProgress` existiert, besitzt aber keinen Screen: `internal/tui/state.go:20`.
- Die Help-Ansicht verspricht Visual Selection über `v`, aber es gibt keine Implementierung:
  - `internal/tui/browser/keys.go:17,62`
  - nur `V` wird in `internal/tui/browser/update.go:262` behandelt
- Der Hostmanager hat keinen Scrolloffset. Bei vielen Hosts kann der Cursor außerhalb des sichtbaren Bereichs liegen:
  - `internal/tui/hostmanager/view.go:20-37`
  - `internal/tui/hostmanager/model.go:100-139`
- Mehrere View-Dateien erzeugen entgegen `AGENTS.md:42` Inline-Styles, etwa `internal/tui/app.go:621` und `internal/tui/hostselector/model.go:116`.
- `internal/diff/engine_test.go:14-29` verwendet einen Stub-Remoteclient und widerspricht damit dem Wortlaut „No mocks in tests“ aus `AGENTS.md:37`.

---

## Testabdeckung

Aktuell gemessene Statement-Coverage:

| Paket | Coverage |
|---|---:|
| `internal/pathmap` | 97,6 % |
| `internal/project` | 80,5 % |
| `internal/log` | 78,9 % |
| `internal/config` | 72,5 % |
| `internal/tui/loading` | 71,7 % |
| `internal/diff` | 55,6 % |
| `internal/tui/diffview` | 51,7 % |
| `internal/tui/browser` | 40,4 % |
| `internal/sftp` | 35,5 % |
| `internal/tui/hostform` | 24,6 % |
| `internal/ftp` | 10,3 % |
| `internal/fs`, `internal/ssh`, `internal/remote` | 0 % |

### Positiv

- keine übersprungenen Tests
- reale In-Process-SFTP-Protokolltests
- lokale FTP-Control- und Data-Verbindungstests
- Race-Test vollständig grün
- Mapping- und Pathmap-Tests sind tiefgehend

### Offene Testlücken

- keine FTPS-Tests
- keine FTP-Upload-, Download- oder Rename-Fehlertests
- keine produktiven FTP- oder FTPS-Server
- keine SSH-Auth-, Agent- oder Known-Hosts-Tests
- keine Walker-, FIFO- oder Symlink-Eltern-Tests
- kein Test für Filter gegen Auswahl
- kein Test für gleiche Größe und Mtime bei unterschiedlichem Inhalt

---

## Sicherheit, Secrets und Datenschutz

### Positiv

- Keine High-Confidence-Secrets in aktuellen tracked Dateien gefunden.
- Keine entsprechenden Signaturen in der geprüften Git-Historie gefunden.
- `.drift/`, Logs, `.idea/` und `.claude/` sind ignoriert: `.gitignore:4-7`.
- Passwörter und Passphrases werden im Formular maskiert: `internal/tui/textfield/textfield.go:114-121`.
- README beschreibt Umgebungsvariablen, Rotation kompromittierter Credentials und `.drift/`-Ignore ausführlich: `README.md:279-295`.
- Logging ist standardmäßig deaktiviert: `README.md:194-210`.
- Geänderte bekannte SSH-Keys werden abgelehnt: `internal/ssh/knownhosts.go:44-56`.

### Risiken

- Unbekannte SSH-Keys werden ohne Benutzerbestätigung akzeptiert und automatisch gespeichert: `internal/ssh/knownhosts.go:61-74`.
- Plain FTP überträgt Passwort und Nutzdaten unverschlüsselt: `internal/ftp/client.go:42-63`; die README warnt nicht ausdrücklich davor.
- Projekt-Hosts können globale Hosts gleichen Namens überschreiben und `$ENV`-Secrets referenzieren: `internal/config/loader.go:104,123`, `internal/ssh/auth.go:27-29`.
- GitHub Secret Scanning, Push Protection und automatische Dependabot-Sicherheitsupdates waren laut GitHub-API deaktiviert.

Die Secret-Prüfung ist ohne spezialisierten Entropie-Scanner kein mathematischer Beweis.

---

## Dokumentation, Lizenz und Repository-Metadaten

### Gut

- Alpha-Status klar kommuniziert: `README.md:9-10`.
- Installation, typische Nutzung, Keybindings, Konfiguration, Mappings und Logging sind ausführlich dokumentiert.
- Screenshot vorhanden.
- Vollständige MIT-Lizenz: `LICENSE:1-20`; GitHub erkennt sie korrekt.
- Repository ist öffentlich, nicht archiviert, Issues sind aktiviert und Topics sinnvoll gesetzt.
- `go install github.com/WariKoda/drift@latest` ist mit den vorhandenen Semver-Tags grundsätzlich nutzbar.

### Lücken und Widersprüche

- `PLAN.md:23-69,447-464` nennt nicht vorhandene Dateien, den falschen Modulpfad `github.com/nibra180/drift` und Go 1.22.
- `docs/mapping-wizard-plan.md:459-481` fordert absolute Remote-Mappings, während Validator und README relative Pfade verlangen:
  - `README.md:265-269`
  - `internal/config/mappings.go:88-107`
- `CONTRIBUTING.md:39-42` spricht von „only a few unit tests“, obwohl 24 Testdateien existieren.
- Keine `SECURITY.md`, Issue- oder PR-Templates oder Code-of-Conduct.
- README erwähnt bei `~/.local/bin` nicht die notwendige PATH-Konfiguration.
- macOS wird unterstützt genannt (`README.md:5`), aber nicht in CI ausgeführt.
- GitHub-Releases mit `-alpha` sind nicht als Prerelease markiert und enthalten keine Assets.
- `v0.1.2-alpha` und `v0.1.3-alpha` sind leichte, nicht signierte Tags; die frühere Tag-Historie ist inkonsistent.
- Es gibt keinen Release-Workflow, keine Checksummen, SBOM oder Signaturen.
- Drittanbieter-Lizenzkompatibilität wurde mangels Lizenzscanner nicht vollständig verifiziert; es wurde kein konkreter Lizenzkonflikt gefunden.

---

## Bereits gut gelöste Bereiche

1. **Atomare Dateiübertragung:** Staging und Rename sowie Copy-, Close- und Sync-Fehlerprüfungen sind substanziell verbessert:
   - `internal/sftp/client.go:164-295,371-455`
   - `internal/ftp/client.go:181-335`

2. **Mapping-Validierung:** Traversal, absolute Pfade, Kollisionen und asymmetrische Überlappungen werden zentral geprüft:
   - `internal/config/mappings.go:12-115`
   - `internal/pathmap/mapper.go:20-143`

3. **FTP-Serialisierung:** Instanz-Mutex und bis `Close` gehaltene Reader-Sperre:
   - `internal/ftp/client.go:30`
   - `internal/ftp/client.go:151-162`
   - `internal/ftp/client.go:490-507`

4. **P2.1:** Die bestehende FTP-Verbindung wird für den ersten Diff-Worker wiederverwendet: `internal/tui/diffview/model.go:511-568`.

5. **Asynchrones TUI-Modell:** Quick-Reload läuft nun als `tea.Cmd`:
   - `internal/tui/diffview/update.go:61-74`
   - `internal/tui/diffview/model.go:770-783`

6. **Preview-Sicherheit:** Größenlimit, Regular-File-Prüfung, Binärerkennung und Steuerzeichenbereinigung: `internal/tui/browser/preview.go:22-30,251-360`.

7. **Fehlerdiagnostik:** Compare- und Syncfehler enthalten inzwischen überwiegend Operation und Pfade: `internal/tui/diffview/model.go:594-729`.

8. **Repository-Hygiene:** Sauberer Arbeitsbaum, keine Produktions-TODOs, FIXMEs oder HACKs, keine unbeabsichtigten Debug-Ausgaben und keine erkannten aktuellen Secrets.

---

## Priorisierte Maßnahmen bis zur Veröffentlichung

1. **Release-Ziel festlegen:** Feature-Commits sauber nach `origin/main` mergen oder lokalen `main` zurücksetzen; danach den exakten Audit- und Tag-Commit einfrieren.
2. **`x/crypto` mindestens auf `v0.52.0` aktualisieren**, Go-Mindestpatch anheben und `govulncheck ./...` verpflichtend in CI aufnehmen.
3. **Browserfilter korrigieren:** Rendering, Navigation, Expandieren und Auswahl müssen dieselbe indexierte Liste verwenden; Regressionstests ergänzen.
4. **Metadaten-Fastpath entfernen oder absichern:** gleiche Größe und Mtime darf höchstens eine Hash- oder Streamingprüfung auslösen.
5. **Symlink-Sicherheit implementieren:** Symlinks standardmäßig nicht synchronisieren; Elternkomponenten No-Follow oder Below-Root prüfen.
6. **Alle terminalgebundenen Werte zentral sanitizen:** Inhalte, Namen, Pfade, Hostwerte und Fehler.
7. **Operations-Cancellation und Deadlines einführen** und den FTP-Walker mit Coordinator statt selbstbefüllender Workerqueue umbauen.
8. **Walker und FTP-Stat korrigieren**, FTP- und FTPS-Defaultport testen und Quick-Sync-Fehler indexieren.
9. **Konfiguration atomar und mit `0600` speichern**, Mutation erst nach erfolgreichem Commit; Logs und neue Downloads restriktiver anlegen.
10. **Integrationstests ausbauen:** FTP- und FTPS-Transfers, RNTO-Verhalten, TLS-Prüfung, SSH/Auth/Known-Hosts, Abbrüche, Symlinks und Spezialdateien.
11. **CI härten:** Race, gofmt, tidy, govulncheck, Go-Minimum und aktuelle Version, macOS-Matrix, explizite Permissions und Timeouts.
12. **Release-Dokumentation bereinigen:** `[Unreleased]` füllen, widersprüchliche Plan-Dokumente aktualisieren, Alpha-Releases als Prerelease markieren.
13. **Für binäre Releases:** Linux und macOS AMD64/ARM64, Archive, SHA-256, SBOM und optional Signaturen automatisieren.
14. **GitHub-Sicherheit aktivieren:** Branch- und Tag-Schutz, Secret Scanning, Push Protection, Dependabot und `SECURITY.md`.

---

## Abweichungen gegenüber `docs/audit-main-2026-07-29.md`

Das alte Dokument war ausdrücklich auf den Sync-Kern und Commit `5ab4f1e` begrenzt (`docs/audit-main-2026-07-29.md:3-16`). Die folgenden Punkte sind daher teils Korrekturen, teils Ergänzungen.

| Auditpunkt | Aktueller Stand |
|---|---|
| P1.1 Binär- und Großdateien | **Nur teilweise behoben:** Byte- und Hashvergleich existiert, aber der gleiche-Größe/Mtime-Fastpath bleibt ein False Negative. |
| P1.2 atomare Transfers | **Code weitgehend behoben**, für FTP und FTPS fehlen reale Transfer- und RNTO-Tests. |
| P1.3 Mapping-Grenzen | **Lexikalisch behoben**, aber Symlink-Quellen und Symlink-Eltern umgehen die Grenze. |
| P1.4 FTP-Concurrency | **Behoben**, inklusive Instanzmutex und TUI-Sperren. |
| P2.1 bestehende FTP-Verbindung | **Behoben.** |
| P2.2 Timeouts und Cancellation | **Offen.** |
| P2.3 FTP-Stat | **Offen.** |
| P2.4 lokaler Walker | **Offen.** |
| P2.5 Quick-Sync-Fehlerindex | **Offen.** |
| P2.6 synchroner Reload | **Inzwischen behoben**; Checkbox `docs/audit-main-2026-07-29.md:520` ist veraltet. |
| P2.7 Agent-Leaks | **Offen.** |
| P2.8 Browser-Generation | **Teilweise mitigiert**, strukturell weiterhin offen. |
| P2.9 Diagnostik | **Deutlich verbessert**, Mapping-, Walk- und Statfehler bleiben unvollständig. |
| P2.10 FTP-Defaultport | **Offen.** |
| P2.11 FTP-Walker-Deadlock | **Offen.** |
| P3.1 überlappende Selektionen | **Offen.** |
| P3.2 Sync-Plan und tote Typen | **Offen.** |

Neue, im alten Audit nicht enthaltene wesentliche Befunde:

- Filter bearbeitet andere Dateien als dargestellt.
- Terminal-Escape-Injection.
- Sieben erreichbare `x/crypto`-Schwachstellen.
- Symlink-Eltern umgehen atomare Root-Grenzen.
- Nicht atomare Config- und Registry-Persistenz.
- Unvollständige Hostvalidierung.
- Release-, Changelog- und GitHub-Metadatenprobleme.
- Hostmanager-Scrollproblem und nicht implementierte Visual Selection.
- Deutlich verbesserte aktuelle Coverage gegenüber der alten Tabelle.

## Abschließendes Release-Go

**Nein**, bis mindestens die sechs P1-Blocker behoben und erneut verifiziert sind.
