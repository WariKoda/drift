# Rust-/GPUI-Port von drift

## Ziel und Produktgrenzen

Die Rust-Anwendung wird neben der Go-TUI im bestehenden Repository entwickelt.
Beide sind eigenständig ausführbar und haben eigene Build-, Installations- und
Release-Wege: `drift` für die Go-TUI und zunächst `drift-gui` für die Rust-GUI.
Rust benötigt kein Go-Binary zur Laufzeit. Gemeinsame TOML-Formate werden weiter
koordiniert. Fachliche Referenz sind der aktuelle Go-Code und seine Tests;
ältere Go-Pläne beschreiben teilweise bereits überholtes Verhalten.

Die erste reguläre GUI-Veröffentlichung erreicht Parität für Projekte,
Hostverwaltung, SFTP/FTP/FTPS, Browser, Vorschau, Finder, Vergleich, Sync,
Zertifikatsvertrauen und die vorhandenen Verwaltungsbefehle. Zielplattformen
sind Linux mit Wayland/X11 und macOS 15+ auf Intel und Apple Silicon. Mehrere
Arbeitsbereiche, ein schreibender Datei-Editor, neue Protokolle und automatische
Transferwiederholungen bleiben außerhalb der ersten Version.

## Oberfläche und Themes

Zwei unabhängig navigierbare Dateibäume, veränderbare Bereichsgrößen, Toolbar,
Kontextmenüs und Dialoge bilden die Oberfläche. Zeilen werden virtualisiert.
Mehrfachauswahl, rekursive Markierungen und bestehende Tastaturabläufe werden
portiert; Buchstabenbefehle greifen ausschließlich im Browser-/Diff-Kontext,
niemals während Texteingaben. Ordnernavigation bietet Up sowie Zurück/Vorwärts.
Abgebrochene oder fehlgeschlagene Navigation erhält den bisherigen Ordner.
Registrierte Projekt-Roots begrenzen den Sync; die freie Ordnerauswahl kann
ausdrücklich einen neuen Projekt-Root öffnen.

Die Standard-Themes sind verbindlich:

| Einstellung | Verwendetes Theme |
| --- | --- |
| **System** (Voreinstellung) | Betriebssystem hell: **Monokai Pro Light Sun**; dunkel: **Monokai Pro Dark** |
| **Light** | **Monokai Pro Light Sun** |
| **Dark** | **Monokai Pro Dark** |

Die Einstellung wird in `<config.Dir()>/gui.toml` gespeichert. System folgt auch
Änderungen der OS-Darstellung während der laufenden Sitzung; Light und Dark
bleiben ausdrücklich festgelegt. Ein Wechsel gilt für Fenster, Kit-Komponenten,
Browser, Vorschau und Diff, ohne die Sitzung oder Auswahl zurückzusetzen.
Farbzuordnung, Fokus, Auswahl, Fehler und Additions-/Deletionsmarkierungen werden
in beiden Themes auf Lesbarkeit geprüft. Die Themes sind geplant; die bisherigen
Kit-Standardfarben stellen noch keine Umsetzung dieser Vorgabe dar.

Vorschau: höchstens 1 MiB, reguläre Textdateien, Zeilennummern, Umbruch,
Textauswahl und native Zwischenablage. Finder/Vorschau laufen im Hintergrund;
späte Ergebnisse ändern weder die falsche Datei noch den Fokus.

Unified-Diff: Dateiliste mit Status/Aktion, zwei Zeilennummernspalten, Hunk-Header,
drei Kontextzeilen, Faltung unveränderter Bereiche, Hunk-Navigation und stabile
Quellanker. Upload zeigt Remote → Local, Download Local → Remote; Delete zeigt
die Entfernung auf der betroffenen Seite. Vergleichsdaten bleiben unveränderlich;
Faltung, Auswahl und Scrollzustand gehören zur View. Zeds Diff ist eine
Architekturreferenz für eine eigene schreibgeschützte Darstellung.

## Architektur

| Crate | Verantwortung |
| --- | --- |
| `drift-core` | Rohkonfiguration, Registry, Mapping, lokale Roots, Ignore-Regeln, Transporte, Diff, Sync-Policies |
| `drift-app` | Abläufe, Sessions, Hintergrundaufgaben, Fortschritt, Abbruch, Ressourcen |
| `drift-gui` | CLI-Einstieg, GPUI/Kit-Views, Actions, Fokus, Dialoge, Themes |

