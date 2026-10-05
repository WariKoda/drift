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

## Übergabe für die nächste Session

Stand: **5. Oktober 2026**. Browser-Mehrfachmarkierungen, Bereichsauswahl und
aufklappbare Dateibäume mit verzögertem Laden sind implementiert. Listenanfang/-ende,
Pane-Wechsel und Browser-/Vergleichs-Shortcuts sind ergänzt. Projekt-/Hostlisten,
Formular-Speichern und Lösch-/Trust-Reset-Bestätigung sind per Tastatur bedienbar.
Link-/Serverpicker, Zertifikatsentscheidungen und Diff-Scroll/Faltung sind ergänzt.
Numerische Dashboardwahl und einblendbare Sync-Fehlerdetails sind ergänzt.
Die vorhandenen CLI-Projektbefehle sowie open/dash/version sind ergänzt.
Optionales Datei-Logging samt Flags und Umgebungsvariablen ist umgesetzt.
Lokale und entfernte Browser-Kontextmenüs sind jetzt ergänzt, einschließlich
Tastaturöffnung, Fokus-/Abbruchverhalten und veralteter Callback-Abwehr.
Veränderbare Browser-/Vorschau-/Remote- und Vergleichs-/Diff-Bereiche sind ergänzt,
mit Drag, Tastatursteuerung und erhaltenem Sitzungszustand.
Vorschau-Toggle/-Fokus/-Kopie, Shortcut-Hilfe und Filter-Rückwege sind ergänzt.
Native Dialogbuttons behalten Enter/Space; Escape/Reject beendet auch ausstehende
Zertifikatsfreigaben ohne Retry und ohne Rücknahme bereits gespeicherten Vertrauens.
Finder-Fuzzy-Suche und explizite Rückkehr zum gespeicherten Browserzustand sind ergänzt.
Gemappte Remote-Ignored-Sichtbarkeit und Fokus-Reveal für gescrollte Host-/Mapping-
Kontrollen sind ergänzt. Tools/Links/Projektformulare haben begrenzte Details und
Aktionen samt Fokus-Reveal und abgesichertem asynchronem Buttonfokus.
Einzeilige Eingabegrenzen für Paste, native Text-/IME-Callbacks und Accessibility
sind ergänzt; gespeicherte problematische Formularwerte werden vor nativer
Normalisierung abgewiesen. Zeichengenaue Diff-Auswahl über Zeilengrenzen ist
mit graphemsicheren Endpunkten, Kopie sichtbarer Inhalte und erhaltener
Auswahl/Faltung/Scrollposition für unveränderte Refresh-Ergebnisse ergänzt.
Automatische Serverlink-Angebote beim Speichern gleicher Endpunkte sind ergänzt,
mit ausdrücklicher Link-/Eigenverbindungswahl und sichtbaren Commit-Teilerfolgen.
Die optische Überarbeitung bleibt ausdrücklich für später geplant.

### Ausgangspunkt und Git

- Aktueller Branch: `feature/rust-serverlink-offers`, aufgebaut auf
  `feature/rust-diff-text-selection` (Commit `07887ce`).
