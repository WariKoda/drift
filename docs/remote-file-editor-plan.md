# Implementierungsplan: Remote-Dateien im externen Editor bearbeiten

Status: im Feature-Branch implementiert; manuelle Terminal-/GUI-Abnahme noch offen.
Dieser Plan ersetzt den früheren Entwurf eines eingebauten Editors und beschreibt
die umgesetzten Regeln.
Geltungsbereich: Go-TUI; kein neues Editor-Feature für die pausierte Rust-GUI.

Automatisierte Prüfungen verwenden echte SFTP-, FTP- und FTPS-Verbindungen sowie
einen echten Editor-Hilfsprozess. Die Bedienung mit Nano/Neovim, Zed und VS Code
ist zusätzlich nach `docs/manual-testing.md` zu prüfen.

## Festgelegter Ablauf

- Eine Datei im Remote-Pane lässt sich in einem vorkonfigurierten externen Editor öffnen.
- Terminal- und GUI-Editoren werden unterstützt.
- Erst nach dem Ende der Editor-Bearbeitung bietet drift den Upload an.
- Jeder Upload braucht eine ausdrückliche Bestätigung. Speichern im Editor allein
  verändert den Host nicht.
- Die Arbeitskopie liegt außerhalb des lokalen Projekts. Eine gleichnamige lokale
  Projektdatei wird weder überschrieben noch als Ausgangsbasis verwendet.
- Es wird kein eigener Texteditor und kein mehrzeiliges Eingabewidget gebaut.

Beispiel:

```text
Remote-Datei auswählen → e → herunterladen → Editor öffnen
    → Editor-Bearbeitung beenden → Änderungen erkennen
    → Upload bestätigen → Remote-Inhalt prüfen → hochladen
    → zum unveränderten Browser-Kontext zurückkehren
```

Die UI-Texte bleiben entsprechend der bestehenden Anwendung Englisch.

## Umfang der ersten Version

Enthalten sind eine reguläre Datei und eine Editiersitzung zurzeit, SFTP, FTP und
FTPS, eine globale Editor-Konfiguration, Konflikterkennung und der Erhalt der
Arbeitskopie bei Fehlern. Ein Host muss bereits im Remote-Browser verbunden sein.

Nicht enthalten sind:

- automatischer Upload bei jedem Speichern oder Dateiüberwachung;
- gleichzeitige Editiersitzungen, Editor-Auswahl je Dateityp oder Host;
- Anlegen, Umbenennen oder Löschen von Remote-Dateien;
- Remote-Shell, auf dem Server ausgeführte Editoren oder editorabhängige Remote-URLs;
- eingebauter Editor, Syntaxhervorhebung oder Language Server;
- automatisches Zusammenführen oder erzwungenes Überschreiben bei Konflikten;
- automatische Wiederaufnahme liegen gebliebener Sitzungen nach einem Neustart;
- garantierte sofortige Unterbrechung eines laufenden Protokollaufrufs.

## Bedienung

### Einstieg

`e` startet die Bearbeitung der Datei unter dem Cursor im aktiven Remote-Pane.
Markierungen spielen dabei keine Rolle. `Enter`, `p`, `P` und `s` behalten ihre
bisherige Bedeutung.

Voraussetzungen:

- reguläre Remote-Datei, kein Verzeichnis und kein bekannter Symlink;
- aktive, nicht fehlgeschlagene Verbindung;
- keine laufende Remote- oder globale Netzwerkoperation;
- zulässiger Schreibbereich gemäß den effektiven Mappings;
- gültige Editor-Konfiguration.

Bei einer nicht erfüllten Voraussetzung bleibt der Browser aktiv und zeigt den
konkreten Grund. `e` im lokalen Pane bearbeitet in dieser Version keine Datei.
Preview und Dateityp werden nicht als Inhaltsquelle verwendet: drift lädt die
Originalbytes neu über die Remote-Verbindung.

### Während der Bearbeitung

Bei einem Terminal-Editor übergibt drift das Terminal über `tea.ExecProcess`.
Nach Ende des Prozesses müssen Alt-Screen, Eingabemodus, Mauspräferenz und die
aktuelle Terminalgröße wieder stimmen, auch nach einem Prozessfehler.