Nur `drift-gui` hängt von GPUI ab. GPUI Kit und seine zusammengehörigen Pakete
sowie die Rust-Toolchain sind gepinnt. Tokio führt Netzwerkaufgaben aus;
blockierende Dateiarbeit und große Diffs laufen in begrenzten Hintergrundaufgaben.
SFTP verwendet `russh`/`russh-sftp`, FTP/FTPS `suppaftp` mit Tokio/Rustls.
Lokale Zugriffe verwenden `cap-std`; Persistenz `serde`/`toml`, Textdiffs `similar`,
gestreamte Inhaltsvergleiche SHA-256.

Vor Meilenstein 3 werden die Views aufgeteilt. Diese Voraussetzung ist im
aktuellen Stand umgesetzt: `shell.rs` koordiniert Sitzung, Projektwechsel,
Registrierung und Hostverwaltung. `browser.rs` besitzt pro Pane Navigation,
History, Auswahl, Filter, Finder, Scrollzustand und Listing-Abbruch;
`preview.rs` besitzt Vorschau und deren eigene Operationsidentität.
`projects.rs` verwaltet das Projekte-Panel, `hosts.rs` die Hostformulare,
`toolbar.rs` die zustandslosen Toolbar-Controls und `actions.rs` die Actions.
Die Komponenten kommunizieren über typisierte Ereignisse und prüfen veraltete
Ergebnisse an ihren Zustandsgrenzen. Dateiarbeit bleibt in `drift-app`.

Ein Headless-Test rendert zwei Browser-Panes im selben Fenster und prüft getrennte
Filter, Auswahl, Fokus, History und Abbruch. Die Anwendung bietet jetzt zusätzlich
einen SFTP-Remote-Baum in `remote.rs`: Hostauswahl, unabhängige Navigation/History,
Filter, Textvorschau im gegenüberliegenden
Bereich, Abbruch, Shutdown und Beobachtung des Verbindungszustands. Dialoge und
Diff erhalten im jeweiligen Meilenstein eigene Views; ihre Zustände werden nicht
in `Shell` oder den lokalen Browser eingebaut.

Remote-Clients erhalten ausschließlich Remote-Pfade und Streams und besitzen
Verbindungszustand und Shutdown. `ProjectRoot` kapselt lokale Zugriffe und atomare
Downloads. Vergleichssessions besitzen Verbindung, Root, Ergebnisse und Scope
und schließen ihre Ressourcen gemeinsam. Typisierte Ereignisse enthalten
Projekt-, Verbindungs- und Operationsidentität. Veraltete Ergebnisse werden
verworfen und Ressourcen geschlossen; Fortschritt darf zusammengefasst werden,
Abschlüsse und Fehler dürfen nicht verloren gehen.

## Persistenz und Sicherheit

Bestehende Speicherorte, `$XDG_CONFIG_HOME`, TOML-Felder, Slugs, Zeitstempel,
Defaults, Serverlinks, Mappings und Zertifikatsausnahmen bleiben kompatibel.
Rohwerte bleiben von aufgelösten Laufzeitwerten getrennt. GUI-Präferenzen liegen
separat in `gui.toml`, keine Verwaltungsdatei im Projektverzeichnis.

Aktualisierte Go-TUI und Rust verwenden dieselbe permanente `write.lock` mit
`flock`: Lesen, Validieren, Ändern und alle Dateien einer Verwaltungsoperation
liegen unter der Sperre. Frische Daten werden geladen; Änderungen am selben
Datensatz seit Formularöffnung erzeugen einen sichtbaren Konflikt und erhalten
die Eingabe. Unabhängige Änderungen werden erhalten. Atomische Writes,
restriktive Rechte und das vorhandene Rücksetzverhalten bleiben bestehen.

Mapping-Priorität und Segmentgrenzen entsprechen Go; konfigurierte Mappings
begrenzen den Sync-Bereich. Ignore-Klassifikation erfolgt gebündelt über echtes
`git check-ignore`, mit vorherigem lokalem Mapping von Remote-Pfaden. Sichtbarkeit
und Sync-Scope bleiben getrennt. Direkt ausgewählte ignorierte Dateien können
Ausnahmen sein, harte Ausschlüsse und Transfer-Staging-Dateien niemals.

