# Codebase-Audit

Stand: 27. August 2026, erstellt auf `docs/release-readiness-audit-2026-08-09`  
Nachgeprüft: 2. September 2026 auf `d60e9b3`  
Fix zu Finding 1: `fix/stale-diff-results`

## Zusammenfassung

Alle 13 Findings wurden am Quellcode nachgeprüft. Eine Begründung war falsch, vier
Bewertungen waren zu hoch angesetzt:

- Finding 1 war als kritisch eingestuft, weil überlappende Host-Auswahlen möglich
  seien. Das stimmt nicht: Während einer laufenden Netzwerkoperation blockiert
  `App.Update` genau die Tasten, die eine zweite starten würden. Ein Ergebnis für
  Host A kann deshalb nicht mit Host B verwechselt werden. Der Kurzschluss zwischen
  Ergebnis und aktivem Zustand existierte trotzdem, nur über einen anderen Weg:
  über den Projektwechsel mit `[P]`. Reproduziert, behoben, siehe unten.
- Findings 3, 5, 6 und 11 beschreiben reale Fehler, deren Auslösebedingungen aber
  enger sind als der Audit angenommen hat. Teilweise greifen inzwischen
  Absicherungen an anderer Stelle.

Stand der Findings:

- 0 kritisch
- 1 behoben (1)
- 4 hoch (2, 4, 7, 8)
- 7 mittel (3, 5, 6, 9, 10, 12, 13)
- 1 niedrig (11)

Blocker vor einem Release bleiben aus meiner Sicht der FTP-Walker (4) und die
Symlink-Behandlung bei lokalen Schreib- und Löschoperationen (2). Die beiden
Sicherheitsentscheidungen 7 und 8 sind ebenfalls hoch bewertet, sind aber bewusst
so gebaut und brauchen zuerst eine Produktentscheidung, keinen Bugfix.

## Findings

### 1. Behoben: Veraltete Diff-Ergebnisse wurden nach einem Projektwechsel übernommen

Behoben am 2. September 2026 auf `fix/stale-diff-results`, noch nicht in `main`.
Der Fix steht am Ende dieses Abschnitts.

Fundstellen der Fassung vom 27. August: `internal/tui/app.go:412-439`,
`internal/tui/app.go:169-192`, `internal/tui/app.go:149-160`,
`internal/tui/browser/update.go:349-355`

Korrigiert gegenüber der Erstfassung. Der ursprüngliche Angriffsweg, zwei
Host-Auswahlen hintereinander, war versperrt. `blocksNetworkKey` verwirft `s`, `@`
und `enter`, solange `loader.Active()` gilt, und
`TestActiveNetworkOperationBlocksQuitAndSecondOperation` deckt das ab.

Nicht versperrt war `[P]`. Der Browser prüft dort nur seine eigene `remoteBusy()`,
die von einem Diff-Ladevorgang der App nichts weiß. Der Ablauf:

1. `[s]` startet einen Diff-Load gegen Host A in Projekt A.
2. `[P]` öffnet das Dashboard, die Auswahl eines Projekts B ruft `openProject` auf.
   `openProject` tauscht Config, Arbeitsverzeichnis, Browser und Selektionen aus,
   lässt `diffLoading` und `SelectedHost` aber unangetastet.
3. Das Ergebnis für Projekt A trifft ein. `MsgDiffLoaded` prüfte nur `diffLoading`
   und `SelectedHost != nil`, akzeptierte also und sprang in den Diff-View.

Der Diff-View selbst war in sich konsistent, weil Sessions, Host und Verbindung
aus demselben Ladevorgang stammen. Gefährlich wurde das erst beim Verlassen:
`MsgBackToBrowser` ruft `StartRemote(*a.state.SelectedHost)` auf, also Host A, nun
aber im Browser von Projekt B. Ein Sync von dort übersetzt Pfade von Projekt B
gegen die Mappings, die aus Projekt Bs Config kommen, in den Baum von Host A.

