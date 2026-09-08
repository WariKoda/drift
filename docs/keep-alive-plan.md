# Implementierungsplan: Keep-alive für Remote-Verbindungen

Status: implementiert auf `feature/keep-alive`, automatisiert geprüft.
Planungsstand: auf Basis von `49f1f69`, einschließlich FTPS-Zertifikatsvertrauen.
Dokumentationsbranch: `docs/keep-alive-plan`.

## Umsetzungsstand

- Konfiguration und Hostformular unterscheiden Standard, explizites Abschalten
  und eigenes Intervall. Standard sind 60 Sekunden, Probe-Timeout 15 Sekunden.
  Im Formular übernimmt `Disable keep-alive` das Abschalten und blendet das
  Intervallfeld aus. Beim Umschalten bleibt dessen Eingabe innerhalb derselben
  Bearbeitung erhalten; gespeichert wird deaktiviertes Keep-alive weiterhin als `0`.
- FTP/FTPS verwenden leerlaufgebundene `NOOP`-Probes, SFTP regelmäßige SSH-Anfragen.
  Transportabbruch, Monitor-Ende und terminaler Fehler sind verbindungsgebunden.
- Browser und Diff zeigen Verbindungsverluste auch hinter einem Modal. Alte
  Browser-Generationen werden abgewiesen, abgebrochene Listings schließen ihren
  Client. Lokale Previews bleiben von Remote-Fehlern unberührt.
- Bestätigte Sync-Erfolge bleiben erhalten. Neue Netzwerkaktionen auf einer
  defekten Verbindung sind gesperrt. Es gibt keine automatischen Retries.
- Beim Schließen werden laufende Diff-Befehle beendet und abgewartet. Noch nicht
  gestartete Befehle werden abgewiesen, damit Programmende nicht auf Befehle wartet,
  die Bubble Tea nach dem Beenden möglicherweise nie mehr startet.
- `README.md`, `CHANGELOG.md` und `AGENTS.md` sind aktualisiert.

Erfolgreich geprüft: LSP, `go test ./...`, `go test -race ./...`, `go vet ./...`,
`go build ./...`, zusätzliche wiederholte Race-Tests und `make update`.
Transporttests verwenden echte lokale FTP-, FTPS- und SSH/SFTP-Server.

Offen bleibt die manuelle TUI-Abnahme mit längeren Bedienpausen und echten
Netzunterbrechungen. Die nachstehenden Abschnitte dokumentieren den ursprünglichen
Plan und die Abnahmekriterien, nicht zusätzliche bereits durchgeführte Tests.

## Ziel

Offene Remote-Verbindungen sollen während Bedienpausen möglichst erhalten bleiben.
Ein erkannter Verbindungsverlust soll im Browser und Diff sichtbar werden, bevor
Benutzer eine weitere Aktion auf einer bereits unbrauchbaren Verbindung starten.

Keep-alive ist keine Garantie gegen Serverlimits, Netzwechsel oder Verbindungsabbrüche.
Es ersetzt weder Verbindungs-Timeouts noch eine Wiederverbindung. Insbesondere darf
es niemals Uploads, Downloads oder Löschvorgänge automatisch wiederholen.

## Ausgangszustand vor der Umsetzung

- `internal/ftp/client.go` verwendet einen Verbindungs-Timeout, sendet aber kein
  regelmäßiges `NOOP`. `opMu` serialisiert FTP-Kommandos. Streaming-Lesevorgänge
  halten diese Sperre bis zum Schließen des Readers.
- `internal/sftp/client.go` öffnet SSH und ein SFTP-Subsystem ohne eigene
  regelmäßige SSH-Keep-alive-Anfragen.
- Go kann über die verwendeten Dialer TCP-Keep-alive aktivieren. TCP-Probes sind
  jedoch kein Ersatz für Protokollkommandos gegen Anwendungs-Idle-Timeouts.
- `internal/remote/client.go` definiert die gemeinsame Schnittstelle und Fabrik.
  Es existiert kein gemeinsamer asynchroner Verbindungsstatus.
- Browser und Diff halten Verbindungen über einzelne Aktionen hinaus offen.
  Zusätzliche Diff- und FTP-Walk-Worker öffnen weitere Verbindungen.
