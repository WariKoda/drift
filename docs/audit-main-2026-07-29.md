# Audit des Sync-Kerns

Stand: 29. Juli 2026  
Audit-Basis: `main` auf Commit `5ab4f1e1e4b3e70011987ec9ade4265beeba8fab`

## Umfang

Der Audit betrachtet den aktuellen Stand von `main` in folgenden Bereichen:

- Korrektheit und Fehlerfälle
- Performance und Wartbarkeit
- SFTP, FTP und FTPS
- lokale und entfernte Pfad-Mappings
- Fehleranalyse und Stabilität des vollständigen Sync-Prozesses

Untersucht wurde der Datenfluss von der Auswahl im Browser über Mapping,
Verzeichnis-Walk und Diff-Erstellung bis zu Upload, Download und Löschen.

## Zusammenfassung

Der normale Sync kleiner Textdateien ist grundsätzlich funktionsfähig. Es gibt
jedoch vier kritische P1-Risiken:

1. Unterschiedliche Binärdateien und Dateien über 2 MiB können als identisch
   verschwinden und sind dann nicht synchronisierbar.
2. Uploads und Downloads überschreiben das Ziel direkt. Ein Abbruch kann eine
   vorhandene Datei abgeschnitten zurücklassen.
3. Nicht validierte Mappings können das lokale Projekt beziehungsweise den
   konfigurierten Remote-Root verlassen.
4. Mehrere asynchrone TUI-Aktionen können denselben, nicht nebenläufig sicheren
   FTP-Client gleichzeitig verwenden.

Es wurde kein universeller P0-Release-Blocker gefunden. Wegen der P1-Befunde
sollte der Sync für Binär-/Großdateien und auf instabilen Verbindungen aber noch
nicht als ausfallsicher gelten.

## Prioritäten

- **P0:** universeller Release-Blocker oder kritischer Totalausfall
- **P1:** dringender Defekt mit Gefahr von Datenverlust oder falschem Sync
- **P2:** wesentlicher Korrektheits- oder Stabilitätsdefekt
- **P3:** niedriger priorisierte Performance- oder Wartbarkeitsschuld

## P1-Befunde

### P1.1 Binär- und Großdateien werden als identisch verworfen

`diff.Compare` setzt bei Dateien über 2 MiB oder erkannten Binärdateien nur
`Binary = true` und kehrt ohne inhaltlichen Unterschiedsstatus zurück:

- `internal/diff/engine.go:107`
- `internal/diff/engine.go:121`

`DiffResult.HasDiff()` berücksichtigt `Binary` und abweichende Metadaten nicht.
`loadDiffItems` entfernt anschließend jedes Ergebnis, für das `HasDiff()` false
liefert:

- `internal/diff/result.go:56`
- `internal/tui/diffview/model.go:597`

Dadurch verschwinden zwei vorhandene, aber unterschiedliche Archive, Bilder,
Datenbanken oder große Textdateien vollständig aus der Diff-Session. Der
Benutzer kann keine Sync-Richtung auswählen.

**Empfohlener Fix**

- Einen expliziten Vergleichsstatus für vorhandene Binär-/Großdateien führen.
- Unterschiedliche Größen immer als Änderung werten.
- Bei gleicher Größe und ungleichen Metadaten einen Hash oder einen
  streamingbasierten Bytevergleich verwenden.
- `Binary` nur als Darstellungsmerkmal verwenden, nicht als Aussage über
  Gleichheit.
- Tests für kleine Binärdateien, Dateien über 2 MiB und identische Binärdateien
  ergänzen.

### P1.2 Transfers überschreiben Ziele nicht atomar

SFTP öffnet das endgültige Ziel vor dem Kopieren direkt mit `Create`:

- `internal/sftp/client.go:173`
- `internal/sftp/client.go:264`

FTP schreibt direkt mit `STOR` beziehungsweise `os.Create`:

- `internal/ftp/client.go:162`
- `internal/ftp/client.go:179`

Ein Netzwerkabbruch, ein voller Datenträger oder ein Prozessabbruch hinterlässt
eine teilweise geschriebene Zieldatei. Bei einem fehlgeschlagenen Download
erhält die lokale Teildatei zusätzlich eine neue Mtime. Die automatische
Richtungswahl kann sie beim nächsten Durchlauf als neuere lokale Version
einstufen und auf den Server zurückladen.

**Empfohlener Fix**

- In eine temporäre Datei im Zielverzeichnis schreiben.
- Schreib- und Close-Fehler vollständig prüfen.
- Erst nach erfolgreichem Abschluss atomar auf den Zielnamen umbenennen.
- Temporäre Dateien bei Fehlern bestmöglich entfernen.
- Fehlerfälle mit absichtlich abbrechenden realen Verbindungen testen.

