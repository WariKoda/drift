# Implementierungsplan: Remote-Dateien im File Browser bearbeiten

Status: geplant, noch nicht implementiert.
Gewählte Variante: eigener Editor-Screen.

## Ziel und Umfang

Im Remote-Pane ausgewählte kleine Textdateien sollen direkt aus dem File Browser
geöffnet, bearbeitet und sicher auf denselben Remote-Pfad zurückgeschrieben werden
können. Der Editor erhält einen eigenen Screen. Dadurch bleiben Browser-Navigation,
Preview und Texteingabe klar voneinander getrennt.

Zum ersten Lieferumfang gehören:

- Einstieg über eine reguläre Datei im aktiven Remote-Pane.
- Verlustfreies Laden kleiner unterstützter Textdateien.
- Mehrzeilige Bearbeitung in einem eigenen Screen.
- Speichern mit Konfliktprüfung gegen den seit dem Öffnen veränderten Remote-Inhalt.
- Explizites Verwerfen ungespeicherter Änderungen.
- Sichtbare Lade-, Speicher- und Uploadfehler ohne Verlust des Editor-Puffers.
- Aktualisierung von Remote-Liste und Preview nach erfolgreichem Speichern.
- Unterstützung für SFTP, FTP und FTPS über `remote.Client`.

Nicht Bestandteil des ersten Lieferumfangs sind:

- Binär- und Hex-Editor.
- Bearbeitung großer Dateien.
- Anlegen, Umbenennen oder Löschen von Dateien.
- Gleichzeitige Bearbeitung mehrerer Dateien oder Tabs.
- Syntaxhervorhebung, Language Server oder Formatierung.
- Automatisches Zusammenführen konkurrierender Änderungen.
- Garantierte Unterbrechung eines bereits an den Protokoll-Client übergebenen Uploads.

Die UI-Texte bleiben entsprechend der bestehenden Anwendung Englisch.

## Bestehende Ansatzpunkte

| Datei | Aktuelles Verhalten und geplante Nutzung |
| --- | --- |
| `internal/tui/browser/model.go` | Besitzt aktive Pane-Seite, Remote-Host, Verbindung, Einträge und Busy-Zustände. Von hier wird die Editieranforderung ausgelöst. |
| `internal/tui/browser/update.go` | `updateNormal` verarbeitet Tastenkürzel und sendet bereits typisierte Nachrichten an das Root-Modell. |
| `internal/tui/browser/preview.go` | Lädt lokale und Remote-Dateien asynchron, begrenzt die Größe und verwirft verspätete Ergebnisse per Generation. Die sanitisierten Preview-Zeilen dürfen nicht als Editor-Puffer verwendet werden. |
| `internal/tui/browser/view.go` | Rendert Remote-Baum und Preview. Nach Rückkehr müssen Dateimetadaten und eine gegebenenfalls aktive Preview gezielt aktualisiert werden. |
| `internal/tui/app.go` | Root-Routing, Screen-Wechsel, globale Netzwerkaktivität, Abbruch und sichtbare Fehler. Hier wird der Editor-Screen eingebunden. |
| `internal/tui/state.go` | Enthält die Screen-Konstanten. `ScreenRemoteEditor` ergänzen. |
| `internal/remote/client.go` | `Open`, `ReadFile`, `Stat` und `Upload` reichen für Laden, Konfliktprüfung und Speichern aus. Kein Protokoll-Client wird direkt aus der TUI erzeugt. |
| `internal/sftp/client.go` | `Upload` schreibt eine Staging-Datei, erhält vorhandene Berechtigungsbits und ersetzt anschließend das Ziel. Nichtreguläre Ziele werden abgelehnt. |
| `internal/ftp/client.go` | `Upload` schreibt ebenfalls über eine Staging-Datei und Rename. FTP-Operationen auf einer Verbindung sind serialisiert. |
| `internal/tui/loading/model.go` | Gemeinsamer Tracker und Loader für asynchrone Netzwerkoperationen. Abbruch ist kooperativ und beendet nicht zwingend einen bereits laufenden `Upload`. |
| `internal/tui/textfield/textfield.go` | Nur einzeilig und daher nicht als Editor geeignet. |
| `internal/styles/styles.go` | Alle benötigten Editor- und Dialog-Styles zentral ergänzen. Keine Lipgloss-Styles direkt in View-Code definieren. |