- Die Contexts zum Verbindungsaufbau enden teilweise unmittelbar nach einem
  erfolgreichen Ladeauftrag. Sie eignen sich nicht als Lebenszeit des Keep-alive.

## Umfang und Entscheidungen

Zum ersten Lieferumfang gehören:

- FTP und explizites FTPS mit `NOOP` auf der Steuerverbindung.
- SFTP mit einer SSH-Anfrage `keepalive@openssh.com`, die eine Antwort verlangt.
- Pro Host ein konfigurierbares Intervall, standardmäßig 60 Sekunden, mit `0`
  zum Abschalten.
- Begrenzte Wartezeit auf eine Probe-Antwort, zunächst intern 15 Sekunden.
- Verbindungsgebundener Lebenszyklus und sichtbare Fehler ohne automatische Retries.
- Tests mit echten lokalen FTP-, FTPS- und SSH/SFTP-Verbindungen.

Nicht enthalten sind automatische Wiederverbindung, Transfer-Wiederaufnahme,
implizites FTPS, neue TLS-Versionen, neue Zertifikatsausnahmen und eine allgemeine
Hintergrundjob-Infrastruktur. TCP-Keep-alive-Tuning bleibt ein getrenntes Thema.

Die Standardwerte sind Implementierungsvorgaben für diesen Plan. Alle Zeitwerte
an einer passenden Stelle benennen, nicht über Clients und Views verteilen.

## Konfiguration und Hostformular

Neues Host-Feld `keep_alive_interval` als ganzzahlige Sekunden:

```toml
[[hosts]]
name = "staging"
hostname = "staging.example.com"
protocol = "ftps"
keep_alive_interval = 60
```

Semantik:

| Wert | Bedeutung |
| --- | --- |
| Feld fehlt | Standardintervall 60 Sekunden |
| `0` | Protokoll-Keep-alive deaktiviert |
| `1` bis `86400` | Explizites Intervall in Sekunden |
| Negativ, größer als `86400` oder falscher TOML-Typ | Sichtbarer Konfigurationsfehler |

`config.Host` benötigt eine optionale Zahl, beispielsweise `*int`, damit fehlend
und explizit `0` unterscheidbar bleiben. Die Auflösung zum Laufzeitwert darf beim
Speichern keinen zuvor fehlenden Wert in andere Hosts schreiben.

- `internal/config/config.go`, Ladevalidierung, `writer.go` und Store-Persistenz
  anpassen. Das Encoding-Feld darf explizit `0` nicht über `optionalInt` verlieren.
- Bestehende Regeln für globale und projektbezogene Hosts beibehalten. Im ersten
  Schritt kein zusätzliches Feld unter `[defaults]` und kein CLI-Flag einführen.
- Im Hostformular für alle Protokolle den Schalter `Disable keep-alive` und
  `Keep-alive interval (seconds)` ergänzen. Bei deaktiviertem Keep-alive das
  Intervallfeld ausblenden. Sonst bedeutet ein leeres Feld Standard; gültige
  Eingaben sind 1 bis 86400 Sekunden. Hilfetext auf Englisch.
- Fokusreihenfolge, Validierung, Bearbeiten und Speichern testen. Konfiguration
  ausschließlich unter `config.Dir()` schreiben, niemals ins Projektverzeichnis.
- Änderungen gelten für neue Verbindungen; keine laufende Session heimlich umbauen.

## Architektur und Lebenszyklus

### Verantwortung der Clients

Die konkreten Protokoll-Clients besitzen ihren Monitor und dessen Stop-Signal.
Er startet erst nach erfolgreichem Login beziehungsweise SFTP-Subsystem-Aufbau.
Die Fabrik bleibt der einzige Einstiegspunkt für TUI-Verbindungen.

- Pro Verbindung höchstens eine laufende Probe, keine anwachsende Probe-Warteschlange.
- Der Monitor verwendet einen eigenen Lebenszeit-Context, nicht den temporären
  Connect-Context. Ein abgebrochener Aufbau darf keinen Monitor zurücklassen.
- `Close()` stoppt den Monitor und gibt Ressourcen auch nach einem Probe-Fehler frei.
  Mehrfaches oder konkurrierendes Schließen muss sicher sein.