Bei einem GUI-Editor bleibt drift sichtbar und wartet in einem eigenen
Sitzungs-Screen. Der Editor läuft in einem `tea.Cmd`; seine Ausgaben gehen nicht
ungefiltert in das von Bubble Tea verwendete Terminal. Der Screen zeigt Host,
Remote-Pfad und den lokalen Arbeitsdateipfad.

Beide Varianten erlauben währenddessen keinen Projekt-, Host- oder Sync-Wechsel.
Ein GUI-Prozess wird bei einem Rückkehrversuch nicht still beendet. Die UI fordert
zum Beenden der Bearbeitung im Editor auf. Ein erzwungenes Beenden von drift darf
keinen automatischen Upload und kein Löschen der Arbeitskopie auslösen.

Die konfigurierte Warteoption ist Voraussetzung: `code --wait` muss warten, bis
die Datei im Editor geschlossen wurde. Ein nur kurz laufender Launcher ohne
Warteoption kann nicht zuverlässig als Fehlkonfiguration erkannt werden.

### Nach dem Editor-Ende

- Erfolgreicher Prozess, Datei bytegleich: kein Upload und Rückkehr zum Browser.
- Erfolgreicher Prozess, Datei geändert: Bestätigungsdialog mit Host und Remote-Pfad.
- Prozessfehler, fehlende oder ungültige Arbeitsdatei: kein Upload; Fehler und
  erhaltenen Arbeitsdateipfad anzeigen.

Der Bestätigungsdialog bietet `Upload`, `Edit again`, `Keep local copy` und
`Discard`. `Upload` ist nicht vorausgewählt. `Esc` bedeutet nicht Zustimmung und
verwirft keine Änderungen. `Discard` benötigt eine eigene Bestätigung.

`Keep local copy` beendet die Sitzung ohne Remote-Änderung, erhält die Datei und
zeigt ihren Pfad an. Bei `Edit again` wird dieselbe Arbeitskopie geöffnet; die
ursprüngliche Remote-Vergleichsbasis bleibt unverändert.

Nach erfolgreichem Upload werden betroffener Remote-Eintrag und eine zugehörige
aktive Preview aktualisiert. Cursor, Scrollposition, aufgeklappte Verzeichnisse,
Markierungen und lokale Dateien bleiben erhalten.

## Editor-Konfiguration

Geplant ist ein optionaler Abschnitt in der globalen Konfiguration
`<config.Dir()>/config.toml`. Keine Editor-Befehle in Projekt- oder Host-Datensätzen:
Die Freigabe eines Projekts oder Servers darf kein lokales Programm konfigurieren.

Terminal-Beispiel:

```toml
[editor]
command = ["nvim", "{file}"]
terminal = true
```

GUI-Alternative, statt des obigen Abschnitts:

```toml
[editor]
command = ["code", "--wait", "{file}"]
terminal = false
```

Regeln:

- Genau ein konfigurierter Editor. `terminal` hat bei fehlender Angabe den Wert `true`.
- `command` ist eine Argumentliste mit einem nicht leeren Programmnamen und genau
  einem eigenständigen `{file}`-Argument. Andere Platzhalter sind nicht vorgesehen.
- `{file}` wird durch den absoluten lokalen Arbeitsdateipfad ersetzt. Host,
  Remote-Pfad und Dateiinhalte werden nie in einen Shell-Ausdruck eingesetzt.
- Aufruf über `os/exec`, ohne `sh -c`, Shell-Expansion oder implizite Ausführung
  einer Projektkonfiguration. Leerzeichen und Sonderzeichen bleiben Argumentinhalt.
- Arbeitsverzeichnis ist das private Sitzungsverzeichnis, nicht das Projekt.
- Kein hartcodierter Default-Editor. Ohne Konfiguration zeigt `e` eine Anleitung.
- Kein implizites Parsen von `$VISUAL` oder `$EDITOR` in der ersten Version.
- Einrichtung zunächst über TOML; ein Einstellungsdialog ist kein Teil dieses Plans.

Die Go-Typen gehören nach `internal/config/config.go`. Loader, Laufzeitkonfiguration
und Writer müssen den Abschnitt erhalten, auch bei Host-Verwaltungsoperationen und
beim Schreiben einer vorher nicht vorhandenen globalen Datei. Ohne Abschnitt bleibt
das bisherige Verhalten unverändert.

### Gemeinsame Konfiguration mit Rust