Aktuelle Tastenbelegung: `p` schaltet die Preview, `P` öffnet den Projektwechsel.
Beide Belegungen bleiben unverändert. Als Editor-Einstieg ist `e` vorgesehen.

## Sicherheits- und Datenregeln

### Originalinhalt getrennt von der Darstellung halten

`preview.lines` ist keine zulässige Datenquelle für den Editor. Die Preview ersetzt
ungültiges UTF-8, normalisiert Zeilenenden, erweitert Tabs und entfernt Steuerzeichen.
Der Editor muss die geladenen Originalbytes separat halten und nur eine sichere
Darstellung rendern.

Die Editor-Sitzung speichert mindestens:

- Host-Snapshot und Remote-Pfad.
- Originalbytes oder deren kryptografischen Hash plus die für Konfliktprüfung
  benötigten Originalbytes innerhalb des Größenlimits.
- Editierbaren Textpuffer.
- ursprünglichen Zeilenendentyp und Zustand des abschließenden Zeilenumbruchs.
- Dirty-Zustand.
- eindeutige Sitzungs- und Operations-ID.
- aktuellen Lade-, Konflikt-, Speicher- und Fehlerzustand.

### Unterstützte Dateien

Der erste Ausbau akzeptiert nur reguläre UTF-8-Textdateien ohne NUL-Bytes.
Symlinks, Verzeichnisse und sonstige Dateitypen werden abgelehnt. Ungültiges UTF-8
wird nicht still ersetzt.

Vor Implementierung ist ein festes Größenlimit zu wählen. Empfehlung für den ersten
Ausbau: 256 KiB. Das Limit gilt beim Laden sowie nach Eingabe oder Paste vor dem
Speichern. Die vorhandene Preview-Grenze von 1 MiB ist davon unabhängig.

LF und CRLF sollen erhalten bleiben. Gemischte Zeilenenden werden im ersten Ausbau
abgelehnt oder nur nach ausdrücklicher Bestätigung normalisiert. Die genaue Regel ist
vor dem ersten Code-Schritt in Tests festzuschreiben.

### Schreibbereich

Empfehlung: Remote-Bearbeitung nur für Pfade erlauben, die durch die effektiven
Host- oder Projekt-Mappings abgedeckt sind. Ohne konfigurierte Mappings gilt wie beim
Sync der Bereich unter `Host.RootPath`.

Die Prüfung verwendet `internal/pathmap` und erfolgt beim Öffnen sowie erneut vor dem
Speichern. Damit erweitert der Editor die vorhandenen Sync-Schreibrechte nicht
unbemerkt auf jeden vom Remote-Browser erreichbaren Pfad.

### Konfliktprüfung

Vor jedem Upload wird der Remote-Inhalt erneut unter demselben Größenlimit geladen
und bytegenau mit dem beim Öffnen gespeicherten Original verglichen.

- Unverändert: Upload darf fortfahren.
- Verändert: nicht automatisch überschreiben.
- Gelöscht: als Konflikt behandeln, nicht still neu anlegen.
- Kein reguläres Ziel mehr: Speichern ablehnen.
- Prüfung fehlgeschlagen: Puffer erhalten und Fehler anzeigen.

Der Konfliktdialog bietet:

1. `Continue editing`
2. `Reload remote`, nur nach Bestätigung zum Verwerfen lokaler Änderungen
3. `Overwrite`, mit ausdrücklicher zweiter Bestätigung

Zwischen Konfliktprüfung und Rename bleibt mit dem vorhandenen `remote.Client` ein
Race-Fenster. Die Schnittstelle bietet kein bedingtes Schreiben. Diese Grenze muss in
Dokumentation und Fehlermeldungen ehrlich benannt werden. Strikte Compare-and-Swap-
Semantik wäre ein getrenntes Folgefeature und müsste für alle Protokolle entworfen
werden.

### Upload und unklarer Ausgang

`remote.Client.Upload` verwendet bereits Staging und Rename. Ein Fehler schützt in
vielen Fällen die bisherige Zieldatei. Wenn die Verbindung nach einem möglicherweise
erfolgreichen Rename abbricht, ist der Ausgang für den Client jedoch unklar.