Fix: Jede Diff-Anfrage bekommt eine ID aus einem monotonen Zähler. `App` hält
`diffRequest` (ID der noch gewollten Anfrage, 0 heißt keine) und `diffSeq` (vergibt
die IDs) und ersetzt damit `diffLoading`. Ein Bool konnte nicht ausdrücken, welche
Anfrage noch gewollt ist, genau das war die Lücke.

- `diffview.LoadCmd` nimmt die `requestID` und gibt sie mit dem `Host` in
  `MsgDiffLoaded` und `MsgDiffError` zurück. Ergebnis, Verbindung und
  Host-Identität gehören damit nachweisbar zusammen.
- `MsgDiffLoaded` wird nur bei ID-Treffer übernommen, sonst wird die Verbindung
  geschlossen und der Fall geloggt. Der Host für `diffview.New` und
  `state.SelectedHost` kommt aus der Nachricht statt aus dem App-Zustand.
- `openProject` und `browser.MsgOpenDashboard` geben eine laufende Anfrage auf und
  beenden den Loader. Vom Dashboard führt kein Weg in denselben Browser zurück, ein
  weiterlaufender Loader würde dort nur Tasten blockieren.

Getestet in `internal/tui/app_test.go`:
`TestDiffResultOfLeftProjectIsDiscarded` ist die umgedrehte Reproduktion,
`TestSupersededDiffResultIsDiscarded` deckt zwei konkurrierende Anfragen ab,
`TestAcceptedDiffResultUsesHostFromResult` prüft die Host-Herkunft.

### 2. Hoch: Symlinks in Pfadkomponenten umgehen die Projektgrenze

Fundstellen: `internal/pathmap/mapper.go:146-156`, `internal/pathmap/mapper.go:84-97`,
`internal/tui/diffview/model.go:725-728`, `internal/sftp/client.go:389-399`,
`internal/ftp/client.go:268-278`

Bestätigt, aber der Wirkungsbereich ist kleiner als beschrieben. Für das letzte
Pfadsegment gibt es inzwischen Schutz: `DownloadFile` in beiden Clients macht ein
`os.Lstat` auf das Ziel und bricht bei allem ab, was keine reguläre Datei ist.
`internal/sftp/client_test.go` deckt das für Upload und Download ab. Auch
`fs.WalkFiles` überspringt Symlinks, ein symlinkter Ordner im Projekt wird also
nicht durchlaufen.

Ungeprüft bleiben die Verzeichniskomponenten. `hasLocalPathPrefix` vergleicht
Strings. Ist `project/output` ein Symlink auf `/etc`, dann gilt
`project/output/passwd` als projektintern, `Lstat` sieht am Ende eine reguläre
Datei, und der Download schreibt nach `/etc/passwd`. Der Weg dorthin führt über
die Remote-Auswahl: `RemoteToLocal` erzeugt aus einem entfernten Pfad ein lokales
Ziel, ohne dass dieses lokal existieren muss. `os.Remove` in `bulkSyncCmd` prüft
gar nichts, hängt aber an einer explizit durchgeschalteten Löschrichtung,
`AutoDecision` wählt sie nie. Uploads lesen über Symlinks hinweg und können damit
Inhalte von außerhalb des Projekts auf den Server tragen.

Empfehlung: Verändernde lokale Operationen relativ zum geöffneten Projektordner
mit Beneath- und No-Symlink-Auflösung ausführen. Wo das nicht geht, alle
Pfadkomponenten prüfen und Symlinks ablehnen, nicht nur das letzte Segment.

### 3. Mittel: Gleiche Größe und mtime gelten als gleicher Inhalt

Fundstelle: `internal/diff/engine.go:106-110`