Go und Rust schreiben dieselbe globale Datei. Rusts gespeicherter `GlobalConfig`
erhält jetzt ebenfalls den Editor-Abschnitt; ein Roundtrip-Test prüft Host-Save und
-Delete. Ohne diese koordinierte Schemaänderung würde ein typisierter
Lese-/Schreibzyklus den Abschnitt verlieren. Das ist reine Store-Kompatibilität,
keine Wiederaufnahme der GUI-Featureentwicklung. Alte Binaries, die den Abschnitt
nicht erhalten, werden dadurch nicht nachträglich kompatibel.

## Arbeitskopie und Inhalt

Jede Sitzung erhält ein zufällig benanntes, privates Arbeitsverzeichnis unter
`<os.UserCacheDir()>/drift/remote-edit/`. Es ist ein temporärer Arbeitsbereich im
Benutzer-Cache, nicht im Projekt. Anders als ein automatisch aufgeräumtes
Prozess-Tempverzeichnis darf er bei Fehlern oder ungespeicherten Änderungen
nicht bedingungslos entfernt werden.

- Sitzungsverzeichnis Modus `0700`, Arbeits- und Snapshot-Dateien Modus `0600`.
- Ursprünglichen Dateinamen einschließlich Erweiterung für die Editor-Erkennung
  erhalten; unzulässige Basenames vor der lokalen Erstellung ablehnen.
- Eigene `fs.Root` über diesem Verzeichnis öffnen. Transfers und erneutes Lesen
  bleiben darauf beschränkt; die Projekt-Root wird nicht erweitert.
- Download mit `remote.Client.Open` und `fs.Root.WriteAtomic`; Upload aus
  `fs.Root.Open` über den bestehenden Sync-Pfad. Reader- und Close-Fehler auswerten.
- Typ und Größe vor und nach der Bearbeitung prüfen. Symlink-Escapes, Geräte,
  Verzeichnisse und andere nichtreguläre lokale Dateien ablehnen.
- Planvorgabe für den ersten Ausbau: höchstens 10 MiB pro Datei. Beim Download
  begrenzt lesen und eine Überschreitung als Fehler behandeln, nicht als gekürzte
  Datei. Dasselbe Limit gilt für bearbeiteten Inhalt und erneute Remote-Prüfung.
- Originalinhalt beziehungsweise SHA-256-Vergleichsbasis unabhängig von der
  veränderlichen Arbeitsdatei halten.
- Nach Editor-Ende einen unveränderlichen Upload-Snapshot erzeugen. Zustimmung und
  Upload beziehen sich auf diese Bytes, nicht auf eine später erneut geöffnete,
  möglicherweise weiter veränderte Editor-Datei.

Die Berechtigungen sind nach dem Download und nach einem atomaren Editor-Save zu
prüfen beziehungsweise über den geöffneten Dateihandle wieder einzuschränken.
Das private Elternverzeichnis schützt auch während eines editorseitigen Austauschs.
Der externe Editor ist ein vom Benutzer gewähltes Programm, keine Sandbox:
Plugins und editorseitige Backups liegen außerhalb der Kontrolle von drift.

Drift normalisiert keine Kodierung, Zeilenenden, Tabs, BOMs oder Schlussumbrüche.
Es gibt keine eigene UTF-8-Textarea-Beschränkung. Welche Formate bearbeitbar sind,
bestimmt der konfigurierte Editor. Vergleiche beziehen sich auf Originalbytes,
nicht auf die sanitisierten Zeilen der Preview.

Aufräumen nur nach bestätigtem Verwerfen, bestätigtem Upload-Erfolg oder einem
unveränderten Abschluss. Vor dem Aufräumen nach Upload-Erfolg erneut prüfen, ob
die Arbeitsdatei noch dem bestätigten Snapshot entspricht. Spätere Änderungen
bleiben erhalten und brauchen eine neue Entscheidung; sie sind weder übertragen
noch zum Löschen freigegeben. Fehler beim Aufräumen bleiben sichtbar und werden
protokolliert. Bei Prozess-, Lese-, Verbindungs- und Uploadfehlern bleibt eine
bereits vorhandene Arbeitskopie erhalten. Ein globales `defer os.RemoveAll` wäre
dafür falsch. Eine vollständige Cache-/Recovery-Verwaltung wird nicht vorsorglich
eingeführt.

## Schreibbereich und Konflikte

### Schreibbereich