In diesem Fall:

- Editor-Puffer nicht verwerfen.
- Zustand `Save result unknown` anzeigen.
- Remote-Inhalt vor einem weiteren Upload erneut lesen.
- Übereinstimmung mit dem Editor-Puffer als Erfolg behandeln.
- Abweichung als Konflikt behandeln.
- Fehler mit Host, Pfad und Operation über `internal/log` protokollieren, niemals
  Dateiinhalt oder Zugangsdaten.

## Bedienung

### Einstieg aus dem Browser

`e` öffnet den Editor nur, wenn:

- das Remote-Pane aktiv ist,
- eine reguläre Remote-Datei ausgewählt ist,
- ein Host verbunden ist,
- keine Remote-Operation läuft,
- der Pfad nach der Schreibbereichsregel zulässig ist.

In allen anderen Fällen bleibt der Browser aktiv und zeigt eine konkrete Statuszeile.
Die Editieranforderung enthält Kopien von Host und Pfad, nicht nur einen Zeiger auf den
beweglichen Browser-Cursor.

### Editor-Screen

Vorgeschlagene Darstellung:

```text
Edit remote file                                      modified
host: staging                 /var/www/app/config.php
────────────────────────────────────────────────────────────────
  1 │ <?php
  2 │ return [
  3 │     'debug' => true,
  4 │ ];
    │
────────────────────────────────────────────────────────────────
[Ctrl+S] save  [Esc] back  [Ctrl+G] help
```

Regeln:

- Normale Buchstaben einschließlich `q`, `p`, `P`, `s` und `e` schreiben Text.
- `Ctrl+S` startet Konfliktprüfung und Speichern.
- `Esc` kehrt bei unverändertem Puffer direkt zurück.
- `Esc` bei Änderungen öffnet eine Auswahl `Continue editing`, `Discard`, `Save`.
- Während Laden, Prüfen und Speichern sind Textänderungen und weitere Speichervorgänge
  gesperrt.
- Ein Fehler bleibt sichtbar, bis eine neue Aktion ihn ersetzt. Der Puffer bleibt
  bearbeitbar.
- Terminal-Resize erhält Cursor und Scrollposition.
- Mausunterstützung ist für den ersten Ausbau optional. Falls das eingesetzte Widget
  Mausereignisse unterstützt, darf es das bestehende Browser-Mausverhalten nicht
  unbeabsichtigt übernehmen.

Vor Auswahl einer Textarea-Abhängigkeit ist ein kleiner Prototyp nötig. Zu prüfen sind
Bubble Tea 1.3.10, Unicode, breite Zeichen, Tabs, mehrzeilige Paste, Undo, horizontales
Scrollen, Größenlimit und programmatisches Setzen ohne Inhaltstransformation. Das
bestehende `textfield` wird nicht erweitert.

## Architektur und Datenfluss

### Neues Package

Neues Screen-Package:

```text
internal/tui/remoteeditor/
  model.go
  update.go
  view.go
  content.go
  model_test.go
  update_test.go
  content_test.go
```

`content.go` enthält nur konkret benötigte Regeln für UTF-8, Größe, Zeilenenden,
Originalvergleich und Serialisierung. Keine allgemeine Dokument- oder Editor-
Abstraktion anlegen.

Vorgeschlagene Nachrichten:

```go
type MsgLoaded struct {
    SessionID uint64
    Content   []byte
    Info      os.FileInfo
    Err       error
}

type MsgSaved struct {
    SessionID   uint64
    OperationID uint64
    RemoteInfo  os.FileInfo
    Err         error
    Unknown     bool
}

type MsgBackToBrowser struct {
    Host config.Host
    Path string
    Saved bool
}
```

Konflikte können als eigene typisierte Nachricht oder als expliziter Zustand nach
einem Prüfergebnis modelliert werden. Nicht über Fehlertext entscheiden.

### Verbindungseigentum

Für den ersten Ausbau öffnet jede Editor-Sitzung eine eigene Verbindung über
`remote.Connect` und schließt sie beim endgültigen Verlassen. Das kostet einen
zusätzlichen Verbindungsaufbau, trennt aber Lebensdauer und Fehlerzustand sauber von
der Browser-Verbindung.