Bestätigt, Bewertung von hoch auf mittel gesenkt. Der Kurzschluss verlangt
`ModLocal.Equal(ModRemote)` auf die Nanosekunde. SFTP liefert Sekunden aus einem
`uint32`, FTP `MDTM` ebenfalls Sekunden, lokale Dateien auf ext4 haben
Nanosekunden. Und drift überträgt keine mtimes, weder Upload noch Download setzt
Zeitstempel, nur der Modus wird erhalten. Der Kurzschluss greift also fast nur bei
lokalen Dateien mit Nanosekundenanteil null, etwa nach einem `tar`-Entpacken oder
einem `rsync -t`.

Damit ist der Fehler selten, aber die Optimierung auch: In den üblichen Fällen
greift sie nie und liest trotzdem beide Seiten komplett. Sie kostet Korrektheit
und bringt wenig.

Empfehlung: Kurzschluss entfernen. Wer die Ersparnis will, braucht einen
Content-Hash vom Server, kein Zeitstempelpaar.

### 4. Hoch: Der FTP-Walker kann deadlocken

Fundstellen: `internal/ftp/client.go:348-422`, `internal/ftp/client.go:424-446`

Bestätigt, unverändert. `dirs` hat 4.096 Plätze. Bis zu vier Worker lesen daraus
und schreiben in `walkDirLevel` neu gefundene Verzeichnisse synchron in denselben
Kanal (`internal/ftp/client.go:440-441`). Ist der Puffer voll und blockieren alle
Worker im Send, liest niemand mehr.

Der Folgeschaden geht über den Scan hinaus: `pending` erreicht nie null, die
Closer-Goroutine schließt `dirs` nie, `workerWG.Wait()` kehrt nie zurück, und
`WalkFiles` hält dabei `opMu`. Die Verbindung ist damit dauerhaft blockiert und der
Loader in der TUI läuft weiter.

Empfehlung: Eine Koordinator-Goroutine verwaltet die Warteschlange, Worker senden
Funde über einen separaten Kanal an sie. Kein Worker darf blockierend in eine
begrenzte Warteschlange schreiben, die nur von denselben Workern gelesen wird.

### 5. Mittel: Konfigurationsschreibvorgänge sind nicht atomar

Fundstellen: `internal/config/writer.go:14-50`, `internal/config/writer.go:123-129`,
`internal/project/store.go:44-53`

Bestätigt, Bewertung von hoch auf mittel gesenkt, und ein Teil der Empfehlung ist
schon umgesetzt: `writeToml` schreibt mit Modus `0600`, `project.Store.Save`
ebenfalls.

Offen bleiben zwei Punkte. `SaveGlobalHost` und die drei Geschwisterfunktionen
setzen `cfg.GlobalHosts` beziehungsweise `cfg.ProjectHosts` und rufen
`rebuildMerged` auf, bevor der Write läuft. Scheitert er, weicht der
Laufzeitzustand von der Datei ab, bis drift neu startet. Und `os.WriteFile` kürzt
die Zieldatei vor dem Schreiben, ein Abbruch mitten drin hinterlässt eine
unvollständige Konfiguration. Die TOML-Kodierung passiert immerhin vorher in einen
Puffer, Kodierfehler richten also keinen Schaden an.

Empfehlung: In eine Nachbardatei mit Modus `0600` schreiben, `Sync`, dann
`os.Rename`. Den Laufzeitzustand erst nach erfolgreichem Rename aktualisieren.

### 6. Mittel: FTP und FTPS erben den Default-Port 22

Fundstellen: `internal/config/loader.go:48`, `internal/config/loader.go:87-99`,
`internal/ftp/client.go:36-39`

Bestätigt, Bewertung von hoch auf mittel gesenkt. `loadGlobal` setzt
`Defaults{Port: 22}`, `applyDefaults` schreibt diesen Wert in jeden Host ohne
eigenen Port, unabhängig vom Protokoll. Der Fallback auf 21 in `ftp.Connect` ist
damit toter Code.