**Status: behoben auf `fix/critical-sync-audit`**

- SFTP- und FTP-/FTPS-Uploads schreiben zunächst in zufällig benannte,
  versteckte Geschwisterdateien.
- SFTP ersetzt das Ziel per `PosixRename` und fällt sicher auf normales
  `Rename` zurück. Das alte Ziel wird niemals vorab gelöscht.
- FTP ersetzt das Ziel per `RNFR`/`RNTO`. Lehnt ein Server den atomaren
  Austausch eines vorhandenen Ziels ab, bleibt das Original erhalten und der
  Sync meldet einen Fehler.
- Downloads schreiben lokal in eine Geschwisterdatei, prüfen Copy-, Remote-
  Close-, Sync- und Local-Close-Fehler und verwenden erst danach `os.Rename`.
- Vorhandene lokale Dateimodi und SFTP-Dateimodi bleiben erhalten.
- Direkte Symlink-Ziele werden abgelehnt, damit der atomare Austausch nicht
  versehentlich einen Link durch eine reguläre Datei ersetzt.
- Fehlgeschlagene Transfers räumen ihre Staging-Datei bestmöglich auf.
- Reale In-Process-SFTP-Protokolltests decken erfolgreichen Austausch,
  Moduserhalt, Fehler nach dem Staging und Symlink-Ziele ab.

Verbleibendes Integrationsrisiko: In der lokalen Audit-Umgebung ist kein
echter FTP-/FTPS-Testserver vorhanden. Das Verhalten von `RNTO` beim Ersetzen
vorhandener Dateien und die serverseitigen Standardberechtigungen müssen vor
dem Release noch gegen die unterstützten FTP-Server geprüft werden. Der Code
verwendet bewusst keinen unsicheren Delete-plus-Rename-Fallback.

### P1.3 Mappings können Root-Grenzen verlassen oder kollidieren

Das Hostformular prüft Mapping-Felder nur auf Nicht-Leere:

- `internal/tui/hostform/model.go:247`

Der Mapper verbindet die Werte anschließend mit `filepath.Join`, wodurch
`..`-Segmente normalisiert werden:

- `internal/pathmap/mapper.go:49`
- `internal/pathmap/mapper.go:103`
- `internal/pathmap/mapper.go:119`

Beispiele:

- `local = "../outside"` kann Downloads außerhalb des Projekts ablegen.
- `remote = "../etc"` kann Uploads außerhalb von `RootPath` schreiben.
- Zwei lokale Mappings mit demselben Remote-Ziel können sich gegenseitig
  überschreiben.
- Mehrdeutige überlappende Mappings können in Hin- und Rückrichtung
  unterschiedliche Paare ergeben.

**Empfohlener Fix**

- Absolute Mapping-Pfade und `..`-Segmente ablehnen.
- Nach der Bereinigung prüfen, dass lokale Ziele innerhalb von `ProjectRoot`
  und Remote-Ziele innerhalb von `RootPath` liegen.
- Doppelte und mehrdeutige lokale sowie entfernte Mapping-Basen ablehnen.
- Dieselbe Validierung beim TOML-Laden und im Hostformular verwenden.
- Traversal-, Kollisions- und Überlappungstests ergänzen.

**Status: behoben auf `fix/critical-sync-audit`**

- `config.ValidateMappings` bildet eine gemeinsame Invariante für
  Projekt- und Host-Mappings.
- Leere und absolute Pfade sowie jedes `..`-Segment werden abgelehnt. Lokale
  Pfade werden mit lokalen Dateisystemregeln, Remote-Pfade mit POSIX-Regeln
  bereinigt.
- Nach der Bereinigung werden doppelte lokale und entfernte Basen erkannt.
- Verschachtelte Mappings sind nur erlaubt, wenn Richtung und relativer Suffix
  auf beiden Seiten exakt übereinstimmen. Dadurch bleiben konsistente
  Spezialisierungen möglich, asymmetrische Überlappungen aber nicht.
- Globale Hosts, Projekt-Mappings und Projekt-Hosts werden direkt beim
  TOML-Laden validiert. Die Writer validieren vor der Mutation des
  Laufzeitmodells und nochmals vor dem Schreiben.
- Das Hostformular zeigt den konkreten Fehler im Mapping-Editor an und bleibt
  bei ungültigen Eingaben geöffnet. Die irreführende Beschreibung des
  Deploy-Pfads als absolut wurde korrigiert.