Projektgebundene I/O wird über geöffnete Verzeichnis-Capabilities ausgeführt,
nicht durch eine alleinige `canonicalize`-Prüfung autorisiert. Symlink-Escapes und
Spezialdateien werden abgewehrt. Transfers verwenden benachbarte Staging-Dateien
im bestehenden Namensformat; Download ersetzt erst nach vollständigem Lesen,
Transferabschluss, Flush und geprüftem Schließen.

Vergleich: Metadaten-Schnellpfad, 2-MiB-Textgrenze, gestreamter SHA-256-Vergleich
großer Dateien. LocalOnly → Upload, RemoteOnly → Download; Inhaltsunterschiede
mit deutlich neuerer Remote-mtime → Download, unklare Zeitstempel → Upload.
Fehlerhafte Einträge erhalten keine ausführbare Aktion. Höchstens acht SFTP-
Worker beziehungsweise vier FTP-Verbindungen; abgelehnte zusätzliche FTP-Logins
reduzieren Parallelität. Sync bleibt seriell. FTP-550-Klassifikation prüft das
Elternverzeichnis zusätzlich und erhält Zugriffsfehler.

Keep-alive: 60 Sekunden als Default, `0` deaktiviert, Probe-Timeout 15 Sekunden.
FTP-Probes überspringen belegte Verbindungen, SSH-Probes können Transfers
begleiten. FTPS behält TLS 1.2 und endpoint-/fingerprintgebundene Ausnahmen;
Handshake-Challenges öffnen erst nach Ende des Hintergrundaufrufs einen Dialog.
Der erste Retry verlangt das gerade bestätigte Zertifikat.

Abbruch stoppt weitere Arbeit und laufende Netzwerk-I/O. Bestätigte Transfers
bleiben erhalten; Verbindungsverlust sperrt Sync bis zu einem neuen Vergleich.
Keine automatischen Transfer-Retries. Fortschritt kann ohne Abbruch verborgen
werden. Projektwechsel/Fensterende brechen Arbeit ab und schließen Ressourcen
im Hintergrund. Logging bleibt dateibasiert, standardmäßig aus und ohne Secrets.

## Meilensteine und Stand

| Meilenstein | Ergebnis / Abnahme | Stand |
| --- | --- | --- |
| 1. Grundlage | Workspace, Toolchain, CI, Kit-Fenster, Fokus/Eingaben/Clipboard, Virtualisierung, gemeinsame Fixtures | Grundlage vorhanden; native Plattformabnahme offen |
| 2. Persistenz / lokaler Browser | TOML/Registry/Mapping/Ignore/Root, gemeinsame Sperre, Projekte, Hostformulare, Finder/Vorschau, GUI-Präferenzen/Themes | In Arbeit: Stores, lokaler Browser, Rücknavigation, aufgeteilte Views, Projektwechsel und Host-CRUD mit projektübergreifenden Links/Server-Promotion, Mappings, Verbindungstest und Trust-Reset sowie Projekt-CRUD/Archivieren, Dashboard und Startwiederherstellung vorhanden; automatische Endpunktvorschläge, vollständige Tastaturparität und Themes offen |
| 3. SFTP | Auth-Fälle, Remote-Browser, Vergleich, Unified-Diff, alle Sync-Aktionen, Abbruch/Verlust | In Arbeit: SFTP-Verbindung/Browser/Vorschau, Vergleich/Unified-Diff und serieller Upload/Download/Delete mit Abbruch und Verlust vorhanden; Hostzertifikate und vollständige Auswahl-/Textselektionsparität offen |
| 4. FTP / FTPS | Listings, Missing-Klassifikation, TLS/Trust-Dialoge, Keep-alive, Vergleichsparallelität | In Arbeit: nativer FTP-/FTPS-Browser/Vorschau/Vergleich/Sync, Pool bis vier Verbindungen, adaptive Login-Grenze, 550-Prüfung und Keep-alive sowie TLS 1.2, Zertifikatsspeicher und Trust-Dialog vorhanden; vollständige native Plattformabnahme offen |
| 5. Parität | Verwaltung/CLI/Tastatur; Refresh und Sync bauen Vergleich mit erhaltenem Scope neu auf | Offen |
| 6. Veröffentlichung | Linux-Paket/Desktop-Eintrag, macOS-Bundles für Intel/Apple Silicon, Installation und Release-Builds | Offen |