Öffnen und Speichern prüfen den Remote-Pfad mit `internal/pathmap` gegen die
effektiven Host-Mappings beziehungsweise Projekt-Fallback-Mappings. Ohne Mappings
gilt der Bereich unter `Host.RootPath`. Sichtbare, aber ungemappte Remote-Dateien
werden dadurch nicht editierbar.

Hard Exclusions bleiben gesperrt. Ein direkt gewählter, bewusst sichtbar gemachter
gitignorierter Dateipfad folgt derselben Ausnahme wie eine direkt gewählte Sync-Datei.
Das Feature erweitert weder Mapping-Grenzen noch Ignore-Regeln.

Der Remote-Dateityp wird beim Öffnen und vor dem Upload neu geprüft, bei Bedarf
über die Elternverzeichnis-Liste. `remote.Client.Stat` allein genügt nicht als
Symlink-Prüfung: SFTP-`Stat` folgt Links. Bekannte Symlinks und nichtreguläre Ziele
werden abgelehnt. Die vorhandene API bietet aber keine allgemeine serverseitige
Pfad-Confinement-Garantie für zwischenzeitlich ausgetauschte Pfadkomponenten.

### Prüfung vor dem Upload

Nach ausdrücklicher Upload-Zustimmung lädt drift den Remote-Inhalt erneut begrenzt
und vergleicht ihn mit der beim Öffnen gesicherten Originalbasis. Größe und
Änderungszeit allein reichen dafür nicht.

| Beobachtung | Verhalten |
| --- | --- |
| Remote-Inhalt entspricht der Originalbasis | Bestätigten Snapshot hochladen. |
| Remote-Inhalt entspricht bereits dem Upload-Snapshot | Keine erneute Übertragung; gewünschter Inhalt ist bestätigt. |
| Remote-Inhalt anderweitig verändert | Konflikt, kein Upload. |
| Datei erkennbar gelöscht oder Zieltyp geändert | Kein Upload und kein stilles Neuanlegen. |
| Lesen, Klassifizieren oder Rechteprüfung fehlgeschlagen | Kein Upload; Arbeitskopie erhalten und Fehler anzeigen. |

Bei einem Konflikt kann der Benutzer weiterbearbeiten, die Arbeitskopie behalten
oder den aktuellen Remote-Stand neu laden. Neuladen verwirft Änderungen nur nach
Bestätigung und legt eine neue Vergleichsbasis an. Es gibt in der ersten Version
keinen `Overwrite anyway`-Knopf und kein automatisches Merge.

FTP-Status `550` kann auch eine Berechtigungsverweigerung bedeuten. Er darf nicht
pauschal als bestätigte Löschung dargestellt werden.

Zwischen erneuter Prüfung und serverseitigem Rename bleibt ein Race-Fenster.
`remote.Client` bietet kein bedingtes Schreiben; die Prüfung ist keine atomare
Compare-and-Swap-Garantie. Das gilt auch für Zieltypwechsel oder Löschungen in
diesem Fenster und ist in Hilfe und Dokumentation zu benennen.

### Übertragung und unklarer Ausgang

Die Übertragung nutzt `internal/sync.Run` mit genau einem Upload-Item, der
Sitzungs-Root und dem zuvor geprüften Remote-Pfad. `remote.Client.Upload` verwendet
bereits Staging-Dateien und Rename. Keine zweite Transferimplementierung und keine
Editor-spezifische Remote-Client-Schnittstelle ergänzen.

Die Protokollgrenzen bleiben bestehen: SFTP erhält vorhandene Berechtigungsbits;
FTP/FTPS garantieren nicht dieselbe Metadatenerhaltung oder identische Rename-Semantik
auf jedem Server. Atomare Ersetzung ist von den Serverfähigkeiten abhängig.

Bei Verbindungsverlust kann ein Upload trotz Fehlermeldung bereits angekommen sein.
Dann `Save result unknown` anzeigen, Snapshot und Arbeitskopie erhalten und niemals
automatisch erneut hochladen. Nach einer ausdrücklich angeforderten Wiederverbindung
zuerst den Remote-Inhalt prüfen. Übereinstimmung mit dem Snapshot bestätigt den
Erfolg; eine Abweichung wird erneut gegen die Originalbasis geprüft. Ein weiterer
Upload benötigt erneut Zustimmung.