- Implementierungsstand: Commit `66359d9`,
  [Draft-PR #93](https://github.com/WariKoda/drift/pull/93) für automatische
  Serverlink-Angebote, Snapshot-/Defaultprüfung vor Writes und abgesicherte
  Native-Dialogrouten. Er basiert auf #92. Die macOS-CI fand einen Fixture-Fehler:
  Passwort-Select-all nutzte Ctrl+A statt Cmd+A; der Test verwendet jetzt die native
  Plattformtaste. Linux-Regression bestanden; neue vollständige CI noch nicht bestätigt.
- Basisstand: Commit `4ebdda8`,
  [Draft-PR #92](https://github.com/WariKoda/drift/pull/92) für zeichengenaue
  Diff-Auswahl, Inhaltskopie, virtualisierten Auto-Scroll und zustandserhaltenden
  Refresh. Er basiert auf #91; alle sechs Go-/Linux-/macOS-Checks von #92
  sind für `07887ce` bestanden:
  [37335795643](https://github.com/WariKoda/drift/actions/runs/37335795643) und
  [37335788987](https://github.com/WariKoda/drift/actions/runs/37335788987).
- Basisstand: Commit `4fe98de`,
  [Draft-PR #91](https://github.com/WariKoda/drift/pull/91) für Raw-Paste-/InputHandler-/
  Accessibility-Grenzen, IME-/Bestätigungsisolation und blockierte problematische
  gespeicherte Formularwerte. Er basiert auf Commit `86ae26c`,
  [Draft-PR #90](https://github.com/WariKoda/drift/pull/90) für begrenzte Tools-/Links-/
  Projektform-Details und sicheren asynchronen nativen Fokus. Er basiert auf
  [#89](https://github.com/WariKoda/drift/pull/89) für Fokus-Reveal gescrollter
  Host-/Mapping-Kontrollen (`2687a15`), dieser auf [#88](https://github.com/WariKoda/drift/pull/88)
  für gemappte Remote-Ignored-Sichtbarkeit (`68e6934`), dieser auf
  [#87](https://github.com/WariKoda/drift/pull/87) für
  Finder-Fuzzy-Suche und gespeicherten Browser-Rückweg (`2f99818`, Fixture-Fix `932917c`,
  Git-Pipe-Fix `9d12622`),
  dieser auf
  [#86](https://github.com/WariKoda/drift/pull/86) für Vorschau/Hilfe und sichere
  Tastatur-/Dialog-Routen, dieser auf
  [#85](https://github.com/WariKoda/drift/pull/85) für veränderbare Bereiche,
  dieser auf [#84](https://github.com/WariKoda/drift/pull/84) für Kontextmenüs,
  dieser auf [#83](https://github.com/WariKoda/drift/pull/83) für Datei-Logging,
  dieser auf [#82](https://github.com/WariKoda/drift/pull/82) für CLI-Projektverwaltung
  und open/dash/version, [#81](https://github.com/WariKoda/drift/pull/81) für
  numerische Projektdirektwahl/Sync-Fehlerdetails und
  [#80](https://github.com/WariKoda/drift/pull/80) für Picker-/Diff-Tastaturarbeit.
  Alle PRs #93 bis #80 sind weiterhin offene Drafts und nicht in `main` integriert.
  Vor Integration die
  gestapelte Kette #93 → #92 → #91 → #90 → #89 → #88 → #87 → #86 → #85 → #84 → #83 → #82 → #81 → #80 prüfen.
- Bestätigte CI von #92 bis #80 ist vollständig bestanden, jeweils Go und Rust Linux/macOS.
  #87 erhielt portable Git-Fehler-Fixtures (`932917c`) statt auf APFS unzulässiger
  nicht-UTF-8-Dateinamen und die primäre Git-Diagnose vor sekundärem EPIPE (`9d12622`).
  #88 hatte einen kapazitätsabhängigen Reconnect-Test: vier FTP-Sockets pro Pool
  überlappten mit asynchronem Shutdown. `70e3ab4` gibt allen vier überlappenden Pools
  Platz, ohne Produktions-Retries; 32 Scheduler-Seeds und Gesamtprüfung bestanden.
  Die App-Visibility-Fixture in #89 hatte dieselbe Peer-Cleanup-Grenze beim Wechsel vom
  Rohclient zur Dienst-Sitzung: `2a40c22` erlaubt beide Vier-Socket-Pools (8), ohne
  Sleep/Retry oder Produktionsänderung. Die korrigierte CI ist vollständig grün.
  Auch die Go-/Linux-/macOS-Läufe von #90 sind bestanden
  (`37205728099`, `37205731151`). Für #91 sind auch alle sechs Checks der Läufe
  [37316574252](https://github.com/WariKoda/drift/actions/runs/37316574252) und
  [37316567202](https://github.com/WariKoda/drift/actions/runs/37316567202) bestanden
  (geprüfter Stand `c8ae4b4`, Implementierung `4fe98de`). Damit ist die Paste-/IME-
  Härtung lokal und in Linux-/macOS-CI geprüft. CI ist keine native Plattformabnahme.

### Fortschritt auf einen Blick

| Bereich | Implementiert und lokal geprüft | Verbleibende Arbeit |
| --- | --- | --- |
| Projekte und Hosts | CRUD, Archivierung, Dashboard/Startwiederherstellung, Mappings, Serverlinks/Promotion und automatische Angebote, Verbindungstest, Trust-Reset und Fokus-Reveal, begrenzte Details/Aktionen und einzeilige Eingabe-/IME-Grenzen | Native Eingabeabnahme |
| Browser | Lokale/entfernte Navigation, gemappte Remote-Ignored-Sichtbarkeit, Filter/Vorschau, Finder-Fuzzy/Rückweg, aufklappbare Bäume, Mehrfach-/Bereichsmarkierungen, Kontextmenüs, veränderbare Bereiche, Vorschau-Tasten/Hilfe, Filter-Rückwege und Eingabegrenzen | Native Eingabeabnahme |
| Vergleich und Sync | Unified-Diff, graphemsichere Textauswahl über Zeilen, Faltung/Hunks, bestätigte Upload-/Download-/Delete-Aktionen, Zustandserhalt bei unverändertem Refresh, Abbruch/Verlust und Fehlerdetails | Native Plattformabnahme |
| Transporte | Native SFTP-/FTP-/FTPS-Verbindungen, Auth/Keep-alive und FTPS-Zertifikatsvertrauen | SSH-Hostzertifikate/CA und SFTP-Zielersatz auf eingeschränkten Servern |
| CLI und Diagnose | projects list/add/edit/archive/remove, open/dash/version, Hilfe/Fehlercodes und optionales Datei-Logging mit Flags/Umgebungsvariablen | Release-Versionseinbettung mit den GUI-Paketen |
| Oberfläche und Veröffentlichung | Kit-Oberfläche und lokaler Linux-Release-Build | GUI-Präferenzen/Monokai-Themes, native Plattformabnahme und Release-Pakete |

Letzte lokale Gesamtprüfung: **358 Rust-Tests, davon 210 Headless-GPUI-Tests
(GUI-Binärsuite 231); 0 fehlgeschlagen, 0 übersprungen**. Rustfmt, striktes All-target-
Clippy, Go-Test/Vet/Build, frischer store_probe, Go/Rust-Prozessparität,
Linux-Release-Build und Display-/Go-freier CLI-/Logging-Smoke bestanden.
Prüfartefakte: `/var/tmp/drift-offer-verified-*`; Go mit `TMPDIR=/var/tmp`, um den
fremden `/tmp/.git` aus Plain-directory-Fixtures fernzuhalten. CI der Basis #92 ist
vollständig bestanden. #93 hatte den oben beschriebenen macOS-Select-all-
Fixture-Fehler; korrigierte vollständige CI ist noch nicht bestätigt.
Zwei unabhängige statische Reviews sind ohne Blocker abgeschlossen. Offen bleiben
native Plattformabnahme sowie Review und Integration des Draft-PR-Stacks; die fünf
verbleibenden Arbeitsblöcke stehen unten.

### Implementiert und lokal geprüft

- Direkte Projekt-Hosts werden vor Save gegen gleiche Endpunkte geprüft:
  ASCII-case-insensitiver Hostname, effektiver Port, exakter User/Protokoll
  (leeres Protokoll = SFTP), jeweils mit eigenen Scope-Defaults. Auth, Keep-alive,
  Root und Mappings sind keine Match-Kriterien. Sortierte globale Server haben
  Vorrang vor sortierten Hosts anderer Projekte; andere Ziele bleiben im Picker.
- Angebote ändern keine Host-/Serverdaten. `y`/Use server link übernimmt ausdrücklich
  Server-Auth/Keep-alive und erhält Name/Root/Mappings; `n`/Save own connection
  erhält die eigene Verbindung. Back/Escape erhält native Inputs und Secrets.
  Native Enter/Space behält das Buttonlabel; kein Default-Enter für Linkfreigabe.
  Global/bestehende Links umgehen die Suche; ohne Treffer bleibt gewöhnliches Save
  samt Formular-/Fehlersemantik. Suchfehler speichern nicht stillschweigend.
- Linkfreigabe prüft unter einer Schreibsperre Zielversion/Namenskollision,
  Quell-Rawwerte/Defaults und frischen Endpunkt vor dem ersten Write. Promotion
  schreibt global → Quelle → Ziel mit kollisionsfreiem Namen. Nach Global-Commit
  bleiben Quellwarnung, typisierter Zielfehler und Serverreferenz erhalten; kein
  Rollbackversprechen oder automatischer Promotion-Retry. Eine Promotion, die
  den effektiven Account/Endpunkt ändern würde, scheitert vor dem ersten Write.
- Lookup-Abbruch entwertet Antworten; bestätigte Writes sind nicht abbrechbar.
  Old-frame-Pointer/Button- und Buchstabenrouten werden abgewehrt. Hilfe/Zertifikate
  deaktivieren synchron; Hintergrundabschluss stiehlt keinen fremden Fokus.
  Fokus-Rückgabe prüft Operation, native Inputidentität, aktuellen Owner und
  gemountetes Ziel im fertigen Baum. Help-Owner enthält die Host-Operation, damit
  Save/Reload keinen verschwundenen Formular-/Offer-Fokus zurückbringt.
- 17 Backend- und 17 neue Headless-GPUI-Regressionen nutzen echte Stores und native
  Eingaben: Defaults/Reihenfolge, unveränderte Suche, Eigenverbindung/globale Links,
  Promotion/Kollisionen, stale Versionen/Defaults, echte Write-Teilerfolge,
  IME/Hidden-Inputs/Secrets, Busy/Abbruch, alte Frames/Hilfe und kleine Viewports.
  Native Plattformabnahme bleibt getrennt.
- Die Diff-Auswahl verwendet unveränderliche Quellzeilenindizes und UTF-8-
  Graphemgrenzen statt Pixelendpunkten oder flüchtiger Auswahl einzelner
  virtualisierter Rows. Native `StyledText`-Geometrie liefert Hit-Tests und
  Markierung; `unicode-segmentation` ist direkt auf die bereits transitive
  Version `1.13.3` gepinnt, ohne Vendoring oder neue Paketversionen.
- Textklick setzt den Caret, Drag/Shift-Klick erweitert zeilenübergreifend;
  Gutterklick wählt eine Inhaltszeile. Links/Rechts bewegt den Caret,
  Shift-Pfeile/Shift-Home/End erweitern, Ctrl/Cmd+A wählt sichtbare Inhalte.
  Kopie enthält nur ausgewählten angezeigten Dateiinhalt mit Leerzeichen und
  Leerzeilen: keine Nummern, +/-Präfixe, Hunk-Header, Fold-Platzhalter oder
  eingeklappte Inhalte. Ohne nichtleere Auswahl bleibt die formatierte Diff-Kopie.
  Vergleichstext ist dekodiert/zeilenende-normalisiert, kein Raw-Dateiexport.
- Auto-Scroll trifft neu gemountete Zeilen nach abgeschlossenen Frames;
  Release/Escape, Blur, Datenwechsel und Abdeckung stoppen die eigene Geste.
  Resize behält seine Priorität. Hilfe/Hosts/Projekte/Zertifikate und Vergleichs-
  Schließen deaktivieren synchron, bevor ein neues Bild vorliegt; Capture-
  Grenzen verhindern alte Pointer-/Button-/Wheel-Callbacks und Fokusdiebstahl.
- Pro Datei bleiben Folds, Range und Scroll bei Dateiwechsel/Resize erhalten;
  Vorschau-Richtungswechsel ordnen Endpunkte in sichtbarer Reihenfolge und
  remappen den Scroll-Quellanker. Refresh erhält Zustand nur für dieselben
  lokalen/entfernten Pfadpaare mit identischen fehlerfreien Vergleichsdaten und
  gleicher Seitenpräsenz. Geänderte/entfernte/fehlgeschlagene Ergebnisse werfen
  alte Koordinaten ab; Transferentscheidungen werden frisch aufgebaut.

- Eigenständige Rust-App neben der Go-TUI; gemeinsame TOML-Dateien und permanente
  `write.lock`, mit getrennten Rohwerten und aufgelöster Konfiguration.
- `form_input.rs` schützt alle nativen einzeiligen Formularfelder und Filter, ohne
  ihre `InputState`-Entities, Editoren oder Historie zu ersetzen. Ganze Einfügungen
  mit `char::is_control()` (C0/C1/DEL, darunter Tab/NUL/CR/LF) oder U+2028/U+2029
  werden vor nativer Normalisierung abgewiesen. Unicode, kombinierende Zeichen,
  Emoji und Leerzeichen bleiben exakt; vorhandene fachliche Feldprüfung gilt weiter.
  Ablehnung verändert weder Wert, Auswahl, Komposition, Fokus, Scroll noch Undo.
  Statische Warnungen geben keine Zwischenablageinhalte oder Secrets wieder.
- Native UTF-16-Abfragen, Konfiguration und Geometrie werden delegiert. Paste liest
  den echten unveränderlichen Clipboard-Item; die asynchrone SDK-Fallback-Abfrage
  verlangt weiterhin Dokument/Auswahl/Markierung/Fokus/Editierbarkeit plus Revision
  und Request-Serial. Edit+Undo sowie Blur+Rückkehr machen alte Ergebnisse ungültig.
  Die Zwischenablage wird nicht umgeschrieben. Native OS-/Permission-Abnahme bleibt offen.
- Native Accessibility-`SetValue` umgeht im SDK den InputHandler und normalisiert
  selbst. Deshalb ersetzt ein geschützter zugänglicher TextInput-Knoten den
  exponierten nativen Knoten, mit tatsächlichen Kontrollgrenzen, Maskierungsprivacy
  und ohne weitere Tab-Stopps. Unexponierte SDK-Callback-IDs existieren weiterhin;
  Headless-Metadatentests ersetzen keine reale Accessibility-Abnahme.
- Escape/Enter bei fokussierter nativer Komposition beendet nur deren Markierung
  und behält das Preedit samt nativer Historie; es ist kein Rollback. Erst die
  folgende Taste folgt der Bildschirmroute. Resize/Popup/Hilfe bleiben vorrangig.
  Save/Test/Registrierung prüfen auch unsichtbare oder unscharfe komponierende Felder.
  Modifizierte Sync-Bestätigung darf einen komponierenden fokussierten Filter nicht
  übergehen; absichtlich fokussierte native Bestätigung bleibt eine explizite Aktion.
- Eine boundary-eigene, über Repaint erhaltene Einmal-Grenze weist passende leere
  IME-Folgecallbacks über der unveränderten Markierung ab, ob explizite/implizite
  Löschung, leeres Preedit oder Unmark. Gültige Bearbeitung/Auswahl, Paste, Blur und
  echte native Change-Ereignisse lösen sie. Öffentliche SDK-Callbacks haben keine
  Transaktions-ID: eine ununterscheidbare sofortige Löschung/Abbruch wird ebenfalls
  einmal unterdrückt; Escape oder die nächste native Aktion bleibt möglich.
- Gespeicherte Host-/Projekt-/Link-Werte mit verbotenen Zeichen verhindern das
  Formularöffnen bereits vor nativer Initialisierung. Reparatur geschieht außerhalb
  dieses GUI-Formulars, ohne automatische Konfigurations-/Secret-Umschreibung.
- Host-/Mapping-Kontrollen werden beim Tab-/Shift-Tab-Fokus im tatsächlich
  gemessenen äußeren Scrollbereich sichtbar. `focus_reveal.rs` arbeitet mit
  formular-eigenem ScrollHandle und nicht tabbbaren Kontroll-Scopes; native
  Input-/Button-Entities und deren Enter-/Space-Aktionen bleiben erhalten.
  Fokuswechsel und Viewport-Verkleinerung erlauben einmaligen Nearest-Reveal,
  normale Repaints oder Mausradbewegungen erzeugen kein Zurückschnappen.
  Vorherige Frame-/Fokus-/Viewport-Anfragen werden verworfen; Input selbst wird
  gemessen, nicht ein zu hoher Label-Block. Verborgene Abschlüsse stehlen keinen Fokus.
  Mapping-Löschung verwendet stabile Entity-IDs und fokussiert die verwandte
  verbleibende lokale Eingabe, nach der letzten Löschung das Rootfeld; keine
  Tastaturfalle durch einen entfernten nativen Button. Entwürfe/Maskierung bleiben
  erhalten; Kontrollen/Mappings/Aktionen wrappen. Keine Präferenz-/Config-Änderung.
- Tools-, Links- und Projektformular-Details/Aktionen haben begrenzte scrollbare
  Bereiche und wrappende Inhalte/Kontrollen. Link-/Projektlisten behalten den
  ursprünglichen ScrollHandle samt direkten Zeilenindizes; native Zeilenbuttons
  revealen im selben Handle. Eingabe-Entities, Query, Cursor, Drafts und Stores
  bleiben erhalten. Neue Lösch-/Promotion-Bestätigungen starten bei ihren
  identifizierenden Details oben, ohne Listenscroll zurückzusetzen.
  Deaktivierte native Tools-Buttons können aus dem Dispatchbaum fallen:
  vor Arbeit übernimmt ein lebender temporärer Owner den Fokus. Erst ein
  vollständig gerenderter neuer Baum darf den konkreten noch aktiven Control-
  Handle wiederherstellen, mit Operation-/Fokus-/Abbruch-/Zertifikatsprüfung.
  Enter auf diesem temporären Fallback-Owner bestätigt kein Zertifikatsreset,
  auch nicht zwischen Backendabschluss und deferred Restore oder nach einem
  entfernten/deaktivierten Ziel. Native affirmative Buttons und explizites `y`
  bleiben erreichbar. Trust-Reload hält seine Details deaktiviert gemountet;
  Fehler verwerfen den Snapshot. Zertifikatsroute und Retry-Regeln sind unverändert.
- Remote-Ignored-Klassifikation in `drift-app` nach Mapping mit kanonischem
  Session-Root; Host-Mappings gehen Projekt-Fallbacks vor. Die vorhandene
  gebatchte Git-Policy berücksichtigt Remote-only-/negierte/getrackte Pfade und
  Verzeichnismetadaten, ohne GUI-seitige Mapping-/Gitignore-Schleifen.
  Show-ignored-Button, Shift-I und angehaktes Kontextmenü ändern nur den gecachten
  Browser, nicht Vergleichsscope oder Transferbestätigung. Filter, History,
  Verbindung und verborgene Marks bleiben erhalten; ungemappte Pfade bleiben
  browsbar, aber nicht markierbar. Rohe/gemappte Hard-Exclusions bleiben ausgeschlossen,
  auch bei Off-tree-Verzeichnismarks und Baumwiederherstellung; äußere Komponenten
  des Host-Roots erzeugen keine falschen Exclusions. Revisions-/Projekt-/Operations-/
  Verbindungsprüfungen verwerfen alte Abschlüsse. Klassifikation sperrt veraltete
  Zeilenaktionen/Markierungen/Sichtbarkeitstoggles; Fehler bleiben sichtbar,
  ohne Session-Abbau. Lokaler Klassifikationsabbruch schließt keine Session.
  Unveränderte Auswahl erzeugt keine zusätzlichen Selection-Events, sodass
  bestätigte FTPS-Sync-Berichte nach Reconnect erhalten bleiben.
- Finder-spezifisches, deterministisches Unicode-Lowercase-Subsequenz-Matching in
  `drift-app`; zusammenhängende Treffer, Wort-/Pfad-/Camel-Grenzen und frühe/kurze
  Treffer werden bevorzugt, gleichwertige Treffer behalten ihre Reihenfolge.
  Normale Browserfilter bleiben Substring-Filter; keine vollständige Unicode-
  Normalisierung/Casefolding und keine identische Go-Score-Berechnung.
  `f` startet eine leere fokussierte Query; wiederholtes `f` fokussiert nur diese.
  Return-Button, Ctrl/Cmd+Alt+F (auch aus dem Filter) und Back stellen geladenen Baum,
  Einträge, sichtbare Zeilen, Cursor, Query, Range und ursprünglichen ScrollHandle
  wieder her. Markierungen bleiben live, einschließlich Hinzufügen/Entfernen in
  zugeklappten Ordnern. Normale Busy-Listings verhindern Finder-Einstieg.
  Explizite Rückkehr bricht Indexierung ab und verwirft alte/queued Abschlüsse;
  Refresh/Navigation stellt den Browser vor der normalen Aktion wieder her.
  Rootwechsel/Invalidierung kann keine alte Ansicht oder Marks zurückholen.
  Fehler stellt die Ansicht ohne Fokusdiebstahl wieder her; Hilfe verwirft dabei
  das Fokusziel eines verschwundenen Return-Buttons, nicht fremde Hostfilter.
- Lokale und entfernte Browser mit unabhängiger Ordnernavigation, Up/History,
  Filtern, Finder und Vorschau; virtualisierte Dateizeilen. Aufklappbare Knoten
  laden Kinder im Hintergrund. Zuklappen erhält Markierungen und zeigt deren
  Anzahl unter dem geschlossenen Ordner. Refresh stellt offene Knoten und den
  Cursor wieder her; Sichtbarkeitswechsel merken auch vorübergehend verborgene
  Ordner. Die explizite Ordnernavigation mit History bleibt verfügbar.
- Native SFTP-/FTP-/FTPS-Verbindungen, Authentifizierung, Keep-alive,
  Zertifikats-Challenges, Sitzungs-/Dateivertrauen und gepinnter erster Retry.
- Vergleich, Unified-Diff mit Richtung/Faltung/Hunk-Navigation, serieller
  Upload/Download/Delete, Fortschritt, Abbruch und Verbindungsverlust. Refresh und
  regulär beendeter Sync bauen den Vergleich mit erhaltenem Scope neu auf.
- Unabhängige Markierungen pro Browser, getrennt von Cursor/Vorschau: Space,
  v/V, Invertieren, Shift-Pfeile und Ctrl/Cmd-/Shift-Klick. Filterwechsel,
  Verzeichnisnavigation und Refresh erhalten Markierungen; Projekt-/Hostwechsel
  setzen sie zurück. `s` vergleicht alle Markierungen beider Panes; die beiden
  Auswahl-Buttons verwenden sämtliche Markierungen der jeweiligen Seite.
  Remote-Markierungen respektieren effektive Mappings und lokal übersetzte
  harte Ausschlüsse. Direkt ausgewählte ignorierte Dateien bleiben Ausnahmen;
  rekursive Ordnerauswahl überspringt ignorierte Kinder weiterhin.
- Pane-eigene Kontextmenüs überleben virtualisierte Zeilen. Rechtsklick verändert
  nur den Cursor, nicht Markierungen, Bereichsanker, Filter oder Vorschau.
  Leere Listenfläche bietet Pane-Aktionen ohne alten Cursorbezug. Vorschau,
  Ordner-/Baumnavigation, Markierungen, Pfadkopie, Vergleich und Abbruch verwenden
  bestehende Abläufe; Menüaktionen starten keine Transfers. Vergleich des
  angeklickten Eintrags ignoriert Markierungen als Scope, erhält sie aber;
  markierter Vergleich kombiniert beide Panes samt zugeklappten Kindern.
  Shift+F10/Menu öffnet am Cursor, Pfeile und Tab/Shift+Tab navigieren,
  Enter wählt und Escape kehrt ohne Filter-/Markierungsänderung zur Pane zurück.
  Außenklicks schließen und erreichen weiterhin die angeklickte Kontrolle.
  Browser-/globale Shortcuts greifen nicht hinter dem Menü. Verdeckte Panes,
  Verwaltungsdialoge und Vergleich schließen ihre alten Menüs ohne Fokusdiebstahl.
  Öffnungsidentität, Projekt-/Listing-/Verbindungsdaten und monotone View-Revisionen
  verwerfen alte Menü- und Zeilen-Callbacks auch bei weiterhin gültigem Pfad oder
  erneut gleicher Filter-/Mapping-Konfiguration. Vergleich prüft zusätzlich die
  gegenüberliegende Sitzung. Kit-0.7-Callbacks prüfen deaktivierte Einträge selbst;
  gecachte Fokus-Handles vermeiden rekursive Entity-Borrows während Item-Callbacks.
- Browser `p` schaltet die gewählte Dateivorschau ein/aus, `c` kopiert vollständig
  geladenen Text (auch eine leere Datei). Ctrl/Cmd+Alt+P fokussiert den nativen
  schreibgeschützten Editor; dessen Escape schließt nur die Vorschau und stellt
  den Quellbrowser wieder her. Sitzung, Marks und übrige Dateioperationen bleiben.
  F1 beziehungsweise ? im Browser/Diff öffnet scrollbare Shortcut-Hilfe ohne
  Fachaktionen; Escape stellt den aufrufenden Fokus wieder her. Neue FTPS-Challenges
  schließen Hilfe vor dem Prompt, damit kein verschwundener Hilfefokus gespeichert wird.
  Filter-Down/Enter/Escape stellt Listenfokus her, ohne Dateiaktivierung, Query-Clear
  oder Work-Abbruch. `f` fokussiert die Finder-Query; Ctrl+Escape bricht aktive
  Browser-/Vergleichsarbeit auch aus dem Filter ab. Modified Sync-Bestätigung gilt
  auch bei Filter-/Button-Fokus, benötigt weiterhin den angezeigten Plan.
  Native Enter/Space auf Cancel/Back/Reload/Test bleibt die beschriftete Aktion;
  Default-Enter bestätigt nur am Containerziel. Der Host-Löschcontainer ist kein
  unsichtbarer Tab-Stopp. Global-only-Hostlisten lassen nicht verfügbaren Scope-Tab
  zur normalen Traversierung durch; inaktive Listen verlassen die Tab-Reihenfolge.
  Reject während Trust-Speichern invalidiert den Abschluss und verhindert Retry,
  warnt aber ausdrücklich, dass bereits gespeichertes Vertrauen nicht rückgängig wird.
- Browser/Vorschau/Remote und Vergleichsliste/Diff besitzen veränderbare Bereiche.
  Drag und Ctrl+Alt+Links/Rechts verschieben den Trenner; Ctrl+Alt+0 setzt auf
  50/50 beziehungsweise die 350-Pixel-Vergleichsliste zurück, soweit Platz besteht.
  Mindestbreiten passen sich kleinen Bereichen an. Relative Präferenzen bleiben
  während der Sitzung über Fenster-, Projekt- und Ansichtswechsel erhalten;
  vorübergehende Mindestgrößen-Clamps überschreiben sie nicht. Reset bei zu wenig
  Platz erhält den Standard für ein später wieder größeres Fenster.
  Bestehende Entities, Navigation, Filter, Cursor, Markierungen, Bereiche,
  Vorschau-IDs, Diff-Aktionen/Faltung/Auswahl und Scrollzustand bleiben erhalten.
  Größenänderungen starten keine Datei-/Vergleichs-/Transferarbeit und warten nicht
  auf I/O. Escape beendet nur den eigenen Resize, auch bei Filter-/Button-Fokus
  außerhalb des Split-Bodys und vor einem weiteren Frame; laufende Arbeit bleibt
  unberührt. Menüs blockieren Resize-Shortcuts; während eines eigenen Press/Drag
  öffnen keine neuen Browsermenüs. Ein Außenklick auf den Trenner schließt das Menü
  ohne Zeilenaktion. Die betroffenen Kontrollzeilen umbrechen bei wenig Platz.
  `pane_split.rs` misst die tatsächliche Fläche statt der Fenstergröße, damit
  Linux-Client-Dekorationen nicht überlaufen. Eine Canvas-Layoutphase verwendet
  Kit-Slots/Handles; stabile Content-Siblings isolieren element-lokalen Button-Fokus
  von erneuerten Gesten-Epochen. Epochen und Gesten-Serien verwerfen alte Callbacks;
  fremde Drags werden nicht übernommen. Persistenz bleibt dem `gui.toml`-Block
  vorbehalten, gemeinsame Go-Konfiguration bleibt unverändert.
- Home/g und End/G springen in beiden Browsern und der Vergleichsliste zu den
  sichtbaren Grenzen. Tab/Shift+Tab wechselt lokal/remote beziehungsweise
  Vergleichsliste/Diff. Browser: / Filter, f lokaler Finder, . Hidden, I lokale
  Ignore-Sichtbarkeit, r Refresh, P Projekte, H Hosts und @ Remote.
  Vergleich: n/p Dateiwechsel auch aus dem Diff, r Refresh, i Include ignored,
  s/S Sync-Bestätigung und Ctrl/Cmd+Enter Ausführen; Escape verwirft die Bestätigung.
  Textfelder behalten ihre Buchstaben, Verwaltungsdialoge stellen Browserfokus wieder her.
  Escape in der Dateiliste leert Browserfilter samt sichtbarer Liste; das programmatische Leeren des
  Kit-Inputs wird explizit in die Listenprojektion übernommen und erhält Markierungen.
- Host-CRUD, Duplizieren, Mappings, globale Serverlinks, projektübergreifender
  Picker und Server-Promotion, Verbindungstest auch ungespeicherter Formulare
  sowie FTPS-Trust-Reset. Konflikte/Teilerfolge bleiben sichtbar.
- Verwaltungslisten mit sichtbarem Cursor nach Projekt-Slug und Host-Eintragsname,
  automatischem Scrollen, Down/Enter vom Filter zur Liste, Pfeilen/j/k und Home/g/End/G. Projekte:
  Enter öffnen, 1–9 die ersten neun sichtbaren Projekte direkt öffnen,
  n/e anlegen/bearbeiten, a archivieren/wiederherstellen, . Archivierte,
  d/Delete entfernen, r neu laden. Hosts: n/e/c/d, t Verbindungstest, r FTPS-Trust-Reset,
  l Link-Picker, F5 Reload und Tab/Shift+Tab Scope-Wechsel. Ctrl/Cmd+S speichert
  Formulare; Default-Enter/y bestätigt Löschen/Trust-Reset, Escape kehrt zur Liste zurück.
  Fokussierte native Buttons behalten Enter/Space statt pauschaler Bestätigung.
  Filter/Formulare behalten Texteingabe; Konflikte behalten Form/Bestätigung.
- Link-Picker: sichtbarer Cursor nach Projekt/Host, Down/Enter vom Filter,
  Pfeile/j/k und Home/g/End/G, Enter wählen, Enter/y Promotion bestätigen,
  Escape zurück, / oder Ctrl/Cmd+F suchen und r neu laden. Reload erhält die
  Identität; auch fehlgeschlagene Globalserverwahl behält Dialogfokus.
  Zertifikate: Reject als Startwahl; Tab/Shift+Tab oder Links/Rechts/h/l wählen,
  Enter bestätigen, Escape ablehnen; Details scrollen bei sichtbaren Buttons.
  Weitere Trust-Entscheidungen sind während laufender Zustimmung gesperrt.
- Sync-Berichte behalten die Zusammenfassung samt Fehlerzahl. `e` in Dateiliste
  oder Diff und der Fehler-Button blenden Details zu Failed-/Unknown-Ergebnissen
  ein; Escape schließt zuerst die Details. Textfilter behalten `e` als Eingabe.
  Refresh und Verbindungsverlust erhalten den Bericht. Ein neuer Sync oder
  Projekt-/Verbindungswechsel verwirft alte Details; Sync-Bestätigung schließt sie.
  Echte SFTP-/FTPS-GUI-Tests prüfen Fehler nach Symlink-Wechsel, Zertifikatswechsel,
  laufenden Sync-Abbruch und Unknown nach Serververlust ohne automatischen Retry.
- Diff: Pfeile/j/k, PgUp/PgDown, Ctrl+U/D, Home/g/End/G scrollen; [/] springen
  zwischen Hunks, Enter/l öffnet sichtbaren Fold, h schließt ihn, c toggelt alle.
  Space wechselt die aktuelle Aktion, A alle gültigen Aktionen samt gefilterten
  Dateien. u/d wählt Upload/Download und öffnet die vorhandene Bestätigung;
  fehlende Quellen/Dateifehler/ungültige Sitzungen starten keine Übertragung.
- Projekt-Anlegen/Bearbeiten/Archivieren/Wiederherstellen/Entfernen, Dashboard,
  Startwiederherstellung, `--dashboard`, `--no-dashboard`, Ordnerargument und
  `--help`. Registrierung schlägt den Git-Root einschließlich Worktrees vor.
  Gleiche Projektwahl/Schließen erhält die Sitzung; Verschieben/Entfernen des
  aktiven Projekts verwirft den alten Root und schließt die Remote-Verbindung.
  Fehlgeschlagener Registry-Write beim Entfernen stellt den Hoststore wieder her.
- CLI-Projektverwaltung ohne GUI-/Runtime-Start: `projects list/add/edit/archive/remove`.
  `open` verwendet die vorhandene Slug-/Namens-/Prefix-/Substring-Suche und prüft
  Pfad und Konfiguration vor dem Fensterstart. `dash` öffnet das Dashboard,
  `version` zeigt die Rust-Paketversion. Hilfe/Version funktionieren ohne Display
  und Config-Verzeichnis. `--` schützt Befehlsnamen und führende Bindestriche als
  Ordnerargumente. Mutationen verwenden die bestehenden Store-Transaktionen;
  Open-Zeitstempel schreibt erst das erfolgreich geladene GUI-Projekt.
  Echte CLI-Prozesse prüfen Fehlercodes, Schreibsperren und unveränderte Projektdateien.
- Datei-Logging ist standardmäßig aus. `--log`/`--debug` und
  `$DRIFT_LOG`/`$DRIFT_DEBUG` übernehmen Go-Priorität und Boolean-Regeln;
  Flags gelten vor/nach GUI- und Verwaltungsbefehlen, `--` bleibt verbindlich.
  Hilfe/Version öffnen auch mit Logging-Flags keine Datei oder Konfiguration.
  Neue Logdateien haben Modus 600, bestehende Dateien werden ergänzt.
  `drift-app/src/logging.rs` besitzt den injizierten Logger samt Hintergrundwriter,
  Fehlerkanal und geprüftem Drain/Flush/Close nach Ende des Befehls/Fensters.
  Ein TaskTracker erfasst App-Aufgaben einschließlich blockierender Arbeit und
  Vergleichsworker. Nach Ende des GUI-Loops wird außerhalb von Update auf Abbruch
  und Transport-Shutdown gewartet, bevor Logging schließt. Ein Headless-Test
  schließt das letzte Fenster während eines echten Uploads und prüft erhaltene
  Unknown-/Stop-/Abschlussdiagnosen ohne Retry.
  Öffnungsfehler warnen und lassen den Befehl weiterlaufen; Schreibfehler
  deaktivieren Logging und bleiben in einem GUI-Banner beziehungsweise auf CLI-stderr
  sichtbar. Browser-/Verwaltungs-/Zertifikats-/Vergleichsansichten behalten das Banner,
  während Fachfehler und Sync-Berichte unverändert bleiben.
  Connect-/Monitor-/Vergleichs-/Sync-Fehler protokollieren Identitäten, sichere
  Pfad-/Endpunktkontexte, Stadien, Kategorien und Ergebniszahlen; keine Auth-Felder,
  Dateiinhalte oder rohen Server-/TOML-Fehlermeldungen. Prozess-/Dateisystemtests prüfen
  Priorität, Append/Rechte, nebenläufiges Drain, FIFO-/Öffnungs-/Schreibfehler und
  Redaction. Echte SFTP-/FTP-/FTPS-Tests prüfen erfolgreiche Abläufe, Auth-/Trust-Fehler,
  Symlink-Wechsel und Unknown nach Serververlust ohne Retry.
- Letzter vollständiger lokaler Lauf: **222 Rust-Tests bestanden, 0 fehlgeschlagen,
  0 übersprungen**, einschließlich echter SFTP/FTP/FTPS-Server und Headless-GPUI.
  Go-Test/Vet/Build, Go/Rust-Prozessparität, Rustfmt, striktes Clippy und
  Linux-Release-Build und display-/Go-freier Release-CLI-Smoke bestanden.
  107 Headless-GPUI-Tests prüfen auch Tastaturfokus,
  Filterrücksetzung sowie bestätigte/abgebrochene Tastatur-Syncs gegen echtes SFTP.
  Numerische Direktwahl prüft alle neun Positionen, gefilterte/archivierte/leere
  Listen, unveränderte Textfelder und tatsächliches Öffnen samt Open-Zeitstempel.
  Verwaltungsprüfungen decken Cursor/Scroll, leere Filter, Archivierung, CRUD,
  Formularfehler/Konflikte, SFTP-Verbindungstest und FTPS-Trust-Reset per Taste ab.
  Picker-/Diff-Prüfungen ergänzen Zielidentität nach Reload/Promotion/Konflikt,
  Zertifikatswahl und Busy-Sperre mit echtem FTPS-Challenge, Detailscrolling,
  Fold-/Viewport-Grenzen, A und bestätigte direkte SFTP-Transfers.
  Baumtests prüfen echtes lokales/SFTP-Laden,
  verschachteltes Collapse, Visibility/Refresh, verschwundene Ordner und veraltete
  Ergebnisse. FTP/FTPS prüfen Aufklappen/Markieren/Zuklappen; der gemeinsame
  lokale/entfernte Scope bleibt auch mit markierten zugeklappten Kindern erhalten. Der Gesamtlauf nutzt
  `TMPDIR=/var/tmp`, weil ein fremdes `/tmp/.git` die Starttests beeinflusst.
  24 neue Menütests prüfen echte lokale/SFTP-/FTP-Abläufe, reine Cursoränderung,
  beide Fokus-Rückwege, Tab-Sperre, Escape und durchgereichte Außenklicks,
  deaktiviertes erstes Item per Down/Enter, Filter-/Baum-/Mapping-Wechsel und
  veraltete Callbacks vor dem nächsten Render. Ein kleineres Fenster prüft
  Randposition und Menüscrolling; ein aus der Virtualisierung verschwundener
  Auslöser behält sein Menü und seinen Pfad. Scope-Tests erhalten Markierungen
  und verlangen weiterhin ausdrückliche Sync-Bestätigung. Native Rendering-/
  OS-Clipboard-Abnahme ist damit noch nicht erbracht.
  16 Resize-Regressionen ergänzen echte lokale/SFTP-Abläufe, aktive Listings,
  Vorschau/Vergleich und einen erfolgreich beendeten Upload ohne Abbruch/zweiten
  Transfer. Sie prüfen Scope/Entities/Filter/Marks/Range/Scroll, Diff-Auswahl/Folds,
  ausstehende Sync-Bestätigung, kleine/inset Flächen und Shrink/Grow, Modal-/Projekt-
  Wechsel, alte Gesten-Callbacks und Menü-Außenklicks. Impliziter Kit-Button-Fokus
  überlebt Keyboard-/Fensteränderungen; echte Input-Komposition über den öffentlichen
  InputHandler und No-Frame-Abbruch verändern keine Texte/Fachaktionen.
  19 zusätzliche Keyboard-Regressionen prüfen native Tab-/Enter-Routen ohne
  manuelles Ziel-Fokussieren, sichere Cancel/Back/Reload/Test-Buttons, inaktive Listen,
  globale/leere/gefilterte Zustände, Default-Enter/y, Vorschau-Quellfokus und Menü-/
  Hilfesicherheit. Echte SFTP-Uploads bestätigen den Plan auch aus Filter-/Run-Button-Fokus.
  Echte FTPS-Challenges/Store-Locks prüfen Reject vor queued Abschlüssen und bereits
  gespeicherte Freigaben ohne Retry/Rollback; Hilfe übersteht realen SFTP-Abbau und
  kommende Zertifikatsdialoge. Clipboard-Prüfungen unterscheiden geladenen/leeren,
  fehlgeschlagenen und ersetzten Inhalt. Filter-Escape erhält Query/Work; Ctrl+Escape
  bricht gezielt ab. Native OS-Abnahme bleibt offen.
  19 weitere Finder-Regressionen (8 App, 11 Headless-GPUI) prüfen Unicode, Ranking/
  stabile Ties, leere/fehlende Treffer, normale Substring-Filter, Hard-Exclusions und
  eingeschränkte Marks. Echte Dateisystemabläufe prüfen gespeicherten Baum/Filter/
  Cursor/History/Root/Range und exakten Scrollzustand, live hinzugefügte/entfernte
  Marks, Busy/Abbruch, queued Index-Abschlüsse, Refresh/Rootwechsel/Invalidierung,
  native Tab-/Enter-/Space-Aktionen, schmale Bereiche und Resize-/Menü-/Hilfesicherheit.
  Echte beschädigte Git-Indizes prüfen Fehler vor sichtbaren Hosts/Hilfe ohne
  versteckten Browserfokus oder Rückkehr zu entfernten Buttons, auch unter macOS.
  Kein impliziter Transfer.
  13 Remote-Ignored-Regressionen (4 App, 9 Headless-GPUI) prüfen Mapping-Priorität,
  Remote-only-/negierte/getrackte Pfade, gecachte FTP-Toggles ohne Netzwerk-I/O,
  native Buttons/Menüs/Tastatur, Filter-/Hilfe-/Popup-Sperren, stale Identitäten,
  Abbruch und Git-Fehler/Refresh. Reale SFTP-/FTP-Verbindungen, Off-tree-Metadaten,
  Hard-Exclusions bei Restore und äußere Rootnamen prüfen Policy und Session-Erhalt.
  Eine weitere App-Regression sendet mehr als einen Pipe-Puffer echter Pfadanfragen
  an Git: früher Prozessabbruch meldet den Indexfehler, nicht nur den sekundären EPIPE;
  erfolgreiche Prozesse müssen weiterhin vollständige Eingaben erhalten.
  6 Hostformular-Regressionen prüfen native Tab-/Shift-Tab-/Enter-/Space-Routen
  anhand sichtbarer Kontrollbounds in kurzen/schmalen/inset Viewports, Add/Remove,
  Auth-/Protokoll-/Linkwechsel, Mausrad ohne Snap-back, verborgene Load-Abschlüsse,
  Shrink/Grow und alte Frame-Geometrie. Echte Stores bleiben unverändert;
  Eingabe-Entities, Maskierung, Entwürfe und letzter Löschfokus werden geprüft.
  Weitere 15 Regressionen (7 echte FTPS-Tools-, 4 Link- und 4 Projekt-Tests) prüfen
  kurze/schmale/inset Kontrollbounds, lange Details/Aktionen, Resize/Mausrad,
  native Tab/Shift-Tab/Enter/Space, Busy-/Zertifikatsabbruch, unveränderte
  Stores/Entities/Snapshots, Bestätigungs-Reopen und nativen Zeileneinstieg.
  Entferntes Reload nach Maus-Test, deaktiviertes Reset nach leerem Reload und
  mehrfaches Reload-Enter sind abgesichert. Ein echter Backendabschluss bei
  ungemounteter Szene erzwingt Enter auf dem frischen Baum vor deferred Restore;
  temporärer Owner und anschließendes `r` dürfen nicht zu Trust-Reset werden.
  38 zusätzliche Eingabe-Regressionen (26 produktive Adapter-/Accessibility-Tests,
  10 echte Store-/Formular-Tests und 2 Shell-/SFTP-Tests) prüfen Raw-Control-Ablehnung
  vor Normalisierung, exaktes Unicode/UTF-16/Undo, Auswahl/Komposition/Fokus/Scroll,
  Readonly und stale Kontext/Clipboard-Ziele. Sie prüfen nativen Paste/Tab/Shift-Tab,
  Escape/Enter, leere IME-Folgecallbacks über Repaint, blockiertes Save/Test/Registrieren,
  problematische gespeicherte Werte ohne Umschreibung, Resize-Escape-Priorität und
  Sync-Bestätigung erst nach Kompositionsende oder explizitem nativen Buttonfokus.
  Diese Tests sind keine OS-IME-/Accessibility-/Permission-Clipboard-Abnahme.
  29 zusätzliche Diff-Regressionen (11 Modell-, 14 Headless-GPUI- und 4 echte
  Shell-/SFTP-Tests) prüfen native Glyphen-Hits, kombinierende/ZWJ-Zeichen,
  Vorwärts-/Rückwärtsdrag, Shift-Tasten, Leerzeichen/Leerzeilen, Upload-Reordering,
  Folds und alte Paint-Identitäten. Deterministische Timer treiben Auto-Scroll
  über gemountete Zeilen hinaus; Release/Escape/Abdeckung stoppen ihn.
  Echter Refresh prüft eingefügte frühere Listenzeilen, geänderte/entfernte Inhalte
  und Dateiwechsel/Help/Resize ohne Transfer. Raw-Events auf dem alten Dispatch-
  Baum prüfen Fokus/Range/Folds/Scroll/Kopie nach Hilfe, Projekte, Hosts und
  Vergleichs-Schließen, einschließlich zuvor gedrücktem Copy-Button und Wheel.
  Native Glyphen-/Clipboard-/Accessibility-Abnahme bleibt separat.

### Native Abnahme und nächster Codeblock

Der Go-/Rust-Keymap-Abgleich ist erfolgt. Vergleichs-/Diff-Navigation, Faltung,
Hunks und bestätigte Transfers sind bereits erreichbar. Browser-Tab bleibt
Pane-Wechsel, Vergleichs-Tab List/Diff-Wechsel; die Vorschau verwendet native
Editor-Tasten. Listen-Paging und ein `q`-Alias sind keine fehlenden Go-Abläufe.
Native Buttons bedienen bereits Protokoll/Auth und Mapping-CRUD; fehlende
Buchstaben-Aliase allein machen diese Kontrollen nicht mausabhängig.

1. **Native Formularabnahme:** Host-/Mapping-Reveal und begrenzte Tools/Links/
   Projektform-Details/Aktionen sind headless umgesetzt. Reale Wayland/X11/macOS-
   Fenster, lange Inhalte und kleine Viewports bleiben praktisch abzunehmen;
   Sichtbarkeit statt nur FocusHandles prüfen. Keine optische Neugestaltung.
2. **Native Eingabeabnahme:** Whole-insertion-Ablehnung und IME-Escape/Enter sind
   headless umgesetzt. Echte Wayland/X11/macOS-IME, exponierte Accessibility-Knoten,
   Clipboard/Permission-Settlement und SDK-Folgecallbacks nach abgelehnten
   Einfügungen bleiben praktisch zu prüfen. Keine Secret-/Pfadbereinigung.
3. **Nächster begrenzter Codeblock:** SFTP-Zielersatz auf eingeschränkten Servern.
   SSH-Hostzertifikate/CA bleiben wegen Parser-/Exchange-Signatur-/Rekey-Problemen
   der Abhängigkeiten gesperrt. Der separate lokale Prototyp ist mit 42 gezielten
   Tests geprüft, aber nicht freigegeben; siehe
   [SSH-Blocker und Wiederaufnahme](rust-ssh-certificate-blockers.md).
   Native Diff-Auswahl, Glyphen-Hit-Tests, Clipboard und Serverlink-Angebote
   bleiben praktisch mit abzunehmen.
4. Jede neue Route headless gegen echte lokale/Remote-Abläufe prüfen: leere/gefilterte
   Listen, Busy/Abbruch, stale Identitäten, erhaltene Auswahl/Scroll und ausdrückliche
   Sync-Bestätigung. Keine Buchstabenbefehle in Texteingaben oder hinter Popups.
5. Pane-/Fensterpersistenz bleibt im späteren `gui.toml`-Block, nicht in Go-Konfiguration.
   Native Plattformabnahme und die optische Überarbeitung bleiben offen.

### Weitere offene Arbeit bis zur Veröffentlichung

**5 größere Arbeitsblöcke bleiben offen.** CLI-Verwaltung, Datei-Logging und
Browser-Kontextmenüs, veränderbare Bereiche, Keyboard-/Dialog-Sicherheit,
Finder-Fuzzy/Rückweg, Remote-Ignored-Sichtbarkeit sowie Formular-Scroll und
Eingabegrenzen, zeichengenaue Diff-Auswahl und automatische Serverlink-Angebote
sind headless umgesetzt. Die verbleibenden Blöcke enthalten mehrere Teilaufgaben und
sind keine gleich großen Zeiteinheiten. Bereits implementierte Kernabläufe
stehen oben.

| Nr. | Arbeitsblock | Fortschritt / nächster verbleibender Schritt |
| --- | --- | --- |
| 1 | SSH-Hostzertifikate/CA | Blockiert: separater lokaler Prototyp mit 42 gezielten Tests; Upstream-Korrekturen für Signer-Decoding, Exchange-Algorithmusprüfung und erneute Prüfung beim Rekey erforderlich. Regulärer Stand weist Zertifikate weiterhin ab; keine Release-Freigabe. |
| 2 | SFTP-Zielersatz | Offen: vorhandenes Ziel auf Servern ersetzen, die den zusätzlichen POSIX-Rename-Kanal ablehnen und keinen passenden Standard-Rename unterstützen. |
| 3 | GUI-Präferenzen und Themes | Offen: `gui.toml` für Fenster/Pane/Theme, Monokai Pro Dark/Light Sun und System/Dark/Light samt laufendem OS-Wechsel. |
| 4 | Native Plattformabnahme | Offen: Wayland/X11, macOS Intel/Apple Silicon; Fokus, Rendering, IME, Accessibility, OS-Clipboard/Permissions und laufende I/O. Headless-Tests ersetzen diese Abnahme nicht. |
| 5 | Pakete und GUI-Releases | Offen: Linux-Paket/Desktop-Eintrag, macOS-App-Bundles, getrennte Release-Wege, Versionseinbettung und Installationsdokumentation. Lokaler Release-Build vorhanden. |

### Orientierung und Prüfbefehle

Die zuständigen Module sind bereits aufgeteilt: `projects.rs` mit
`projects/form.rs` und `projects/management.rs`; `shell/projects.rs` für Start
und aktiven Root; `hosts/form.rs`, `hosts/tools.rs`, `hosts/links.rs` und
`hosts/linking.rs` und `hosts/offer.rs`; `comparison.rs`, `comparison/view.rs`, `comparison/sync.rs`
sowie `diff.rs` mit `diff/{selection,text,interaction,viewport,view}.rs`.
`form_input.rs` samt `form_input/handler.rs`,
`form_input/accessibility.rs` und `form_input/ime_rejection.rs` hält die
Eingabegrenze bei unveränderten nativen Entities. Kontextmenüs liegen in `browser/menu.rs`, `remote/menu.rs` und
`browser_menu.rs`; `shell/comparison.rs` koordiniert ihren Vergleichsscope.
`pane_split.rs` hält ausschließlich GUI-Layout-/Gestenzustand; Shell und
ComparisonPane routen seine Keyboard-/Escape-Aktionen. `shell/keyboard.rs` hält
Vorschau-Routing und Shortcut-Hilfe. `remote/visibility.rs` hält Klassifikations-
Identitäten und gecachte Sichtbarkeit, `drift-app/src/remote.rs::classify_entries`
die Mapping-/Git-Policy. `focus_reveal.rs` misst äußeren Formularscroll ohne neue
Tab-Stopps; `hosts/form.rs` hält die Controls und deren stabile Mapping-IDs.
`browser/finder.rs` hält den gespeicherten
Browser-Rückweg, `drift-app/src/finder.rs` das reine Ranking. `DefaultConfirm` in Verwaltungsdialogen schützt
native Enter/Space-Aktionen. `Ctrl+Escape` ist die explizite Work-Abbruchroute aus Filtern.
Abläufe liegen in `drift-app`, Persistenz/Policies in `drift-core`.
Referenz für Build und Bedienung: [rust/README.md](../rust/README.md).
Rust **1.98.1**, GPUI Kit **0.7.0** und `Cargo.lock` beibehalten.

```sh
make rust-check
make rust-test
make rust-build
make test
make vet
make build
# Cross-process parity must run with the built probe, otherwise Go skips it:
(cd rust && cargo build --locked -p drift-core --example store_probe)
DRIFT_RUST_STORE_PROBE="$PWD/rust/target/debug/examples/store_probe" go test -count=1 ./internal/parity
```

Protokolltests benötigen Go und echte OpenSSH-Werkzeuge (`ssh-keygen`,
`ssh-agent`, `ssh-add`) ausschließlich als Testabhängigkeiten. In der bisherigen
lokalen Umgebung wurden `CARGO_HOME=/tmp/drift-cargo`, `CARGO_BUILD_JOBS=2`
und `TMPDIR=/var/tmp` für Go/Rust verwendet; temporäre Caches vor Wiederverwendung
prüfen. Unterschiedliche Checkouts/Worktrees nicht auf dasselbe Cargo-Target
zeigen lassen. Für manuelles Testen des automatischen Starts das Binary ohne
Ordnerargument starten: `make rust-run` reicht ausdrücklich den Repository-Pfad
weiter und prüft deshalb den Start mit explizitem Ordner.

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
einen Remote-Browser für SFTP/FTP/FTPS in `remote.rs`: Hostauswahl, unabhängige Navigation/History,
Filter, Textvorschau im gegenüberliegenden
Bereich, Abbruch, Shutdown und Beobachtung des Verbindungszustands. Dialoge,
Projektverwaltung und Diff besitzen eigene Views; `Shell` koordiniert ihre
Ereignisse und Ressourcen, während Eingaben/Darstellung in den Child-Views liegen.

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
| 3. SFTP | Auth-Fälle, Remote-Browser, Vergleich, Unified-Diff, alle Sync-Aktionen, Abbruch/Verlust | Kernabläufe implementiert und lokal geprüft; Hostzertifikate/CA, SFTP-Zielersatz auf eingeschränkten Servern, native Tastatur-/Textauswahlabnahme offen |
| 4. FTP / FTPS | Listings, Missing-Klassifikation, TLS/Trust-Dialoge, Keep-alive, Vergleichsparallelität | In Arbeit: nativer FTP-/FTPS-Browser/Vorschau/Vergleich/Sync, Pool bis vier Verbindungen, adaptive Login-Grenze, 550-Prüfung und Keep-alive sowie TLS 1.2, Zertifikatsspeicher und Trust-Dialog vorhanden; vollständige native Plattformabnahme offen |
| 5. Parität | Verwaltung/CLI/Tastatur; Refresh und Sync bauen Vergleich mit erhaltenem Scope neu auf | Teilweise umgesetzt: GUI-Verwaltung, Scope-erhaltender Refresh/Sync, numerische Projektdirektwahl, Sync-Fehlerdetails, CLI-Projektbefehle samt open/dash/version, optionales Datei-Logging und Browser-Kontextmenüs implementiert und lokal geprüft; restliche Tastaturparität offen |
| 6. Veröffentlichung | Linux-Paket/Desktop-Eintrag, macOS-Bundles für Intel/Apple Silicon, Installation und Release-Builds | Lokaler Linux-Release-Build bestanden; Pakete/Bundles, GUI-Release-Wege, Installation und native Abnahme offen |

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
Zwischenablage; Auswahl einzelner Zeichen über Zeilengrenzen bleibt offen.
Die Browser besitzen unabhängige Mehrfachmarkierungen und Bereichsauswahl;
markierte Kinder bleiben beim Zuklappen und Refresh erhalten. Die Aktionswahl
zeigt die Vorschau; Sync selected und Sync all actions führen die bestätigten
Aktionen seriell aus. Die Bestätigung
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
Konflikte, Archivierung, Startregeln und aktive FTP-Sitzungen ab. Die wesentlichen
Cursor-/CRUD-/Formulartasten und numerische Direktwahl des Dashboards sind
umgesetzt; verbleibende Tastaturparität bleibt in Meilenstein 5.

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