Der SFTP-Durchstich verwendet native `russh`-/`russh-sftp`-Clients und ein
Remote-Session-Handle in `drift-app`. Die GUI erreicht Transporte ausschließlich
über diesen Dienst. Der ausdrückliche Abbruch laufender Remote-I/O schließt
die Verbindung; die Oberfläche bietet danach einen ausdrücklichen neuen Connect.
Ein Vorschauwechsel verwirft das alte Ergebnis und lässt den bereits laufenden,
auf 15 Sekunden begrenzten Read einschließlich Dateischließen enden. Pro Session
liest höchstens eine Vorschau gleichzeitig; überholte wartende Anfragen entfallen.
Projekt-, Host- und Operationswechsel verwerfen verspätete Ergebnisse. Die
bekannte TOFU-Regel bleibt bestehen; geänderte und widerrufene Hostkeys werden
abgewiesen. OpenSSH-Hostzertifikate/CA-Einträge werden aktuell mit einem klaren
Fehler abgewiesen; ihre Parität ist noch offen.

Rust-Protokolltests starten einen echten lokalen Go-SSH/SFTP-Daemon, verwenden
OpenSSH-Schlüssel und einen echten Agent. Geprüft sind Passwort/Schlüssel mit
Passphrase, Agent-Timeout, Hash-/Pattern-Einträge und Widerruf in `known_hosts`,
Hostkey-Wechsel, Listing-/Vorschau-Limits, Keep-alive einschließlich Timeout,
Session-Scope, Projektwechsel und die GUI-Verbindung/Vorschau. Go ist nur für
diesen Test-Daemon und die Paritäts-Probe erforderlich; das GUI-Binary hat keine Go-Laufzeitabhängigkeit.

Der SFTP-Vergleich läuft in `drift-app::comparison` mit höchstens acht Workern.
Er hält Projekt-Root, Verbindung, Auswahl und Ignore-Scope als Session. Ganze
Projekte vergleichen die wirksamen Mapping-Roots; direkte lokale oder entfernte
Auswahlen erweitern die Gegenverzeichnisse rekursiv. Einzelfehler bleiben sichtbar
und erhalten keine Aktion. Metadaten-Schnellpfad, 2-MiB-Textlimit, gestreamtes SHA-256
und Aktionsvorschläge sind gegen echte Go-Vergleiche geprüft. F5 erhält den Scope;
Fortschritt kann ohne Abbruch verborgen werden. Ein Abbruch schließt die Verbindung.

`comparison.rs`/`comparison/view.rs` besitzen Dateiliste und Operationen;
`diff.rs` besitzt Unified-Zeilen, Richtung, Faltung, Quellanker und Scrollzustand
je Datei. Textauswahl erfolgt derzeit zeilenweise mit Shift-Klick und nativer
Zwischenablage; Auswahl einzelner Zeichen über Zeilengrenzen und Mehrfachmarkierung
in den Browsern bleiben offen. Die Aktionswahl zeigt die Vorschau; Sync selected
und Sync all actions führen die bestätigten Aktionen seriell aus. Die Bestätigung
nennt Upload-/Download-/Delete-Zahlen; der Dateifilter reduziert Sync all nicht.

`drift-app::sync` validiert Entscheidungen und Mapping-/Root-Grenzen gegen den
Vergleich und hält dessen Root/Verbindung während der Ausführung. Der Bericht
unterscheidet bestätigt, übersprungen, fehlgeschlagen, unklar und nicht versucht.
Abbruch/Verlust schließen Netzwerk-I/O, stoppen weitere Dateien und erhalten
bestätigte Abschlüsse. Bereits laufende lokale Mutationen werden bis zu ihrem
Ergebnis abgewartet. Nach einem regulär beendeten Sync entsteht ein neuer Vergleich
mit erhaltenem Scope; Fehlerberichte bleiben daneben sichtbar. Bestätigte Änderungen
aktualisieren auch die Browser. Nach Abbruch/Verlust ist ein neuer Connect und
Vergleich erforderlich, ohne automatische Transferwiederholung.