- Ein Ablauf zum erzwungenen Transportabbruch muss blockierte Netzwerk-I/O lösen
  können, ohne auf die von genau dieser I/O gehaltene Operationssperre zu warten.
- Netzwerk-I/O und wartendes Schließen laufen niemals direkt in `Update()`.
- Monitor und Fehlerzustand sind synchronisiert. Ein TUI-Modell darf nicht aus
  einem Hintergrund-Goroutine heraus verändert werden.

Vor der Umsetzung die konkreten APIs der eingesetzten Bibliotheksversionen prüfen:
`NoOp`, Zugriff auf den Steuertransport, Deadlines, erzwungenes Schließen und
`SendRequest`. Wenn dafür ein eigener Dialer den Transport festhalten muss, ihn
gezielt im jeweiligen Client ergänzen, nicht per Reflection oder Bibliotheks-Fork.

### FTP und FTPS

`NOOP` ist eine normale Operation auf dem bestehenden Steuerkanal. Bei FTPS bleibt
sie innerhalb der bereits geprüften TLS-Verbindung; es wird kein Datenkanal geöffnet.

1. Zeitpunkt der letzten abgeschlossenen Operation erfassen.
2. Nur nach Ablauf des Leerlaufintervalls eine Probe versuchen.
3. Die bestehende Operationssperre nicht wartend belegen: Ist sie beschäftigt,
   die Probe überspringen und später erneut prüfen.
4. Nach Übernahme der Sperre Leerlauf und Verbindungszustand erneut prüfen.
5. `NOOP` mit begrenzter Antwortzeit ausführen. Eine nur dafür gesetzte Deadline
   vor Freigabe der Sperre entfernen, damit sie spätere Transfers nicht abbricht.
6. Antwortfehler, EOF und Timeout als Verbindungsfehler behandeln. Nach Timeout
   die Verbindung verwerfen, weil verspätete Antworten die Kommandozuordnung
   beschädigen könnten.

Während `Open()` bis einschließlich Reader-`Close()`, `Upload()` oder anderer
laufender FTP-Operationen darf kein `NOOP` dazwischenkommen. Ein Langzeittransfer
wird nicht wegen einer ausgelassenen Probe als tot bewertet. Ein allgemeiner
Transfer-Stall-Timeout ist nicht Teil dieses Features.

Zusätzliche Verbindungen aus `parallelWalkFiles` und Diff-Workern berücksichtigen.
Jede Verbindung besitzt höchstens einen Monitor, der beim Worker-Ende mit endet.
Server können auch aktive Transfers wegen eigener Limits abbrechen; Keep-alive
während Bedienpausen löst dieses getrennte Problem nicht.

### SFTP über SSH

SSH kann globale Anfragen neben SFTP-Kanaldaten transportieren. Daher darf die
Probe auch während eines Transfers laufen, ohne SFTP-Dateikommandos einzuschieben.
Hier ist ein regelmäßiges Intervall ausreichend; kein zusätzlicher Zähler für
jedes übertragene SFTP-Paket erforderlich.

- `SendRequest("keepalive@openssh.com", true, nil)` verwenden.
- Eine negative SSH-Antwort ohne Transportfehler bestätigt ebenfalls, dass der
  Peer erreichbar ist. Sie ist kein Grund, die Verbindung zu schließen.
- Fehler oder eine ausbleibende Antwort innerhalb der Frist beenden die Verbindung.
- Da `SendRequest` keinen Context-Parameter hat, muss der Timeout den zugrunde
  liegenden Transport schließen und das Ende der blockierten Anfrage sicherstellen.
  Kein zurückgelassenes Goroutine pro abgelaufener Probe.
- Keine kurze globale Socket-Deadline über einen gleichzeitig laufenden Transfer
  legen. Die Probe bekommt eine eigene überwachte Frist.

SSH-Keep-alive bestätigt die SSH-Verbindung, nicht zwingend die Funktionsfähigkeit
oder fortbestehende Freigabe des SFTP-Subsystems. Diese Grenze dokumentieren.

## Fehlerweitergabe und TUI