Über die TUI passiert das nicht: `hostform` setzt bei leerem Portfeld selbst 21 für
FTP und FTPS (`internal/tui/hostform/model.go:283-287`). Betroffen sind
handgeschriebene Configs und Hosts, die einen globalen `defaults.port` erben. Das
Symptom ist dann ein Verbindungsfehler gegen den SSH-Port, was die Ursache gut
versteckt.

Empfehlung: Standardports erst nach Auflösung des Protokolls setzen, 21 für FTP und
FTPS, 22 für SFTP. Ein globaler `defaults.port` darf nur für Hosts gelten, deren
Protokoll dazu passt.

### 7. Hoch: Unbekannte SSH-Hostkeys werden automatisch akzeptiert

Fundstellen: `internal/ssh/knownhosts.go:16-22`, `internal/ssh/knownhosts.go:43-65`,
`internal/ssh/knownhosts.go:110-120`

Bestätigt. Das ist kein Versehen, der Kommentar über `HostKeyCallback` schreibt
TOFU ausdrücklich fest, und `TestHostKeyCallbackUsesDefaultsForUnknownHost` hält
das Verhalten fest. Beim ersten Kontakt landet ein unbekannter Schlüssel ohne
Rückfrage in `known_hosts` und wird akzeptiert. Wer in diesem Moment im Netz sitzt,
bekommt Passwort oder Passphrase und kann Transfers manipulieren. Ein
Schlüsselwechsel danach wird korrekt abgelehnt.

`ssh` selbst fragt an dieser Stelle nach. Dass drift das nicht tut, ist die
eigentliche Abweichung.

Empfehlung: Bei unbekannten Schlüsseln abbrechen und den SHA-256-Fingerprint zur
Bestätigung anzeigen. Für unbeaufsichtigte Verbindungen konfigurierbare
Hostkey-Pins unterstützen.

### 8. Hoch: Projektkonfigurationen können Umgebungsgeheimnisse weiterleiten

Fundstellen: `internal/config/loader.go:120-125`, `internal/ssh/auth.go:28`,
`internal/ssh/auth.go:36`, `internal/ssh/auth.go:48`, `internal/ftp/client.go:62`

Bestätigt. Beide Bausteine sind da. `merge` schreibt Projekt-Hosts über
`hosts[h.Name] = h` in dieselbe Map wie globale Hosts, ein Projekt kann also den
Host `prod` neu definieren. Und `os.ExpandEnv` läuft auf `Password`, `Passphrase`
und `KeyFile`, ohne Einschränkung auf bestimmte Variablen.

Das Angriffsszenario braucht ein fremdes Repository mit eigener
`.drift/config.toml` und einen Nutzer, der drift darin startet und den Host
auswählt. Dann geht `$AWS_SECRET_ACCESS_KEY` als Passwort an einen Server, den das
Projekt bestimmt. Die Mapping-Validierung ist an dieser Stelle übrigens sauber,
`normalizeLocalMappingPath` lehnt absolute Pfade und `..` ab.

Empfehlung: Keine Secret-Expansion für Zugangsdaten aus Projekt-Configs. Ein
Projekt-Host darf einen globalen Host gleichen Namens nicht unbemerkt verdrängen.
Vor dem ersten Verbindungsaufbau Quelle, Ziel und referenzierte Variable bestätigen
lassen.

### 9. Mittel: Lokales Walking verschluckt Fehler und akzeptiert Spezialdateien

Fundstelle: `internal/fs/local.go:26-42`

Bestätigt. `WalkFiles` gibt bei einem Fehler von `filepath.WalkDir` `nil` zurück,
ein unlesbarer Unterbaum fehlt danach ohne Hinweis im Ergebnis und damit im
Sync-Plan. Übersprungen werden nur Symlinks und die Verzeichnisse aus `skipDirs`.
FIFOs, Sockets und Geräte gehen an den Callback. Bei einer FIFO blockiert schon das
`os.ReadFile` in `diff.Compare` unbegrenzt, und weil der Diff-Load in einer
`tea.Cmd`-Goroutine läuft, bleibt der Loader stehen.