- Der Laufzeit-Mapper validiert die tatsächlich aktive Mapping-Liste erneut.
  Zusätzlich lehnt der Fallback lokale Pfade außerhalb von `ProjectRoot` ab
  und bereinigt Remote-Pfade vor der Rückübersetzung.
- Tests decken TOML-Laden, Writer ohne Teilmutation, Formularfeedback,
  Traversal, absolute Pfade, normalisierte Duplikate, Zielkollisionen,
  asymmetrische Überlappungen, konsistente Verschachtelung und beide
  Übersetzungsrichtungen ab.

### P1.4 Ein FTP-Client kann gleichzeitig aus mehreren TUI-Kommandos verwendet werden

Die verwendete Bibliothek `github.com/jlaffaye/ftp` dokumentiert
`ServerConn` als nicht nebenläufig sicher und erlaubt nur eine aktive
Datenverbindung.

Der Remote-Browser kann trotzdem mehrere Verzeichnisse gleichzeitig über
dieselbe Verbindung laden:

- `internal/tui/browser/update.go:327`
- `internal/tui/browser/remote.go:74`

Quick-Upload und Quick-Download bleiben außerdem während anderer Sync- oder
Refresh-Aktionen erreichbar:

- `internal/tui/diffview/update.go:158`

Dadurch können FTP-Kommandos und Datenverbindungen ineinandergreifen. Mögliche
Folgen sind Protokollfehler, falsche Antworten, abgebrochene Transfers oder
eine unbrauchbare Verbindung.

**Empfohlener Fix**

- Alle Operationen einer `ftp.Client`-Instanz serialisieren.
- In der TUI nur eine Operation pro Verbindung zulassen.
- Wiederholte Quick-Aktionen und Refresh während Bulk-Sync sperren.
- Concurrency-Tests mit einem realen lokalen FTP-Server ergänzen.

## P2-Befunde

### P2.1 FTP-Diffing ignoriert die vorhandene Verbindung

`forEachCompare` öffnet für FTP bei jedem Worker eine neue Verbindung, selbst
wenn nur ein Worker läuft:

- `internal/tui/diffview/model.go:538`

Die bereits vorhandene und funktionierende Verbindung bleibt ungenutzt. Bei
einem Server, der nur eine Session pro Benutzer oder IP erlaubt, funktioniert
das Browsing, während anschließend jede Diff-Datei mit einem
Worker-Verbindungsfehler endet.

**Empfehlung:** Den ersten Worker mit der bestehenden Verbindung betreiben und
nur zusätzliche Worker bestmöglich separat verbinden. Bei fehlenden
Zusatzverbindungen auf weniger Parallelität zurückfallen.

### P2.2 Dateioperationen besitzen keine Timeouts oder Cancellation

Die Methoden von `remote.Client` akzeptieren keinen `context.Context`:

- `internal/remote/client.go:18`

Der 30-Sekunden-Context in `LoadCmd` begrenzt nur den Verbindungsaufbau:

- `internal/tui/diffview/model.go:334`

`LIST`, `RETR`, `STOR` sowie SFTP-Reads und -Writes können daher unbegrenzt
hängen. Das Verlassen des Ladebildschirms beendet nur den UI-Zustand, nicht die
laufende Arbeit.

**Empfehlung:** Context-aware Remote-Operationen oder kontrollierte
Verbindungs-Deadlines einführen und laufende Commands beim Verlassen des
Screens abbrechen.

### P2.3 FTP-Stat kann Dateien als Verzeichnisse einstufen

Schlägt `SIZE` fehl, reicht ein erfolgreiches `LIST`, damit `Stat` immer ein
Verzeichnis zurückgibt:

- `internal/ftp/client.go:72`

Die gelisteten Einträge und deren Typ werden ignoriert. Server ohne
`SIZE`-Unterstützung können deshalb normale Dateien als Verzeichnisse melden.

**Empfehlung:** Bevorzugt `MLST`/`GetEntry` verwenden und im Fallback das
Elternverzeichnis listen und den konkreten Eintrag samt Typ suchen.

### P2.4 Der lokale Walker verschluckt Fehler und akzeptiert Spezialdateien

Fehler von `filepath.WalkDir` werden ignoriert. Außer Symlinks werden alle
Nicht-Verzeichnisse an den Callback weitergegeben:

- `internal/fs/local.go:24`

Unlesbare Teilbäume fehlen damit lautlos im Sync. FIFOs, Devices oder Sockets
können später bei `os.ReadFile` hängen oder unerwartete Fehler erzeugen.

**Empfehlung:** Traversal-Fehler propagieren und ausschließlich reguläre
Dateien weiterreichen.

