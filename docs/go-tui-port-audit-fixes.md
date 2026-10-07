# Go-TUI: Fehlerkorrekturen aus dem Portierungs-Audit

Branch: `fix/go-tui-port-audit`.

## Behobene Fehler

- **Sync-Start:** Die lokalen und entfernten Auswahl-Maps werden in `Update`
  kopiert, bevor der Hintergrundbefehl startet. Spätere Eingaben können den
  Snapshot nicht mehr ändern.
- **Projektwechsel:** Config, Browser und Gitignore-Snapshot werden im
  Hintergrund vorbereitet. Abgebrochene oder überholte Ergebnisse werden
  verworfen; bei Fehlern bleibt die alte Session erhalten. `OpenedAt` wird
  ebenfalls asynchron gespeichert, mit dem Zeitpunkt des erfolgreichen
  Wechsels. Neuere Zeitstempel bleiben erhalten; verzögerte Ergebnisse führen
  nötigenfalls zu einem frischen Registry-Abgleich.
- **Remote-Verzeichnisabfragen:** Ein 30-Sekunden-Limit und Esc/Ctrl+C-Abbruch
  verhindern dauerhaft blockierte Browser-Aktionen. Der Abbruch schließt die
  Verbindung. Veraltete Ergebnisse werden verworfen; es gibt keinen automatischen Retry.
- **SSH-Agent:** Der Fallback bei leerem Keyfile gibt seinen Socket-Closer an
  den Verbindungsbesitzer zurück.
- **Server-Promotion:** Scheitert der Projekt-Store-Write, wird der globale Write
  unter derselben Sperre zurückgesetzt. Rücksetzfehler werden ebenfalls
  weitergegeben. Die laufende Config ändert sich erst nach beiden Writes.
- **Fehlerbehandlung:** FTP toleriert bei MDTM nur ausdrückliche
  Nicht-implementiert-Antworten (`502`/`504`); andere Fehler bleiben sichtbar.
  Die bestehende Prüfung auf tatsächlich fehlende Dateien bleibt erhalten.
  SFTP verbindet Read- und Close-Fehler. Scan-Fehler werden mit Pfadkontext geloggt.

## Validierung

Regressionstests verwenden reale Dateisysteme, SSH-Agent-Sockets und
FTP-/SFTP-Verbindungen, keine neuen Mocks. Bestanden:

```bash
go test -count=1 ./...
go test -race -count=1 ./...
go vet ./...
go build ./...
DRIFT_RUST_STORE_PROBE="$PWD/rust/target/debug/examples/store_probe" go test -count=1 ./internal/parity
```

Die Paritätsprüfung verwendete das bereits gebaute Rust-Store-Probe-Binary.

## Abgrenzung

Fehlende prozessübergreifende Schreibsperren waren bereits mit `24245f6` behoben.
Die Architektur-/Aufräumbefunde des Audits sowie synchrones lokales Aufklappen
und Host-CRUD sind nicht Teil dieser Fehlerkorrektur. Die vorhandenen
Rust-Arbeitsbaumänderungen wurden nicht verändert.