Gleiches Muster auf der SFTP-Seite: `internal/sftp/client.go:352-369` reicht jeden
Nicht-Verzeichnis-Eintrag weiter, also auch entfernte Symlinks. Der FTP-Walker ist
hier strenger, er behandelt nur `EntryTypeFile`.

Empfehlung: Traversierungsfehler mit Pfadangabe weitergeben und nur reguläre
Dateien ausgeben, lokal und über SFTP.

### 10. Mittel: SFTP-Handshakes haben keine Frist, Auth-Closer gehen verloren

Fundstellen: `internal/sftp/client.go:37-86`, `internal/ssh/auth.go:35-41`,
`internal/ssh/auth.go:62-74`

Bestätigt. `ctx` begrenzt nur `dialer.DialContext`. Danach laufen
`gossh.NewClientConn` und `pkgsftp.NewClient` ohne Deadline auf der Verbindung.
`ClientConfig.Timeout` hilft nicht, den nutzt nur `ssh.Dial` für den TCP-Aufbau.
Ein Server, der nach dem TCP-Handshake stumm bleibt, hängt den Ladevorgang auf
Dauer.

Der Closer aus `ssh.AuthMethods` wird in `Connect` erst am Ende an den Client
übergeben. Scheitert vorher `HostKeyCallback`, `NewClientConn` oder
`pkgsftp.NewClient`, bleibt der Agent-Socket offen. Dazu verwirft der
Keyfile-Zweig in `keyfileAuth` den Closer seines Agent-Fallbacks komplett
(`internal/ssh/auth.go:39`).

Empfehlung: Für die Dauer von SSH- und SFTP-Handshake ein `SetDeadline` auf der
TCP-Verbindung setzen und danach aufheben. Den Auth-Closer direkt nach Erzeugung
per `defer` absichern und erst bei erfolgreicher Übergabe freigeben. `keyfileAuth`
muss seinen Closer zurückgeben.

### 11. Niedrig: Lesefehler einseitiger Dateien werden ignoriert

Fundstelle: `internal/diff/engine.go:62-99`

Bestätigt, Bewertung von mittel auf niedrig gesenkt. Bei nur lokal oder nur remote
vorhandenen Dateien verwerfen `if data, err := ...; err == nil`-Blöcke den Fehler
von `client.ReadFile` und `os.ReadFile`. Das Ergebnis gilt als erfolgreich und
bekommt über `AutoDecision` Upload oder Download.

Der Schaden bleibt aber bei der Anzeige. Die Datei erscheint im Diff-View ohne
Zeilen, also wie eine leere Datei, was in die Irre führt. Der Transfer selbst liest
neu: `UploadFile` und `DownloadFile` würden dieselbe Datei wieder nicht lesen
können und melden den Fehler dann als `SyncFailure`.

Empfehlung: Lesefehler als Session-Fehler weitergeben. `AutoDecision` gibt für
Sessions mit `Err != nil` schon `DecisionNone` zurück, damit wäre die Datei
automatisch aus "Sync all" heraus.

### 12. Mittel: Unbekannte Protokolle fallen still auf SFTP zurück

Fundstelle: `internal/remote/client.go:34-41`

Bestätigt. `Connect` behandelt `ftp` und `ftps`, alles andere geht in den
`default`-Zweig und damit nach SFTP. Ein Tippfehler wie `sftpp` erzeugt einen
Verbindungsfehler, der nichts über die Ursache sagt. `config.Host.Protocol` wird
sonst nirgends validiert, `hostform` schränkt den Wert nur über seinen Umschalter
ein.

Empfehlung: Nur leer, `sftp`, `ftp` und `ftps` akzeptieren. Andere Werte müssen
schon beim Laden der Config einen Validierungsfehler erzeugen, nicht erst beim
Verbindungsaufbau.

