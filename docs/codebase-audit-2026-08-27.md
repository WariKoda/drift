# Codebase-Audit

Stand: 27. August 2026  
Branch: `docs/release-readiness-audit-2026-08-09`

## Zusammenfassung

Der Audit hat 13 bestätigte Findings ergeben:

- 1 kritisch
- 7 hoch
- 5 mittel

Vor einem Release sollten mindestens die Korrelation asynchroner Host-Anfragen, die lokale Symlink-Behandlung, der Metadaten-Kurzschluss beim Diff, der FTP-Walker und das Schreiben der Konfiguration korrigiert werden.

## Findings

### 1. Kritisch: Veraltete Host-Ergebnisse können gegen den falschen Host synchronisieren

Fundstellen: `internal/tui/app.go:376-428`, `internal/tui/hostselector/model.go:61-64`

Mehrere Host-Auswahlen können überlappende Ladeoperationen starten. Die Ergebnisnachricht enthält keine Request-ID und `MsgDiffLoaded` wird nur gegen `diffLoading` und einen vorhandenen `SelectedHost` geprüft. Ein Ergebnis für Host A kann deshalb angenommen werden, nachdem Host B ausgewählt wurde. Die Oberfläche zeigt dann Host B, während Sessions und Verbindung zu Host A gehören.

Empfehlung: Jede Verbindungs- und Diff-Anfrage erhält eine eindeutige ID. Ergebnisnachrichten müssen diese ID und die Host-Identität enthalten. Nur das Ergebnis der aktiven Anfrage darf übernommen werden; abgelehnte Verbindungen müssen geschlossen werden.

### 2. Hoch: Lokale Symlinks umgehen die Projektgrenze

Fundstellen: `internal/pathmap/mapper.go:121-143`, `internal/tui/diffview/model.go:363-377`, `internal/sftp/client.go:383-405`, `internal/ftp/client.go:263-285`

Die Pfadprüfung ist lexikalisch. Ein Pfad wie `project/output/file.txt` gilt als innerhalb des Projekts, auch wenn `output` ein Symlink auf ein Verzeichnis außerhalb des Projekts ist. Downloads und Löschoperationen können dadurch fremde Dateien überschreiben oder entfernen. Uploads können Inhalte außerhalb des Projekts lesen.

Empfehlung: Symlinks in ausgewählten Pfaden und allen vorhandenen Pfadkomponenten ablehnen. Verändernde lokale Operationen sollten relativ zu einem geöffneten Projektverzeichnis mit Beneath- und No-Symlink-Auflösung ausgeführt werden.

### 3. Hoch: Gleiche Größe und mtime gelten fälschlich als gleicher Inhalt

Fundstelle: `internal/diff/engine.go:106-109`

Wenn Größe und Änderungszeit übereinstimmen, liest der Diff-Engine keinen Inhalt. Gleich große Änderungen mit erhaltener oder grob aufgelöster mtime verschwinden dadurch vollständig aus Diff und Sync-Plan.

Empfehlung: Den Metadaten-Kurzschluss entfernen. Inhalte müssen verglichen oder anhand eines vertrauenswürdigen Content-Hashes geprüft werden.

### 4. Hoch: Der FTP-Walker kann deadlocken

Fundstellen: `internal/ftp/client.go:365-441`

Die Worker lesen aus einem Kanal mit 4.096 Einträgen und schreiben neu entdeckte Verzeichnisse synchron in denselben Kanal. Wenn alle Worker beim Schreiben in den vollen Kanal blockieren, kann kein Worker weitere Einträge lesen. Der Scan bleibt dauerhaft stehen.

Empfehlung: Eine eigene Koordinator-Goroutine sollte die Verzeichniswarteschlange verwalten. Worker dürfen nicht blockierend in eine begrenzte Warteschlange schreiben, die nur von denselben Workern gelesen wird.

### 5. Hoch: Konfigurationsschreibvorgänge sind nicht atomar

Fundstellen: `internal/config/writer.go:14-49`, `internal/config/writer.go:123-128`

Host-Slices und die zusammengeführte Laufzeitkonfiguration werden vor dem Schreiben verändert. Schlägt der Write fehl, stimmt der Speicherzustand nicht mehr mit der Datei überein. `os.WriteFile` schreibt außerdem direkt in die Zieldatei und kann bei Fehlern eine abgeschnittene Konfiguration hinterlassen.

Empfehlung: Zuerst eine Kandidatenkonfiguration erstellen. Diese in eine neue Datei mit Modus `0600` schreiben, synchronisieren und atomar umbenennen. Erst danach den Laufzeitstatus aktualisieren.

### 6. Hoch: FTP und FTPS verwenden standardmäßig Port 22

Fundstellen: `internal/config/loader.go:47-49`, `internal/config/loader.go:87-93`, `internal/ftp/client.go:35-39`

Der globale Default-Port 22 wird auf jeden Host ohne expliziten Port angewendet. Dadurch erreicht der FTP-Client seinen eigenen Port-21-Fallback nicht.

Empfehlung: Standardports erst nach Auflösung des Protokolls setzen. Für FTP und FTPS gilt 21, für SFTP 22.

### 7. Hoch: Unbekannte SSH-Hostkeys werden automatisch akzeptiert

Fundstellen: `internal/ssh/knownhosts.go:14-62`

Beim ersten Verbindungsaufbau wird ein unbekannter Schlüssel ohne Rückfrage in `known_hosts` geschrieben und akzeptiert. Ein Angreifer im Netzwerk kann bei dieser ersten Verbindung Zugangsdaten abfangen und Transfers manipulieren.

Empfehlung: Bei unbekannten Schlüsseln abbrechen und den SHA-256-Fingerprint zur Bestätigung anzeigen. Für unbeaufsichtigte Verbindungen sollten konfigurierbare Hostkey-Pins unterstützt werden.