SFTP-Upload schließt Quelle und Staging-Datei vor Rename; Download verwendet
ProjectRoot::write_atomic und prüft den Remote-Close vor dem Zielersatz. Staging
verwendet auf beiden Seiten denselben Namensgenerator. Go/Rust-Sync-Parität prüft
Aktionen, Inhalte und Rechte. Echte Tests unterbrechen Transfers sowie die
Verbindung beim CLOSE nach erfolgreichem EOF. Der aktuelle Bibliotheksstand nutzt
für POSIX Rename einen optionalen zweiten SFTP-Kanal derselben SSH-Verbindung.
Bei dessen Ablehnung bleibt Standard-Rename verfügbar; Server ohne geeigneten
Zielersatz melden einen Fehler und behalten das bisherige Ziel. Diese Servergrenze
ist vor vollständiger Protokollparität noch aufzulösen.

Der FTP-Durchstich verwendet `suppaftp = 12.1.0` mit Tokio und dieselben
Remote-/Vergleichs-/Sync-Services wie SFTP. Eine Session besitzt höchstens vier
Verbindungen; zusätzliche abgelehnte Logins reduzieren den Pool. Eine Verbindung
bleibt über Datenstrom und Abschlussantwort reserviert. Unfertig verworfene
Operationen schließen die Session; mehrdeutige Transferantworten werden nicht
wiederholt. Keep-alive überspringt belegte Verbindungen, Shutdown unterbricht auch
laufende Kontroll- und Daten-I/O. EPSV/PASV sowie MLST/SIZE/MDTM/LIST-Fallbacks sind
gegen einen echten lokalen, dateisystemgestützten FTP-Server geprüft. `550` wird
nur durch eine erfolgreiche Eltern-/Vorfahrenliste als fehlend eingeordnet.
Vergleich und alle Sync-Aktionen sind gegen Go geprüft; Fehler beim Datenkanal-
Abschluss und Ziel-Rename erhalten alte Inhalte und sperren bei Verlust weitere
Aktionen. GUI-Tests führen Connect, Vorschau, Vergleich, Sync, Refresh und
Projektwechsel mit echten FTP-Verbindungen aus. Remote-FTP-Dateirechte entsprechen
der vom Server angelegten Staging-Datei wie in Go; lokale Downloads erhalten Rechte.
Explizites FTPS verwendet Rustls/TLS 1.2 mit nativen CA-Roots. Ein Verifikationsfehler
liefert eine typisierte Challenge; `certificates.rs` zeigt Endpunkt, SHA-256,
Aussteller, Namen, Gültigkeit und Probleme. `shell/certificates.rs` koordiniert
Ablehnen, Sitzungsvertrauen und dauerhafte Ausnahmen über Hintergrundaufgaben.
Ausnahmen gelten exakt für Endpunkt, Fingerabdruck und Problemmenge. Signaturen,
Key Usage und Kettenbedingungen bleiben verbindlich. Dauerhaftes Vertrauen nutzt
Go-kompatibles TOML, Modus 600, atomare Writes und dieselbe `write.lock`; bei
Änderung desselben Eintrags bleibt der Dialog mit einem Konflikt erhalten. Ein
fehlgeschlagener Write vergibt kein Sitzungsvertrauen.

Der erste Connect nach Bestätigung verlangt das geprüfte Zertifikat. Zusätzliche
Kontrollverbindungen und sämtliche Datenhandshakes sind an das primäre Zertifikat
gebunden; TLS-Resumption bleibt deaktiviert. Datenkanal-Zertifikatswechsel schließen
die Sitzung und erzeugen eine neue Challenge. Bestätigung verbindet den Browser;
Vergleich und Sync werden niemals automatisch wiederholt. Projekt-/Hostwechsel
verwerfen überholte Dialoge und Trust-Ergebnisse. Ein kleiner Rustls-Streamadapter
beginnt Datenhandshakes erst bei I/O, damit eine vorausgehende echte `550`-Antwort
keinen TLS-Handshake für einen abgewiesenen Transfer blockiert.

