# 0003: Der Diff-Vergleich verwendet die bestehende Verbindung weiter

Status: Angenommen
Datum: 2026-11-20

## Kontext

`app.Load` vergleicht oft viele kleine Dateien. Bei jeder Datei kostet vor allem
die Rundlaufzeit der einzelnen Anfragen. Wenn der Vergleich beginnt, hat der
Browser meist schon eine Verbindung zum Host aufgebaut, und `LoadRequest.Conn`
übergibt sie an `Load`. Manche Server erlauben nur eine Sitzung pro Benutzer.
Verbindung und Verzeichnisliste klappen dort, jede weitere Anmeldung scheitert.

`docs/architecture-target.md` hält fest, dass `sync.Run` seriell bleibt. Diese
Entscheidung betrifft nur den Vergleich, nicht die Übertragung.

## Entscheidung

`forEachCompare` verteilt die Vergleiche auf einen begrenzten Worker-Pool. Der
erste Worker nutzt immer die bestehende Verbindung.

- Bei SFTP teilen sich bis zu 8 Worker (`maxDiffLoadWorkers`) diese eine
  Verbindung.
- Bei FTP und FTPS öffnet jeder weitere Worker eine eigene Verbindung, insgesamt
  höchstens 4 Worker (`maxFTPDiffLoadWorkers`).
- Lehnt der Server eine zusätzliche Anmeldung ab, läuft der Vergleich mit
  weniger Workern weiter. Ein Zertifikatsfehler beim zusätzlichen Verbindungsaufbau
  und ein terminaler Fehler auf einer bestehenden Verbindung brechen den Vergleich ab.

## Begründung

Weil der erste Worker die offene Verbindung nutzt, liefert auch ein Server mit
nur einer Sitzung pro Benutzer einen vollständigen Diff. Andernfalls würde jede
Datei mit einem Verbindungsfehler des Workers enden.

`pkg/sftp` reicht gleichzeitige Anfragen über eine Verbindung im Pipelining weiter.
Zusätzliche Worker überdecken so die Rundlaufzeit pro Anfrage, ohne neue
Verbindungen zu öffnen. Bei FTP braucht jeder weitere Worker eine eigene
Verbindung. Deren Aufbau ist teuer, und Server begrenzen häufig die Zahl
gleichzeitiger Anmeldungen. Deshalb bleibt die Obergrenze für FTP niedrig.

## Folgen

- Vergleiche über viele kleine Dateien laufen parallel, auch wenn der Server nur
  eine Sitzung zulässt.
- Ein Abbruch muss auch die zusätzlichen FTP-Verbindungen schließen. Das
  Schließen der primären Verbindung beendet deren laufende Ein- und Ausgabe nicht.
- Jede zusätzliche Verbindung muss dieselbe Zertifikatsfreigabe akzeptieren wie
  die erste, auch beim Wiederholungsversuch nach einer Zertifikatsabfrage.
- `fn` darf nur in Daten schreiben, die zu seinem Index gehören. Der Pool kommt
  dadurch ohne Sperren aus.

## Fundstellen

- `internal/app/compare.go`: Doc-Kommentar von `forEachCompare` (Begründung zur
  Wiederverwendung und zu `pkg/sftp`), `maxDiffLoadWorkers`,
  `maxFTPDiffLoadWorkers`, `diffLoadWorkers`
- `internal/app/compare_test.go`: `TestLoadDiffItemsUsesSingleFTPSession`,
  `TestForEachCompareAddsExtraFTPConnections`,
  `TestForEachComparePropagatesExtraWorkerTerminalFailure`
- `internal/app/trust_test.go`: `TestLoadRetriesUnknownFTPSCertificateAfterTrust`