`Esc` oder `Ctrl+C` während eines Netzwerkaufrufs bedeutet Abbruchanforderung, nicht
bestätigte Rücknahme eines bereits begonnenen Uploads. Solange der Ausgang unklar
ist, dürfen weder Erfolg noch sicheres Nichtschreiben behauptet werden.

## Architektur

### Bestehende Ansatzpunkte

| Bereich | Nutzung |
| --- | --- |
| `internal/tui/browser/keys.go`, `update.go`, `model.go` | `e`, Help, Voraussetzungen und typisierte Editieranforderung ergänzen; `StartsNetworkOperation` berücksichtigen. |
| `internal/tui/browser/remote.go`, `preview.go` | Verbindungsübergabe und gezielte Aktualisierung nach Rückkehr; Preview nicht als Editor-Datenquelle verwenden. |
| `internal/tui/app.go`, `state.go` | Eigenen Sitzungs-/Bestätigungs-Screen und Nachrichten routen. |
| `internal/tui/app_connection.go` | Root-eigene Verbindungsbeobachtung um den neuen Eigentümer ergänzen. |
| `internal/tui/app_certtrust.go` | FTPS-Vertrauen für ausdrücklich angeforderte Wiederverbindungen integrieren, ohne Transfer-Auto-Retry. |
| `internal/config/config.go`, `loader.go`, `writer.go` | Editor-Konfiguration laden, validieren und verlustfrei erhalten. |
| `internal/fs/root.go`, `internal/sync/run.go` | Vorhandene lokale Confinement- und Transferpfade verwenden. |
| `internal/styles/styles.go`, `internal/tui/styles.go` | Fehlende Sitzungs-/Dialog-Styles zentral ergänzen. |

### Fachlogik und UI trennen

Neues Package `internal/remoteedit` enthält nur tatsächlich benötigte Sitzungslogik:
Arbeitsverzeichnis, unveränderliche Originalbasis, Größen- und Pfadprüfungen,
Snapshot-Erstellung, Konfliktprüfung, Transferaufruf und kontrolliertes Aufräumen.
Es importiert kein Bubble Tea und liefert Werte sowie typisierte Fehler zurück.

`internal/tui/remoteeditor` ist trotz des Namens kein Texteditor. Der Screen zeigt
Sitzungszustand, Dateipfade, Bestätigung, Konflikt und Fehler. Er verpackt Aufrufe
der Fachlogik in `tea.Cmd` und bindet den externen Prozess ein. Netzwerk-, lokale
Datei- und Prozessarbeit laufen nicht synchron in `Update` oder `View`.

Der Prozessaufruf wird als Argumentliste aufgebaut. Terminal-Prozesse starten über
`tea.ExecProcess`, GUI-Prozesse in einem asynchronen Command. GUI-stdout/stderr
werden verworfen; Prozessfehler werden bereinigt angezeigt. Dateiinhalte,
Prozessausgaben und Zugangsdaten gehören nicht ins Log.

### Verbindungseigentum

Für diese Variante wird die bereits verbundene Browser-Verbindung exklusiv an die
Editiersitzung übergeben, ähnlich dem Übergang zur Diffview. Der Browser behält
seinen Baum und seine Navigation, besitzt währenddessen aber keine aktive
Verbindung. Es wird nicht vorsorglich eine zweite Verbindung geöffnet.

Die Root-Beobachtung folgt derselben Verbindungs- und Projektidentität und verarbeitet
Ausfälle auch bei offenem Bestätigungs- oder Zertifikatsdialog. Während eines
Terminal-Editors können TUI-Nachrichten verzögert verarbeitet werden; vor jedem
weiteren I/O sind deshalb auch `Err()` und `Done()` zu prüfen.

Beim Abschluss wird eine noch verwendbare Verbindung ausdrücklich an denselben
Browser-Kontext zurückgegeben. Eine fehlgeschlagene Verbindung wird nicht als gesund
zurückgereicht. Sie wird in einem Command geschlossen; Baum und Arbeitskopie bleiben
erhalten. Es gibt keine automatische Wiederverbindung oder Transferwiederholung.

Eine manuell angeforderte Wiederverbindung verwendet ausschließlich
`remote.Connect(ctx, host, trustManager, requiredChallenge)` und den root-eigenen
Trust-Manager. Ein akzeptiertes FTPS-Zertifikat erlaubt zunächst nur die Fortsetzung
der Verbindung/Prüfung, niemals einen ungeprüften erneuten Upload.