Die gemeinsame Remote-Schnittstelle benötigt eine kleine Möglichkeit, einen
terminalen Verbindungsfehler zu beobachten. Vorgeschlagen sind ein pro Verbindung
schließendes `Done()`-Signal und ein synchronisiert lesbarer `Err()`-Wert.
Der Fehler wird vor dem Signal gespeichert; normales Schließen liefert keinen
Keep-alive-Fehler. Das verhindert konkurrierende Verbraucher eines Fehlerkanals.

Die konkrete API vorab an Browser, Diff und kurzlebigen Worker-Verbindungen prüfen.
Keine allgemeine Event-Bus-Abstraktion und keine zyklischen Paketabhängigkeiten.

- Ein wartender `tea.Cmd` übersetzt den Status in eine typisierte Nachricht mit
  Verbindungs-ID, Projekt-/Hostzuordnung und ursprünglichem Fehler.
- Nach einem Eigentümerwechsel der Verbindung vom Browser zum Diff den bisherigen
  Beobachter ablösen. Verspätete Nachrichten dürfen keine neue Verbindung schließen.
- Beim Zurückkehren zum Browser keine bereits fehlgeschlagene Verbindung übernehmen.
- Host-/Projektwechsel, Abbruch, Programmende und Zurücksetzen des FTPS-Vertrauens
  stoppen oder invalidieren die zugehörigen Beobachter und Monitore.
- Während Modals Verbindungsnachrichten im Root-Modell verarbeiten oder sicher
  zwischenspeichern. Sie dürfen nicht durch modales Input-Routing verloren gehen.
- Fehler aus kurzlebigen Tests und Workern auch über ihre regulären Ergebnispfade
  weiterreichen. Keine Fehlermeldung allein deshalb verlieren, weil noch kein
  TUI-Beobachter registriert war.

Im Browser den Remote-Status als getrennt markieren und neue Netzwerkaktionen auf
diesem Client verhindern. Bereits sichtbare Einträge dürfen als veraltet erhalten
bleiben; ein neuer expliziter Verbindungsaufbau lädt sie erneut.

Im Diff schreibende Aktionen auf der defekten Verbindung sperren. Bereits bestätigte
Sync-Erfolge behalten ihren Status. Bei einem Abbruch während einer Aktion kennt
der Client möglicherweise deren serverseitigen Endzustand nicht; dies sichtbar
machen und vor einem weiteren Sync einen neuen Vergleich verlangen.

Kein automatischer Reconnect und keine automatische Wiederholung von Dateiaktionen.
Ein späterer expliziter Verbindungsaufbau muss weiterhin `remote.Connect` samt
aktueller FTPS-Trust-Policy verwenden. SSH-Hostkey- und Zertifikatsprüfungen bleiben
unverändert wirksam.

## Logging

- Probe-Fehler einmal pro betroffener Verbindung über `internal/log` protokollieren,
  mit Protokoll, Host, Endpunkt und Fehlerursache.
- Erfolgreiche Probes höchstens auf Debug-Level protokollieren.
- Keine Zugangsdaten, Dateiinhalte oder Zertifikats-Dumps loggen.
- Bei deaktiviertem Logging weiterhin keine Logdatei und keine Terminalausgabe.
- Doppelte Meldungen von Monitor und danach abbrechender Dateioperation vermeiden,
  ohne den individuellen Datei-/Sync-Fehler zu verschlucken.

## Umsetzungsschritte

### 1. Konfiguration und Formular

Optionales Intervall, Laufzeitauflösung, Validierung und Encoding ergänzen.
Hostformular samt Hilfetext und Fokusreihenfolge erweitern.

Abnahme: Fehlend, `0` und positive Werte bleiben über Laden, Bearbeiten und Speichern
unterscheidbar. Alte Konfigurationen verwenden den dokumentierten Standard.

### 2. Client-Lebenszyklus und FTP/FTPS

Transportabbruch, Fehlerstatus und Monitor implementieren. `NOOP` über dieselbe
Serialisierung wie Dateioperationen ausführen. Direkte zusätzliche FTP-Verbindungen
in Walkern berücksichtigen.