Die Verbindung entsteht in einem `tea.Cmd`, nicht in `Update`. Falls FTPS-Vertrauen
oder andere Laufzeit-Verbindungsoptionen bis dahin die Signatur von `remote.Connect`
ändern, übernimmt der Editor exakt denselben Factory-Aufruf wie Browser und Diffview.

Ein späteres Übergeben der Browser-Verbindung ist nur sinnvoll, wenn Eigentum,
Rückgabe bei Fehlern und FTP-Serialisierung ausdrücklich gelöst werden. Für den ersten
Ausbau wird diese Optimierung nicht vorgenommen.

### Ablauf

1. Browser validiert Auswahl und sendet `MsgEditRequested`.
2. `App.Update` erzeugt `remoteeditor.Model`, setzt Größe und wechselt auf
   `ScreenRemoteEditor`.
3. `remoteeditor.Init` startet Verbindung, `Stat` und begrenztes `Open` als
   `tea.Cmd`. Reader werden immer geschlossen; Read- und Close-Fehler werden
   gemeinsam ausgewertet.
4. Ein aktuelles `MsgLoaded` initialisiert Original und Textpuffer. Veraltete
   Sitzungsnachrichten räumen Ressourcen auf und ändern keinen sichtbaren Zustand.
5. `Ctrl+S` startet erneutes `Stat` und begrenztes Lesen.
6. Bei unverändertem Original serialisiert der Editor den Puffer verlustfrei und
   ruft `Upload(path, reader)` in einem `tea.Cmd` auf.
7. Nach Upload liest der Command `Stat` und bei unklarem Ergebnis gegebenenfalls
   den Inhalt zurück.
8. Bei Erfolg wird der gespeicherte Inhalt zur neuen Originalbasis und der Dirty-
   Zustand gelöscht.
9. Beim Verlassen schließt der Editor seine Verbindung asynchron und sendet
   `MsgBackToBrowser`.
10. Der Browser aktualisiert den betroffenen Verzeichniseintrag und lädt eine aktive
    Preview für denselben Pfad neu, ohne Remote-Cursor, Expanded-State und Scrollposition
    pauschal zurückzusetzen.

### Root-Routing und Loader

In `internal/tui/app.go` ergänzen:

- `remoteEditor remoteeditor.Model` im Root-Modell.
- `ScreenRemoteEditor` in `internal/tui/state.go`.
- Größe, `Update`, `View` und Rückkehrnachrichten routen.
- neue globale Netzwerkaktivität oder eine eindeutig zuordenbare Editor-Aktivität.
- Loader-Abbruch nur dann als erfolgreicher Abbruch anzeigen, wenn die Operation noch
  nicht an einen nicht abbrechbaren Upload übergeben wurde.

`q` darf im Editor nicht durch das globale Browser-Quit-Verhalten abgefangen werden.
Der Editor besitzt seine eigene Rückkehrlogik. `Ctrl+C` während Texteingabe und
Netzwerkaktivität muss vor Implementierung ausdrücklich festgelegt und getestet
werden.

## Geplante Änderungen pro Bereich

### Browser

Betroffene Dateien:

- `internal/tui/browser/keys.go`
- `internal/tui/browser/model.go`
- `internal/tui/browser/update.go`
- `internal/tui/browser/remote.go`
- `internal/tui/browser/view.go`
- zugehörige Tests

Änderungen:

- `e` und Hilfetext ergänzen.
- `MsgEditRequested` mit Host- und Pfad-Snapshot ergänzen.
- Editor-Einstieg nur für zulässige reguläre Remote-Dateien.
- Remote-Busy-Erkennung um Editor-Übergang sauber abgrenzen.
- gezielte Aktualisierung eines Remote-Eintrags nach Rückkehr implementieren.
- aktive Preview nach erfolgreichem Speichern neu laden.

### Editor-Screen

Betroffene Dateien:

- `internal/tui/remoteeditor/model.go` neu
- `internal/tui/remoteeditor/update.go` neu
- `internal/tui/remoteeditor/view.go` neu
- `internal/tui/remoteeditor/content.go` neu
- Tests im selben Package

Änderungen:

- Textarea einbinden oder eng begrenztes eigenes Widget implementieren.
- Sitzungs- und Operations-IDs verwalten.
- Dirty-, Bestätigungs-, Konflikt- und Fehlerzustände modellieren.
- Commands für Laden, Prüfen, Speichern, Rücklesen und Schließen implementieren.
- Größen-, Text- und Zeilenendenregeln zentral anwenden.

### Root und Styles

Betroffene Dateien:

- `internal/tui/app.go`
- `internal/tui/state.go`
- `internal/styles/styles.go`
- optional `internal/tui/styles.go`
- `internal/tui/app_test.go`

Änderungen:

- Screen-Lebenszyklus und typisierte Übergänge ergänzen.
- Netzwerkaktivität und Fehleranzeige integrieren.
- zentrale Styles für Editor-Cursor, Dirty-Zustand, Bestätigungen und Konflikte
  hinzufügen, soweit vorhandene Styles nicht reichen.

### Remote-Schicht

Für den Grundablauf ist keine Erweiterung von `remote.Client` vorgesehen. Der Editor
verwendet `Stat`, `Open` und `Upload`.

Vor Freigabe muss die FTP-Testabdeckung für Upload, Staging, Rename-Fehler und
unklaren Verbindungsausgang ergänzt werden. Falls sich dabei zeigt, dass vorhandene
Protokollmethoden eine sichere Zustandsbestimmung verhindern, wird eine kleine
protokollunabhängige API-Änderung separat begründet. Kein `WriteFile`-Alias und kein
Editor-spezifischer Client werden vorsorglich eingeführt.

## Umsetzungsschritte

### 1. Inhaltsregeln und Widget-Prototyp

- Größenlimit und Verhalten bei gemischten Zeilenenden festlegen.
- Verlustfreie Parser-/Serializer-Tests schreiben.
- Textarea-Kandidat gegen Bubble Tea 1.3.10 und die benötigten Eingaben prüfen.
- Entscheidung zur Abhängigkeit dokumentieren.

Abnahme: Laden und unverändertes Serialisieren ergibt bytegleich den Originalinhalt.
Nicht unterstützte Inhalte werden vor dem Editorstart verständlich abgelehnt.

### 2. Editor-Zustandsmaschine ohne Netzwerk

- `remoteeditor.Model`, Update und View anlegen.
- Eingabe, Dirty-State, Resize sowie Speichern-/Verwerfen-Dialog implementieren.
- Sitzungs- und Operations-IDs samt veralteten Ergebnissen testen.

Abnahme: Alle Editorzustände sind mit synthetischen Bubble-Tea-Nachrichten testbar;
keine View- oder Update-Methode blockiert.

### 3. Laden und Konfliktprüfung

- Verbindung und begrenztes Remote-Lesen in `tea.Cmd` integrieren.
- Dateityp, Schreibbereich und Inhalt prüfen.
- bytegenaue Konflikterkennung und Konfliktdialog umsetzen.
- Fehler sichtbar machen und protokollieren.

Abnahme: Zwischenzeitliche Änderungen oder Löschungen werden nie still überschrieben.

### 4. Sicheres Speichern

- `Upload` asynchron aufrufen.
- eindeutigen Erfolg, Fehler und unklaren Ausgang unterscheiden.
- Rücklesen nach unklarem Ergebnis implementieren.
- Puffer bei jedem Fehler erhalten.
- SFTP- und FTP/FTPS-Transporttests ergänzen.

Abnahme: Ein fehlgeschlagener Upload verliert weder Editorinhalt noch bestätigt er
fälschlich Erfolg. Staging-Dateien bleiben nach erwartbaren Fehlern nicht zurück.

### 5. Browser- und Root-Integration

- `e`, Help, Screen-Konstante und Root-Routing ergänzen.
- Editor-Verbindung sicher schließen.
- Browser-Eintrag und Preview nach Erfolg gezielt aktualisieren.
- Cursor, Scrollposition, Expanded-State und Markierungen erhalten.
- Projekt-, Host- und Screen-Wechsel während laufender Ergebnisse absichern.

Abnahme: Der vollständige Ablauf funktioniert ohne Neuaufbau der Browser-Sitzung und
ohne dass verspätete Nachrichten eine andere Datei oder ein anderes Projekt ändern.

### 6. Dokumentation und Gesamtprüfung