### Sitzungszustand und Nachrichten

Die Sitzung hält mindestens Host-Snapshot, Projektidentität, Remote-Pfad,
Verbindung, eigene lokale Root, Arbeitsdateipfad, Originalbasis, bestätigten
Snapshot sowie eindeutige Sitzungs- und Operations-IDs.

Vorgesehene Zustände:

```text
Preparing → Editing → Awaiting decision → Checking → Uploading → Finished
```

`Edit again` führt zurück nach `Editing`. Konflikt, Fehler und unklarer Ausgang
sind eigene Zustände, die Arbeitskopie und Vergleichsbasis erhalten.

Typisierte Nachrichten für Start, Download-Ergebnis, Prozess-Ende, Snapshot,
Prüfergebnis, Upload-Ergebnis und Rückkehr enthalten die zugehörigen Identitäten.
Ein verspätetes Ergebnis darf keine andere Datei oder ein neues Projekt verändern.
Veraltete Ergebnisse räumen ihre Ressourcen auf, ohne unbestätigte Arbeitskopien
zu löschen. Doppelklicks beziehungsweise wiederholte Tasten starten keine zweite
Sitzung oder einen zweiten Upload.

`q`, `Ctrl+C`, Fenstergrößenänderungen und Rückkehraktionen sind je Zustand zu
testen. Ein Beenden mit geänderter Arbeitskopie verlangt eine ausdrückliche
Entscheidung zum Behalten oder Verwerfen; im gestarteten Terminal-Editor gelten
dessen eigene Tasten. Upload-Zustände dürfen nicht über die normale Browser-
Quit-Logik übergangen werden.

## Umsetzungsschritte und Abnahme

### 1. Konfiguration und sicherer Prozessaufruf

- Go-Konfiguration und Writer um den optionalen Editor-Abschnitt ergänzen.
- Rust-Store um den notwendigen Schema-Roundtrip ergänzen, ohne GUI-Anbindung.
- Argumentersetzung und Validierung testen; keine Shell-Zwischenschicht.

Abnahme: Beispiele für `nvim` und `code --wait` sind korrekt abbildbar. Host-Saves
beider aktualisierter Anwendungen erhalten den Abschnitt. Fehlende Konfiguration
verändert das bestehende Browser-Verhalten nicht.

### 2. Isolierte Sitzung und Download

- `internal/remoteedit` mit privatem Cache-Arbeitsverzeichnis und eigener `fs.Root`.
- Mapping-/Dateitypprüfung, begrenzter Download und unveränderte Vergleichsbasis.
- Ressourcenbesitz, Aufräumen und Erhalt bei Fehlern festlegen und testen.

Abnahme: Originalbytes kommen unverändert beim Editor an. Keine lokale Projektdatei
wird gelesen oder geschrieben. Unsichere oder zu große Dateien starten keinen Editor.

### 3. Editor-Lebenszyklus und Bestätigung

- Sitzungs-Screen, Terminal-Übergabe und asynchronen GUI-Prozess integrieren.
- Nach Prozess-Ende Datei prüfen und einen unveränderlichen Snapshot erzeugen.
- Bestätigen, erneut bearbeiten, behalten und verwerfen umsetzen.

Abnahme: Kein Editor-Save und kein Prozessfehler lädt automatisch hoch. Ein
unveränderter Abschluss benötigt keinen Bestätigungsdialog. Terminal und Maus
werden auch nach Editor-Fehlern korrekt wiederhergestellt.

### 4. Konfliktprüfung und Upload

- Aktuellen Remote-Inhalt erneut prüfen und mit Originalbasis/Snapshot vergleichen.
- Bestehenden Sync-Upload auf dem Sitzungs-Snapshot ausführen.
- Konflikt, Fehler und unklaren Ausgang getrennt behandeln.
- Explizite Wiederverbindung samt FTPS-Trust und erneuter Prüfung integrieren.

Abnahme: Beobachtete konkurrierende Änderungen werden nicht still überschrieben.
Jeder neue Upload ist bestätigt, Fehler erhalten die Arbeitskopie und unklare
Ausgänge führen nie zu einem automatischen Transfer-Retry.

### 5. Browser-/Root-Integration und Dokumentation