Die gemeinsamen FTP-Sync-/Go-Paritätsszenarien laufen ebenfalls über echtes FTPS,
einschließlich sämtlicher Aktionen, Abbruch, Verlust und Abschlussfehlern. Echte
TLS-Tests prüfen Unknown CA, Name, Ablauf/noch nicht gültig, ungültige Usage und
Signaturen, genaue Problemmengen, Pins und permanente Konflikte. GUI-Tests prüfen
Ablehnen, Sitzung/dauerhaft, Datenzertifikatswechsel und Projektwechsel bei offenem
Dialog; Prozess-Tests prüfen Trust-TOML in beiden Richtungen und die Schreibsperre.
Der SFTP-Abbruchtest wartet auf einen tatsächlich gestarteten, gedrosselten Upload,
bevor er Cancel auslöst. Die Go-Paritätsprobe lädt ihre Test-CA auf Linux und macOS
ausdrücklich; TLS-Tests mit begrenzten Logins warten auf das beobachtete Ende der
Serververbindungen, bevor sie den nächsten Connect beginnen.

Die Hostverwaltung bietet Verbindungstests für gespeicherte Hosts und ungespeicherte
Formulare. `Store::preview_host` löst frische Defaults und Serverlinks unter der
Schreibsperre auf, ohne Host-Datensätze zu speichern. Der App-Service besitzt eine
separate Testverbindung, prüft Root und Listing und schließt sie auch bei Fehler
oder Abbruch. Browserverbindungen bleiben erhalten. Der eigene Dialog in
`hosts/tools.rs` verwendet den root-eigenen Trust-Manager und die gemeinsame
Zertifikats-View; Freigabe wiederholt ausschließlich den Verbindungstest mit Pin.
Formularwerte und Fokus bleiben beim Zurückkehren erhalten.

Trust-Reset zeigt den aufgelösten FTPS-Endpunkt und Sitzung-/Datei-Ausnahmen vor
Bestätigung. Frische Daten werden unter `write.lock` mit der angezeigten Momentaufnahme
verglichen. Konflikte und Schreibfehler erhalten Dialog und Sitzungsausnahme;
unabhängige Endpunkte bleiben erhalten. Bestehende Verbindungen besitzen weiterhin
ihre unveränderliche Policy, neue Handshakes verifizieren wieder. Reset verbindet
nicht neu und wiederholt keine Transfers. Echte Protokoll-/GUI- und Go/Rust-Prozesstests
prüfen diese Abläufe. `hosts.rs`, `hosts/form.rs` und `hosts/tools.rs` trennen Liste,
Formular und Netzwerk-/Trust-Dialoge. Native Rendering-/Server-Abnahme bleibt offen.

`hosts/links.rs` bietet einen filterbaren Picker für globale Server und eigenständige
Hosts anderer Projekte einschließlich Herkunft und verknüpfender Projekte. Eine
Promotion verlangt ausdrückliche Bestätigung. `Store::select_link_target` prüft
Rohhost und Defaults gegen die gewählte Momentaufnahme, berücksichtigt inzwischen
belegte Servernamen und hält `write.lock` über alle Reads und Writes. Quelldefaults
werden materialisiert; Root und Mappings bleiben im Quelllink. Erst der globale
Server, danach der Quellstore wird geschrieben. Scheitert der zweite Write, bleiben
gültige Kopien erhalten und eine typisierte Antwort trägt die sichtbare Warnung;
es gibt keinen automatischen Retry. Das Zielformular behält Name, Root, Mappings
und Fokus und wird erst beim Speichern persistiert.

Echte Dateisystem-, GPUI- und Go/Rust-Prozesstests prüfen sortierte Linkziele,
Quell-/Default-Konflikte, Namenskollisionen, Schreibsperren, Teilerfolge,
unveränderte Formulare und fehlende Writes in Projektverzeichnissen. Die gemeinsame
Go-Parität prüft dieselben Rohdatensätze nach Promotion und einen konkurrierenden
Go-Edit nach Öffnen des Rust-Pickers. Automatische Angebote zum Verknüpfen eines
neu gespeicherten Hosts mit demselben Endpunkt bleiben eine Paritätsaufgabe.