### 8. Hoch: Projektkonfigurationen können Umgebungsgeheimnisse weiterleiten

Fundstellen: `internal/config/config.go:7-29`, `internal/config/loader.go:120-124`, `internal/ssh/auth.go:27-29`, `internal/ftp/client.go:62-63`

Ein Projekt kann einen globalen Host anhand seines Namens überschreiben. Auth-Felder unterstützen die Expansion beliebiger Umgebungsvariablen. Eine fremde `.drift/config.toml` kann dadurch etwa `$AWS_SECRET_ACCESS_KEY` als Passwort an einen vom Projekt vorgegebenen Server senden.

Empfehlung: Keine Secret-Expansion für projektbezogene Zugangsdaten erlauben. Projekt-Hosts dürfen globale Hosts nicht unbemerkt überschreiben. Vor der ersten Verbindung sollte Drift Quelle, Ziel und referenzierte Variable bestätigen lassen.

### 9. Mittel: Lokales Walking verschluckt Fehler und akzeptiert Spezialdateien

Fundstelle: `internal/fs/local.go:24-40`

Fehler von `filepath.WalkDir` werden in Erfolg umgewandelt. Unlesbare Unterbäume fehlen deshalb ohne Warnung im Ergebnis. Zugleich werden neben regulären Dateien auch FIFOs, Sockets und Geräte an den Callback weitergegeben. Ein späterer Lesezugriff auf eine FIFO kann unbegrenzt blockieren.

Empfehlung: Traversierungsfehler mit Pfadangabe weitergeben und ausschließlich reguläre Dateien ausgeben.

### 10. Mittel: SFTP-Handshakes haben keine wirksame Frist und verlieren Agent-Sockets

Fundstellen: `internal/sftp/client.go:38-84`, `internal/ssh/auth.go:35-40`

Der Context begrenzt den TCP-Aufbau, aber nicht den anschließenden SSH- oder SFTP-Handshake. Ein Server kann die Operation nach erfolgreichem TCP-Aufbau unbegrenzt blockieren. Fehler nach dem Öffnen des SSH-Agent-Sockets schließen diesen nicht auf allen Pfaden. Der Keyfile-Fallback verwirft seinen Closer ebenfalls.

Empfehlung: Während SSH- und SFTP-Handshake ein Deadline auf der TCP-Verbindung setzen. Den Auth-Closer unmittelbar nach Erstellung per `defer` absichern und erst bei erfolgreicher Übergabe an den Client freigeben.

### 11. Mittel: Lesefehler einseitiger Dateien werden ignoriert

Fundstelle: `internal/diff/engine.go:62-98`

Bei nur lokal oder nur remote vorhandenen Dateien werden Fehler aus `ReadFile` und `os.ReadFile` verworfen. Das Ergebnis gilt trotzdem als erfolgreich und erhält eine automatische Sync-Richtung.

Empfehlung: Lesefehler als Session-Fehler weitergeben. Automatische Synchronisierung muss für Inhalte deaktiviert werden, die nicht gelesen werden konnten.

### 12. Mittel: Unbekannte Protokolle fallen still auf SFTP zurück

Fundstelle: `internal/remote/client.go:35-40`

Jeder Protokollwert außer `ftp` und `ftps` verwendet SFTP. Ein Tippfehler führt zu irreführenden Verbindungsfehlern und kann einen unbeabsichtigten Dienst ansprechen.

Empfehlung: Nur leer, `sftp`, `ftp` und `ftps` akzeptieren. Andere Werte müssen vor dem Verbindungsaufbau einen Validierungsfehler erzeugen.

### 13. Mittel: Logdateien sind für andere lokale Benutzer lesbar

Fundstelle: `internal/log/log.go:29-45`

Logdateien werden mit Modus `0644` erstellt und enthalten Hostnamen sowie lokale und entfernte Pfade. Auf Mehrbenutzersystemen können andere Benutzer diese Betriebsdaten lesen.

Empfehlung: Logdateien mit Modus `0600` und das Standardverzeichnis mit `0700` erstellen. Bestehende Berechtigungen müssen beim Öffnen verschärft werden.

## Relevante Testlücken

- Kein Regressionstest für unterschiedliche Inhalte mit gleicher Größe und mtime.
- Kein FTP-Walker-Test mit mehr als 4.096 gleichzeitig wartenden Verzeichnissen.
- Keine Tests in `internal/fs` für FIFOs, Symlinks und unlesbare Unterverzeichnisse.
- Keine Persistenztests für Fehler während eines Schreibvorgangs oder das Verschärfen bestehender Dateiberechtigungen.
- Keine TUI-Tests für überlappende Host-Auswahlen und veraltete asynchrone Ergebnisse.
- Keine Diff-Tests, die Lesefehler einseitiger Dateien als Fehler erwarten.

## Ausgeführte Prüfungen

Die folgenden Befehle waren erfolgreich:

```text
go test ./...
go test -race ./internal/config ./internal/diff ./internal/fs ./internal/ftp ./internal/sftp ./internal/sync ./internal/tui/...
go vet ./...
go build ./...
```

Die erfolgreichen Prüfungen widersprechen den Findings nicht. Die betroffenen Fehlerpfade werden von den vorhandenen Tests nicht abgedeckt.

## Methodik und Einschränkungen

Drei unabhängige Subagenten prüften Sicherheit, Architektur und Concurrency sowie Protokollimplementierungen und Tests. Überschneidende Ergebnisse wurden direkt am Quellcode verifiziert und zusammengeführt.

Der angeforderte Bugbot-Subagent war in der verwendeten Laufzeit nicht registriert und konnte deshalb nicht ausgeführt werden. Es wurden keine Produktionsdateien verändert.