### P2.5 Quick-Sync-Fehler können der falschen Session zugeordnet werden

`MsgSyncError` enthält keinen Session-Index:

- `internal/tui/diffview/model.go:140`

Beim Empfang wird der Fehler der aktuell ausgewählten Session zugewiesen:

- `internal/tui/diffview/update.go:60`

Navigiert der Benutzer während des Transfers, zeigt Drift den Fehler bei einer
anderen Datei an.

**Empfehlung:** Session-Index, Operation und beide Pfade in der Fehlermeldung
mitführen.

### P2.6 Quick-Sync führt Remote-I/O synchron in `Update` aus

Nach `MsgSynced` ruft `Update` direkt `reloadSession` auf:

- `internal/tui/diffview/update.go:56`
- `internal/tui/diffview/model.go:756`

`reloadSession` führt mit `diff.Compare` lokale und entfernte I/O aus. Damit
kann Bubble Teas Event-Loop nach jedem Quick-Sync einfrieren.

**Empfehlung:** Den Reload als `tea.Cmd` ausführen und über eine indexierte
Result-Message anwenden.

### P2.7 SFTP kann SSH-Agent-Sockets leaken

Bei Auth-Typ `keyfile` ohne Pfad wird auf den Agenten zurückgefallen, dessen
Closer aber verworfen wird:

- `internal/ssh/auth.go:35`

Auch mehrere Fehlerpfade in `sftp.Connect` schließen einen bereits geöffneten
Auth-Closer nicht:

- `internal/sftp/client.go:38`

Wiederholte Verbindungen können dadurch Dateideskriptoren aufbrauchen.

**Empfehlung:** Den Closer durch alle Fallbacks zurückgeben und ihn auf jedem
Fehlerpfad schließen.

### P2.8 Remote-Browser-Ergebnisse sind nicht an eine Verbindungsgeneration gebunden

`MsgRemoteChildrenLoaded` enthält nur den Elternpfad:

- `internal/tui/browser/remote.go:24`

Nach Hostwechsel oder Refresh kann ein verspätetes Ergebnis des vorherigen
Hosts in einen gleichnamigen Pfad des neuen Hosts eingefügt werden.

**Empfehlung:** Host-ID und monoton steigende Verbindungsgeneration in jeder
Remote-Message mitführen und veraltete Ergebnisse inklusive Verbindung
verwerfen.

### P2.9 Diff- und Scanfehler sind unzureichend diagnostizierbar

Fehler aus `diff.Compare` werden nur in die Session geschrieben:

- `internal/tui/diffview/model.go:597`

Bulk-Fehler speichern für das Overlay lediglich `err.Error()` ohne Operation
oder Dateipaar:

- `internal/tui/diffview/model.go:707`

Da Logging standardmäßig deaktiviert ist, sind generische Fehler wie `EOF`
später keiner Datei zuzuordnen.

**Empfehlung:** Jeden Diff-, Mapping-, Walk- und Sync-Fehler mit Host,
Operation und beiden Pfaden loggen und dieselben strukturierten Angaben im
Overlay anzeigen.

### P2.10 Der Config-Merge setzt FTP ohne Port auf Port 22

Die Default-Logik setzt einen fehlenden Port protocolunabhängig auf 22:

- `internal/config/loader.go:70`

Der FTP-Client kann seinen vorgesehenen Fallback auf Port 21 danach nicht mehr
anwenden.

**Empfehlung:** Den Defaultport nach dem effektiven Protokoll bestimmen und
Tests für SFTP, FTP und FTPS ohne expliziten Port ergänzen.

### P2.11 Der parallele FTP-Walker kann deadlocken

Alle Worker konsumieren aus einem auf 4096 Einträge begrenzten Kanal und
schreiben neue Unterverzeichnisse wieder in denselben Kanal:

- `internal/ftp/client.go:212`
- `internal/ftp/client.go:271`

Wenn der Kanal voll ist und alle Worker gleichzeitig neue Verzeichnisse
produzieren, bleibt kein Consumer übrig.

**Empfehlung:** Scheduling und Verzeichniserkennung trennen, beispielsweise
über einen dedizierten Coordinator, der allein den Arbeitskanal befüllt.

## P3-Befunde

### P3.1 Überlappende Selektionen werden mehrfach gewalkt

Sind ein Elternverzeichnis und ein darin liegendes Kind markiert, werden beide
vollständig durchlaufen. Erst die fertigen Dateipaare werden dedupliziert:

- `internal/tui/diffview/model.go:368`