Die Projektverwaltung bietet Anlegen, Bearbeiten mit unveränderlichem Slug,
Archivieren/Wiederherstellen und bestätigtes Entfernen. Formulare erhalten Werte
bei Validierungsfehlern oder einem konkurrierenden Edit, Open-Zeitstempel oder
Delete. Frische Registry-Daten werden unter `write.lock` geändert; unabhängige
Einträge bleiben erhalten. Entfernen versteckt zuerst den Quellstore, schreibt
die Registry und löscht anschließend die versteckten Einstellungen. Scheitert der
Registry-Write, wird der Store zurückgesetzt. Ein Cleanup-Fehler nach Commit wird
als sichtbare Warnung mit bestätigtem Ergebnis gemeldet. Lokale Dateien bleiben
unangetastet. Ein verschobenes/entferntes aktives Projekt schließt Verbindung und
Operationen, verwirft den alten Root und lädt den Browser neu; Schließen des
Dashboards oder Wahl desselben Projekts erhält die Sitzung.

Der Start entscheidet im begrenzten Hintergrunddienst: registrierter Kontext oder
unregistriertes Git-Repository → aktueller Ordner; sonst letztes geöffnetes,
aktives Projekt mit benutzbarem Pfad → Wiederherstellung; andernfalls vorhandene
Projekte → Dashboard. Ein explizites Ordnerargument bleibt maßgeblich, außer
`--dashboard` erzwingt die Liste; `--no-dashboard` hat Vorrang. Das initiale
Dashboard öffnet keinen Browser und schreibt keinen Open-Zeitstempel. Bei
Registrierung schlägt der Dienst den Git-Root einschließlich Worktrees vor.
`projects.rs`, `projects/form.rs`, `projects/management.rs` und
`shell/projects.rs` trennen Liste/Formular, Ergebnisübernahme und Sitzung/Start.
Echte Dateisystem-, App-, GPUI- und Go/Rust-Prozesstests decken Rücksetzen,
Konflikte, Archivierung, Startregeln und aktive FTP-Sitzungen ab. Die vollständige
Tastaturparität des Dashboards bleibt eine Aufgabe in Meilenstein 5.

Jeder Schritt entsteht auf einem kurzlebigen Branch und in einem validierten PR;
`main` bleibt releasable. Go-Ziele bleiben unabhängig von Rust verfügbar.

## Verifikation und Freigabe

- Gemeinsame fachliche Fixtures: Mapping, Scope, Ignore-Ausnahmen, Auswahl,
  Diff-Richtung/Faltung, Binärdateien, Limits, Aktionsvorschläge.
- Go ↔ Rust ↔ Go und Rust ↔ Go ↔ Rust: Defaults, fehlend/Null, Serverlinks,
  Zeitstempel, Trust und Konkurrenz zwischen realen Prozessen.
- Echte temporäre Dateibäume: Symlink-Escapes, Root-/Pfadwechsel, FIFOs,
  fehlgeschlagene Downloads, Rechte und Staging-Ausschlüsse.
- Echte lokale SSH/SFTP-, FTP- und FTPS-Server, keine Mocks. Auth/Agent-Timeout,
  Hostkey-/Zertifikatswechsel, Berechtigungen, Datenkanal-Abschlussfehler und
  Verbindungsverlust. Protokolltests dürfen im Freigabelauf nicht übersprungen sein.
- GUI: echte Ordnernavigation über Maus und Tastatur, Eingabefokus, Dialoge,
  Projektwechsel während I/O, veraltete Ergebnisse und versteckter Fortschritt.
  Theme-Modus/Persistenz, laufender OS-Wechsel in System, festgelegtes Light/Dark,
  beide Monokai-Paletten und Kontrast werden geprüft.
- Native Rendering-/OS-Clipboard-Prüfungen auf Wayland, X11 und beiden macOS-
  Architekturen; Headless-Tests ersetzen diese Abnahme nicht.
- Gleiche Bäume erzeugen dieselben Dateipaare, ausführbaren Entscheidungen und
  Sync-Ergebnisse. Gültige Textdiff-Aufteilungen dürfen variieren; Inhalte,
  Zeilenzuordnung und Richtung stimmen überein.
- CI: Rustfmt, Clippy, Workspace-Tests, Release-Build; Go-Test/Vet/Build.
  Freigabe erst nach nachgewiesenen Workflows, sichtbaren Fehlern, bedienbarer GUI
  während I/O und keinen Verwaltungsdateien im Projekt. MIT bleibt bestehen.