### 13. Mittel: Logdateien sind für andere lokale Benutzer lesbar

Fundstelle: `internal/log/log.go:29-46`

Bestätigt. `Init` legt das Verzeichnis mit `0755` und die Datei mit `0644` an.
Geloggt werden Hostnamen sowie lokale und entfernte Pfade. Auf einem
Mehrbenutzersystem liest die jeder mit. Immerhin ist Logging standardmäßig aus und
braucht `--log`, `--debug` oder die passende Umgebungsvariable.

Empfehlung: Datei mit `0600` und Standardverzeichnis mit `0700` anlegen. Beim
Öffnen einer bestehenden Datei die Rechte verschärfen.

## Relevante Testlücken

Überlappende Host-Auswahlen deckt
`TestActiveNetworkOperationBlocksQuitAndSecondOperation` ab, den Projektwechsel
während eines laufenden Diff-Loads die drei Tests aus Finding 1. Alles andere
bleibt offen:

- Kein Regressionstest für unterschiedliche Inhalte mit gleicher Größe und mtime.
- Kein FTP-Walker-Test mit mehr als 4.096 gleichzeitig wartenden Verzeichnissen.
- `internal/fs` hat überhaupt keine Testdatei, also auch keine Tests für FIFOs,
  Symlinks und unlesbare Unterverzeichnisse. Der Aufruf
  `go test -race ./internal/fs` prüft dort nichts.
- Keine Persistenztests für Fehler während eines Schreibvorgangs oder für das
  Verschärfen bestehender Dateiberechtigungen.
- Keine Diff-Tests, die Lesefehler einseitiger Dateien als Fehler erwarten.
- Kein Test für ungültige `protocol`-Werte (Finding 12) und keiner für den
  Default-Port eines FTP-Hosts ohne eigene Portangabe (Finding 6).

## Ausgeführte Prüfungen

Am 2. September 2026 auf `d60e9b3` und danach erneut mit dem Fix zu Finding 1 auf
`fix/stale-diff-results` ausgeführt, alle erfolgreich:

```text
go build ./...
go vet ./...
go test ./...
go test -race ./internal/config ./internal/diff ./internal/fs ./internal/ftp ./internal/sftp ./internal/sync ./internal/tui/...
```

Die grünen Läufe widersprechen den offenen Findings nicht, deren Fehlerpfade deckt
keiner der vorhandenen Tests ab.

## Methodik und Einschränkungen

Erstfassung: Drei unabhängige Subagenten prüften Sicherheit, Architektur und
Concurrency sowie Protokollimplementierungen und Tests. Überschneidende Ergebnisse
wurden am Quellcode verifiziert und zusammengeführt.

Nachprüfung vom 2. September 2026: Jedes Finding wurde einzeln gegen den aktuellen
Code gelesen, Fundstellen und Zeilenangaben korrigiert und die Bewertung anhand der
tatsächlichen Auslösebedingungen neu gesetzt. Finding 1 wurde in `internal/tui`
reproduziert: Nach `dashboard.MsgProjectChosen` schaltete ein danach eintreffendes
`diffview.MsgDiffLoaded` weiterhin in den Diff-View. Aus dem Reproduktionstest
wurde beim Fix `TestDiffResultOfLeftProjectIsDiscarded`. Die übrigen zwölf Findings
sind unangetastet, an ihnen wurde nichts geändert.

Zeilenangaben beziehen sich auf `d60e9b3` plus den Fix zu Finding 1. Betroffen
davon war nur die Angabe in Finding 2 zu `internal/tui/diffview/model.go`, sie ist
nachgezogen. Die Fundstellen in Finding 1 selbst sind als Stand vom 27. August
gekennzeichnet und absichtlich nicht verschoben.

Der angeforderte Bugbot-Subagent war in der verwendeten Laufzeit nicht registriert
und konnte deshalb nicht ausgeführt werden.