- `e`, Hilfe, Screen-Routing, Netzwerkaktivität und Verbindungsbeobachtung ergänzen.
- Eigentum bei Einstieg, Rückkehr, Fehlern und Shutdown eindeutig übertragen.
- Remote-Metadaten/Preview gezielt aktualisieren, ohne Browser-Neuaufbau.
- `README.md`, `CHANGELOG.md`, `docs/manual-testing.md` und gegebenenfalls
  Architekturregeln in `AGENTS.md` aktualisieren.

Abnahme: Der vollständige Ablauf funktioniert mit SFTP, FTP und FTPS, Terminal-
und GUI-Editor. Warteoption, Cache-Datenschutz, Größenlimit und Grenzen der
Konfliktprüfung/Metadatenerhaltung sind dokumentiert.

## Testmatrix

Keine gemockten Remote-Clients. Transporttests verwenden echte Loopback-SFTP-,
FTP- und FTPS-Verbindungen; `internal/ftptest` für FTP/FTPS wiederverwenden. Rein
lokale Sitzungsregeln und TUI-Zustände benötigen keine Remote-Verbindung. Für
Prozess-Tests einen echten kurzlebigen Hilfsprozess verwenden, keinen Remote-Mock.

| Bereich | Pflichtfälle |
| --- | --- |
| Konfiguration | Fehlender Abschnitt, Default für `terminal`, ungültige Argumentliste, fehlender/doppelter Platzhalter, unbekanntes Programm, Go-/Rust-Roundtrip nach Host-Schreibvorgängen. |
| Prozessaufruf | Leerzeichen, Unicode, Shell-Metazeichen und führender Bindestrich im Dateinamen; kein Shell-Aufruf; nicht vorhandenes Programm; Exit ungleich null; korrektes Warten. |
| Inhalt | Leere Datei, LF/CRLF/gemischte Zeilenenden, BOM, ungültiges UTF-8, NUL, Tabs, fehlender Schlussumbruch; bytegleicher Download und keine drift-seitige Umwandlung. |
| Arbeitsdatei | 0700/0600, Größenlimit vor/nach Bearbeitung, atomarer Editor-Save, gelöschte Datei, Symlink-Escape, nichtreguläre Datei, private Snapshots und Aufräumfehler. |
| Einstieg | Lokales Pane, Verzeichnis, Symlink, ungemappter Pfad, Hard Exclusion, direkt gewählte ignorierte Datei, kein Host, tote Verbindung und Busy-State. |
| Bestätigung | Unverändert ohne Upload; Änderungen nur mit Zustimmung; erneut bearbeiten; behalten; bestätigtes Verwerfen; Prozessfehler ohne Upload; neue Bytes nach Snapshot weder still übertragen noch nach Upload-Erfolg löschen. |
| Konflikt | Geänderter Inhalt trotz gleicher Größe/Zeit, Remote bereits gleich Snapshot, beobachtete Löschung, Rechtefehler/FTP 550, Typwechsel, Reload nur nach Verwerf-Bestätigung. |
| Verbindung/Upload | Ausfall während Editor-Lauf, vor/während Upload, Rename-Fehler, unklarer Ausgang, expliziter Reconnect, FTPS-Challenge, kein automatischer Transfer-Retry. |
| Asynchronität | Veraltete Session-/Operations-ID, Doppelauslösung, Projekt-/Hostidentität, Dialogrouting, verzögertes Prozess-Ende, Reader-Close-Fehler, Schließen außerhalb von Update. |
| Browser/Terminal | Cursor, Scrollposition, Expanded-State und Markierungen erhalten; gezielte Metadaten-/Preview-Aktualisierung; Resize, Alt-Screen, Maus an/aus, Rückkehr nach Editor-Fehler. |
| Daten/Logging | Keine Projektdatei verändert; Arbeitskopie bei Fehlern erhalten; keine Inhalte/Zugangsdaten im Log; Logging standardmäßig aus; keine GUI-Prozessausgabe in der TUI. |

Nach der späteren Implementierung:

```sh
go test ./...
go test -race ./...
go vet ./...
go build ./...
cd rust && cargo test -p drift-core
```

Zusätzlich manuell mit `nvim` und `code --wait`, kleinen Terminalgrößen und
Testhosts für SFTP, FTP und FTPS prüfen. Dabei Paralleländerungen, entzogene
Schreibrechte und Verbindungsabbrüche gezielt auslösen. Automatisierte Tests
ersetzen nicht die Prüfung der Terminal-Übergabe und der tatsächlichen
GUI-Warteoption.