- `README.md`, `CHANGELOG.md` und bei geänderten Architekturregeln `AGENTS.md`
  aktualisieren.
- Grenzen von Konfliktprüfung, Upload-Abbruch, Dateiformaten und Größenlimit nennen.
- Hilfe und Fehlermeldungen mit SFTP, FTP und FTPS manuell prüfen.

## Test- und Abnahmematrix

Keine gemockten Remote-Clients. Reine Inhalts- und TUI-Zustände benötigen kein
Remote-I/O. Transportverhalten wird mit echten Loopback-Protokollverbindungen oder
optional übersprungenen externen Testservern geprüft.

| Bereich | Pflichtfälle |
| --- | --- |
| Inhalt | Leere Datei, LF, CRLF, fehlender Schlussumbruch, Tabs, Unicode und breite Zeichen, maximale Größe, Überschreitung nach Paste, NUL, ungültiges UTF-8, gemischte Zeilenenden. |
| Editor | Einfügen, Löschen, Navigation, mehrzeilige Paste, Undo falls zugesagt, Resize, horizontales/vertikales Scrollen, Dirty-State. |
| Tastatur | `q`, `p`, `P`, `s` und `e` schreiben Text; `Ctrl+S`, `Esc`, Bestätigungsdialoge und Hilfe funktionieren kontextabhängig. |
| Einstieg | Nur aktive reguläre Remote-Datei; kein Host, lokales Pane, Verzeichnis, Symlink, Busy-State und nicht erlaubtes Mapping werden verständlich behandelt. |
| Asynchronität | Veraltete Load-/Save-Nachrichten, Doppelspeichern, Rückkehr vor spätem Ergebnis, Projekt-/Hostwechsel, Fehler beim Reader-Close. |
| Konflikt | Inhalt verändert, gleiche Größe mit anderem Inhalt, Datei gelöscht, Zieltyp geändert, Reload mit Dirty-Puffer, ausdrückliches Overwrite. |
| Upload | Erfolg, Quell-Lesefehler, Rechtefehler, Verbindungsabbruch, Rename-Fehler, unklarer Ausgang, Rücklesen, keine verbliebene Staging-Datei. |
| Browser-Rückkehr | Cursor, Scrollposition, Expanded-State und Markierungen bleiben; Größe und Änderungszeit werden aktualisiert; Preview zeigt den neuen Inhalt. |
| Protokolle | SFTP, FTP und FTPS; vorhandene Berechtigungsbits bei SFTP; dokumentiertes FTP-Metadatenverhalten. |
| Logging | Fehler mit Operation, Host und Pfad; keine Zugangsdaten oder Dateiinhalte; bei deaktiviertem Logging keine Datei und keine Terminalausgabe. |

Nach der späteren Implementierung:

```sh
go test ./...
go test -race ./...
go vet ./...
go build ./...
```

Zusätzlich manuell mit Testhosts für SFTP, FTP und FTPS prüfen:

1. Datei öffnen, minimal ändern und speichern.
2. Datei parallel außerhalb von drift ändern und Konflikt erkennen.
3. Verbindung vor sowie während des Uploads trennen.
4. Schreibrechte entziehen und den Puffer nach dem Fehler weiterbearbeiten.
5. Kleine Terminalgröße, Resize und große Paste testen.
6. Sicherstellen, dass keine Datei im lokalen Projekt erzeugt oder geändert wird.

## Vor Implementierungsbeginn zu entscheiden

1. Exaktes Größenlimit. Empfehlung: 256 KiB.
2. Verhalten bei gemischten Zeilenenden. Empfehlung: im ersten Ausbau ablehnen.
3. Textarea-Bibliothek oder eigenes eng begrenztes Widget nach Prototyp.
4. `Ctrl+C` im Editor: Anwendung beenden, Editor abbrechen oder nur laufende
   Netzwerkaktivität abbrechen.
5. Ob `Overwrite` im Konfliktfall bereits im ersten Lieferumfang enthalten ist.
   Sicherheitsorientierte Alternative: nur weiterbearbeiten oder Remote neu laden.

Diese Entscheidungen ändern nicht die grundsätzliche Machbarkeit, müssen aber vor
Implementierung in Akzeptanztests festgeschrieben werden.