Das vervielfacht bei FTP insbesondere `LIST`-Aufrufe und Verbindungsaufbau.

**Empfehlung:** Vor dem Walk alle Selektionen entfernen, die bereits von einem
selektierten Elternverzeichnis abgedeckt sind.

### P3.2 Sync-Plan-Typen und tatsächliche Ausführung sind getrennt

`internal/sync.Plan`, Fortschrittstypen und `AppState.SyncPlan` sind praktisch
ungenutzt:

- `internal/sync/plan.go:14`
- `internal/tui/state.go:65`

Die tatsächliche Ausführung liegt weiterhin im großen `diffview`-Paket. Das
erschwert isolierte Tests, Retry-Logik, Cancellation und einheitliche
Fehlerobjekte.

**Empfehlung:** Entweder den geplanten Sync-Engine-Schnitt umsetzen oder die
ungenutzten Typen bis zur Migration entfernen.

## Verifikation

Folgende Prüfungen waren erfolgreich:

```text
go test -count=1 ./...
go test -count=1 -race ./...
go vet ./...
go build ./...
git diff --check
```

Der ursprüngliche Audit wurde ohne Codeänderungen durchgeführt. Die
Behebungen von P1.2 und P1.3 liegen anschließend gemeinsam und noch
uncommitted auf `fix/critical-sync-audit`.

## Testabdeckung und Restrisiken

Gemessene Statement-Coverage:

| Paket | Coverage |
|---|---:|
| `internal/ftp` | 0,0 % |
| `internal/sftp` | 0,0 % |
| `internal/ssh` | 0,0 % |
| `internal/remote` | 0,0 % |
| `internal/fs` | 0,0 % |
| `internal/tui/diffview` | 1,3 % |
| `internal/tui/browser` | 3,1 % |
| `internal/diff` | 43,5 % |
| `internal/pathmap` | 89,6 % |
| `internal/sync` | 100,0 % |

Nach der Behebung von P1.2 erreicht `internal/sftp` durch die neuen realen
Protokolltests 34,1 % Statement-Coverage. Die Tabelle hält weiterhin den
ursprünglich auditierten Stand von `main` fest.

Die grünen Tests erfassen die risikoreichsten Netzwerk- und Fehlerpfade somit
kaum. Wegen der Projektregel „keine Mocks“ sollten Protokolltests über reale,
lokal gestartete SFTP-/FTP-/FTPS-Server laufen oder explizit übersprungen
werden, wenn diese Testumgebung nicht verfügbar ist.

Insbesondere fehlen Tests für:

- unterschiedliche und identische Binärdateien
- Text- und Binärdateien über 2 MiB
- abgebrochene Uploads und Downloads
- FTP-Server mit nur einer erlaubten Session
- FTP ohne `SIZE`-Unterstützung
- hängende oder abbrechende Netzwerkoperationen
- gleichzeitige Remote-Browser- und Quick-Sync-Aktionen
- unlesbare Verzeichnisse und lokale Spezialdateien

## Geplante Behebungsreihenfolge

Die Punkte werden einzeln implementiert, getestet und überprüft:

- [ ] P1.1 Binär-/Großdateien korrekt als geändert erkennen
- [x] P1.2 Uploads und Downloads atomar ausführen
- [x] P1.3 Mapping-Grenzen und Kollisionen validieren
- [ ] P1.4 FTP-Operationen pro Verbindung serialisieren
- [ ] P2.1 Bestehende FTP-Verbindung für einen Diff-Worker verwenden
- [ ] P2.2 Cancellation und Operations-Timeouts einführen
- [ ] P2.3 FTP-Stat robust implementieren
- [ ] P2.4 Lokale Walk-Fehler und Spezialdateien korrekt behandeln
- [ ] P2.5 Quick-Sync-Fehler eindeutig zuordnen
- [ ] P2.6 Session-Reload vollständig asynchron ausführen
- [ ] P2.7 SSH-Agent-Ressourcen zuverlässig schließen
- [ ] P2.8 Veraltete Remote-Browser-Ergebnisse verwerfen
- [ ] P2.9 Strukturierte Sync-Diagnostik vervollständigen
- [ ] P2.10 Protokollabhängige Defaultports korrigieren
- [ ] P2.11 FTP-Walker ohne möglichen Queue-Deadlock aufbauen
- [ ] P3.1 Überlappende Selektionen vor dem Walk reduzieren
- [ ] P3.2 Sync-Ausführung aus der TUI herauslösen oder tote Typen entfernen

Jeder erledigte Punkt sollte in diesem Dokument zusammen mit den zugehörigen
Tests und dem Commit aktualisiert werden.