Abnahme: Leerlaufverbindungen senden Probes; belegte Verbindungen nicht. Timeouts
beenden die Verbindung ohne Deadlock oder zurückbleibende Goroutinen.

### 3. SSH/SFTP

SSH-Probes und begrenztes Warten ergänzen. Normales Schließen, negative Antworten,
Transportfehler und parallel laufende Transfers absichern.

Abnahme: SFTP arbeitet während SSH-Probes weiter. Ausbleibende Antworten führen
zu einem sichtbaren, endgültigen Verbindungsfehler.

### 4. Root-Routing, Browser und Diff

Verbindungsgebundene Statusnachrichten, Beobachter-Lebenszyklus und sichtbare
Fehlerzustände integrieren. Modals und Verbindungseigentümerwechsel berücksichtigen.

Abnahme: Alte Fehler treffen keine neue Session. Fehler führen weder zu automatischen
Transfers noch zum Verlust bereits bekannter Teilerfolge.

### 5. Dokumentation und Abnahme

`README.md`, `CHANGELOG.md` und bei geänderter Remote-API `AGENTS.md` aktualisieren.
Konfiguration, Standardintervall, Abschalten, Protokollunterschiede und Grenzen
beschreiben. Erst nach erfolgreicher Validierung einen Feature-PR anbieten.

## Testmatrix

Keine gemockten Remote-Clients. Echte Loopback-Server mit steuerbarem Verhalten
verwenden. Kurze interne Testintervalle erlauben, ohne zusätzliche Nutzeroptionen
nur für Tests einzuführen. Serverereignisse synchronisieren statt langer Sleeps.
Externe Server bleiben optional; Kerntests benötigen keine Zugangsdaten.

| Bereich | Pflichtfälle |
| --- | --- |
| Konfiguration | Fehlend, `0`, positive Werte, ungültige Werte/Typen, Roundtrip, globale und projektbezogene Hosts, isoliertes `XDG_CONFIG_HOME` |
| Formular | Standard, Abschalten, Validierungsfehler, Protokollwechsel, Fokus und Speichern |
| FTP | `NOOP` nach Leerlauf, kein `NOOP` bei deaktiviertem Intervall, korrekte Antwortzuordnung, EOF, negative Antwort, Timeout |
| FTP-Parallelität | Kein `NOOP` während Streaming-Read/Close, Upload, Rename oder Listing; keine Probe-Warteschlange; keine Deadlocks beim Schließen |
| FTPS | Probe über TLS-Steuerkanal, kein zusätzlicher Datenkanal, Zertifikatsprüfung weiterhin aktiv, Trust-Reset beendet Monitor |
| SFTP | Positive und negative SSH-Antwort, keine Antwort, Verbindungsabbruch, paralleler Transfer, deaktiviertes Intervall |
| Lebenszyklus | Fehlgeschlagener Aufbau, Ende des Connect-Contexts nach Erfolg, mehrfaches Close, Close während Probe, kurzlebige Worker, keine Goroutine-Leaks |
| TUI | Sichtbarer Fehler im Leerlauf, veraltete Nachrichten, Host-/Projektwechsel, Browser/Diff-Übergabe, Modal geöffnet, normales Schließen ohne Fehlalarm |
| Sync | Abbruch ohne Retry, unsicherer Aktionsausgang sichtbar, Teilerfolge erhalten, neuer Vergleich vor erneutem Sync |
| Regression | FTP/SFTP/FTPS ohne Keep-alive unverändert, Abbruch weiterhin möglich, Logging bleibt optional |

Vor Build und Tests die geänderten Go-Dateien per LSP prüfen. Anschließend:

```sh
go test ./...
go test -race ./...
go vet ./...
go build ./...
```

Manuell mit lokalen Servern einen kurzen Idle-Timeout konfigurieren und vergleichen:
Keep-alive aktiviert, deaktiviert, Server beendet, Netzverbindung unterbrochen.
Browser und Diff länger offen lassen, parallel eine größere Datei übertragen,
während einer Probe Projekt wechseln und FTPS-Vertrauen zurücksetzen.

Nach erfolgreicher Prüfung zur lokalen Nutzung `make update` ausführen. Dies ist
Teil der späteren Implementierung, nicht der Erstellung dieses Plans.
