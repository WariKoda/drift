# Drift GUI — Designsystem und Hauptarbeitsfläche

## Ziel und Geltungsbereich

Zed ist die Referenz für eine ruhige, dichte Entwickleroberfläche: Inhalt vor
Chrome, kleine typografische Hierarchie, geordnete Werkzeugleisten und klar
getrennte Flächen. Drift übernimmt diese Prinzipien, nicht Zeds Branding oder
Implementierung. Die Oberfläche bleibt ein Dateivergleichs-/Sync-Werkzeug, kein
Editor-Nachbau.

Der Nutzer hat **Design-Tokens plus Pilotansicht** gewählt und anschließend die
Hauptarbeitsfläche priorisiert. Lokaler und entfernter Browser, gemeinsame Toolbar,
Vorschau-/Diff-Chrome, Vergleichsliste und Statuszeilen nutzen die Tokens.
Code-Schrift und Zeilengeometrie von Vorschau/Diff bleiben unverändert.
Projekt-/Hostverwaltung und Dialoge werden nicht pauschal umgestaltet; bisher ist
nur das Create/Edit-Projektformular auf das neue Dialoglayout übertragen.

Referenzstand: Zed `6ec43d631972c29422e1915bd6fd597bffa5a88d`.
Eine [schematische Vorschau](design/drift-design-system.html) zeigt Rhythmus und
Komponenten; sie ist **kein Screenshot und keine native GUI-Abnahme**.

## Was von Zed stammt — und was Drift entscheidet

| Beobachtung in Zed | Drift-Adaption |
| --- | --- |
| UI-Textgrößen 14 / 12 / 16; UI- und Buffer-Schrift getrennt | 14 für Inhalt/Aktionen, 12 für Metadaten, 16 für kleine Titel. Bestehende Kit-UI-Schrift; Vorschau/Diff behalten ihre Code-Schrift. |
| Eigene Density-Stufen und semantisches Spacing | Ein kompakter Rhythmus mit 4 / 8 / 12 / 16, ohne neue Density-Einstellung. |
| Kleine outlined Icons, normalerweise 16, kleinere Stufen 14/12 | Bereits eingebettete Kit-/Lucide-Icons; keine importierten Zed-SVGs. Browser-Glyphen 16, native Small-Button-Glyphen folgen dem Kit. |
| Toolbar-Fläche, dünne untere Trennlinie, geringe Abstände | Flache Werkzeugleiste mit stabilen Aktionsgruppen statt einer Folge gleich schwerer Buttons. |
| Modal: Header, Body-Sektionen, optionaler Footer und Scrollhandle | Später stabile Header/Footer mit begrenzt scrollbarem Inhalt. Noch nicht implementiert. |

Die Zahlen sind nominale **logische Größen bei 16 px/rem**, keine physischen
Bildschirmpixel. Zeds Typografie-Kommentar nennt an einer Stelle 0.825 rem für 14 px;
der tatsächlich ausgeführte Wert ist **14/16 = 0.875 rem**. Wir orientieren uns am
Code, nicht an diesem widersprüchlichen Kommentar.

## Tokens

Zentrale Implementierung: `rust/crates/drift-gui/src/design.rs`.
Nur im Pilot verwendete Tokens stehen im Code; spätere Dialogmaße bleiben hier
im Entwurf, statt ungenutzte APIs einzuführen.

### Typografie

| Rolle | Basis | Verwendung |
| --- | --- | --- |
| Body | 14 px / 0.875 rem, regulär | Dateinamen, Navigation, normale Aktionslabels |
| Metadata | 12 px / 0.75 rem, regulär | Markierungszähler, Status, Gruppenhinweise |
| Title | 16 px / 1 rem, semibold | Kleine Titel, Drift-Kennung; später Dialogtitel |
| Code | Bestehende Mono-Schrift und Metriken | Vorschau und Diff, in diesem Pilot unverändert |

Keine wesentlichen Bedienhinweise in 10 px. Pfade werden weder normalisiert noch
umgeschrieben; visuelles Kürzen ändert weder Daten, Auswahl noch Clipboard-Inhalt.
Die bestehende Kit-Schrift-/Rem-Skalierung ist die einzige Skalierungsquelle;
Layoutmaße müssen mitwachsen, ohne einen zweiten DPI-Faktor anzuwenden.
Es werden keine Schriftdateien mitgeliefert oder neue Font-Downloads benötigt.

### Abstände und Dichte

| Token | Nominal | Regel |
| --- | --- | --- |
| Tight | 4 | Innerhalb eines Aktionsblocks, Glyphen-/Textabstand |
| Control | 8 | Zwischen Blöcken bzw. innerhalb kleiner Kontrollbereiche |
| Panel | 12 | Seitlicher Einzug von Browser und Chrome |
| Indent | 16 | Pro Baumebene, unabhängig vom Dateinamen |
| Row | 24 | Kompakte Desktop-Dateizeile, mitwachsend bei UI-Skalierung |
| Control height | 28 | Native Toolbar-Aktion; Glyphengröße ist nicht die Klickfläche |
| Header minimum | 36 | Eine Kontrollreihe plus zweimal Tight; bei Umbruch wächst sie |
| Disclosure | 20 | Eigene Spur für Auf-/Zuklappen |
| Icon | 16 | Dateityp-Glyphen; separate Markierungsspur |
| Radius | 4 | Zurückhaltende Kontrollrundung, keine Kartenoptik |
| Separator | 1 | Dünne Trennung von Bereichen |

Die kleinere Row-Dichte ist eine **Drift-Entscheidung**, keine behauptete universelle
Zed-Zeilenhöhe. Sie richtet sich an Desktop-Zeiger/Tastatur, nicht an Touch.
Ein späterer Comfortable-/Touch-Modus wäre ein eigener, noch nicht beschlossener Block.

### Farben und Zustände

Keine neue Palette und keine festen Zed-Hexwerte. Die semantischen Rollen beziehen
sich weiterhin auf den aktuellen Kit-Modus:

| Rolle | Kit-Quelle |
| --- | --- |
| Canvas | `background` |
| Chrome | `list_head` |
| Text / Metadata | `foreground` / `muted_foreground` |
| Separator | `border` |
| Gewählte Dateizeile | `list_active` |
| Hover | `list_hover` |
| Fokus | Native Kontroll-Fokusdarstellung bzw. sichtbare Pane-Markierung |
| Fehler / Warnung / Erfolg | Bestehende semantische Kit-Farben, nicht neu belegt |

Hover ist kein Fokus, Auswahl ist keine Markierung, Fokus ist keine Zustimmung.
Eine gewählte Zeile bleibt gewählt, wenn der Fokus zu einem Filter oder Button geht.
Markierungen behalten ihre separate Spur und funktionieren weiterhin über Bereichs-
und Mehrfachauswahl. Ordner-/Symlink-/Outside-Mappings-Hinweise dürfen nicht allein
von Farbe oder einem dekorativen Icon abhängen.

Settings bietet System/Omarchy/Dark/Light in einem nativen Dropdown. Omarchy liest
nur die aktive Benutzerpalette und folgt ihren Änderungen; System und feste Kit-
Modi bleiben verfügbar. Dieses Designsystem liefert **keine Monokai-Palette** mit;
deren [Weitergabe-Gate](rust-gui-theme-blockers.md) bleibt offen.
Kontrast und sichtbarer Fokus sind mit beiden Kit-Modi und nativ auf den Zielsystemen
zu prüfen. Eine Token-Zuordnung ist noch kein WCAG- oder OS-Accessibility-Nachweis.

## Komponentenregeln

### Werkzeugleiste

```text
Oben:   Home | Projects | Hosts | Open folder | Settings
Links:  Local · Dateien / Vorschau          | Rechts: Remote · Dateien / Vorschau · Host ▾ · Trennen
        Navigation … Cancel                 |         Navigation … Remote cancel
        Local filter                        |         Remote filter
        Local path                          |         Remote path
        Local files/preview                 |         Remote files/preview
        Marked/range status                 |         Marked/range status
Unten:  Status            Compare: Project · Local selection · Remote selection · Marked
```

- Global: Home, Projects, Hosts, Open folder, Settings. Hosts ist Verwaltung wie
  Projects und schließt die geöffnete Hostverwaltung beim zweiten Klick.
- Linke Pane: lokale Navigation/Sichtbarkeit/Cancel, darunter der lokale Filter.
  Die Remote-Navigation und ihr Filter beginnen rechts auf genau derselben Höhe.
- Remote-Pane: Host-Dropdown und Trennen (Unplug-Icon) rechts in der Tabzeile;
  Remote-Abbruch am Ende der Remote-Navigation, wie Cancel links. Wird die Pane
  zu schmal, rutschen Dropdown und Trennen in eine zweite Zeile, statt die Pane
  zu verlassen. Ein eigener gemessener Slot gleicht die Höhe der linken Tabzeile
  an und zieht die Trennlinie an der gemeinsamen Unterkante.
- Fußleiste unter beiden Panes: Status links, die vier Vergleichsscopes zentriert.
  Vergleichen betrifft beide Seiten und gehört deshalb keiner Pane allein.
- Navigation: Back, Forward, Up, Refresh, Find files.
- Visibility: Show/Hide hidden und ignored; bleibt operation-spezifisch.
- Compare erstellt nur den bisherigen Vergleich, niemals eine Transferfreigabe.

Native Small-Buttons mit vollständigen Tooltips und Accessibility-Labels.
Compare-Zugänge verwenden den semantischen Secondary-Stil; Verwaltung, Verbindung
und Navigation bleiben ruhigere Ghost-Aktionen. Keine Scope-Variante wird zur
impliziten Standardaktion. Die obere Toolbar und die Compare-Fußleiste umbrechen
intern. Die Navigationszeilen dagegen bleiben gleich hoch/einzeilig und scrollen
bei knapper Breite horizontal; unterschiedliche Pane-Breiten verschieben dadurch
keinen der nativen Filter. Fünf allgemeine Toolbar-Zugänge bleiben über allen
Ansichten sichtbar; acht lokale Navigations-Icons gehören links, sieben Remote-
Navigations-Icons sowie Host-Dropdown und Trennen ausschließlich zur rechten
Workspace-Pane. Die vier Compare-Knöpfe haben Icon und kurze Beschriftung. Home steht als Icon am Anfang, Settings am Ende der
oberen Toolbar. Home kehrt zum Browser zurück, erhält
aktuelles Verzeichnis/Markierungen und beendet die Finder-Ansicht über deren
bestehenden Rückweg. Ruhende Vergleiche lassen sich schließen; Entwürfe, Transfers,
Sync-Pläne und Sicherheitsabfragen werden nicht umgangen. Back, Forward, Up,
Refresh, Find files, Hidden, Ignored und Cancel sind auf ausdrücklichen Nutzerwunsch
reine quadratische Icon-Buttons mit vollständigen Tooltips und Accessibility-Labels.
Hidden/Ignored zeigen den aktiven Zustand; Projects markiert seine geöffnete
Verwaltung. Remote verwendet dieselbe Icon-Regel für seine Header-/Navigations-/
Sichtbarkeitskontrollen sowie Disconnect. Ein natives Host-Dropdown zeigt die
konfigurierten Namen; die bestätigte Auswahl eines anderen Hosts startet Connect,
nicht das Öffnen/Schließen oder bloße Aktualisieren der Auswahl. Die aktuelle
Verbindung erneut auszuwählen verbindet nicht neu. Compare verwendet unterschiedliche Icons für Project und Marked sowie
vollständige Scope-Namen in Tooltip/A11y. Sync-/Delete- und Vertrauensentscheidungen
bleiben ausdrücklich beschriftet.

Gruppen und Zeilen dürfen bei geringer Breite umbrechen. Keine neue versteckte
Overflow-Aktionsliste, kein verlorener Tastaturweg und kein künstlich fixer
Toolbar-Höhenwert, der Aktionen abschneidet. Vorhandene IDs, Listener, Disabled-
Bedingungen und Stale-Scene-Sperren bleiben maßgeblich.

### Fester Anwendungsrahmen

Die Toolbar gehört zur äußeren `Shell`, nicht mehr nur zur Browseransicht.
Projects, Hosts, Vergleich, Hilfe und Zertifikatsansichten wechseln ausschließlich
den Inhalt darunter. Die obere Toolbar und ihre nativen Kontrollidentitäten
bleiben stehen; Home/Projects schließen bzw. wechseln sichere Verwaltungsansichten,
ohne Browser/Preview/Remote neu anzulegen. Hosts steht in der oberen Leiste; es öffnet
aus dem Browser und schließt eine offene Hostverwaltung, greift aber nicht durch
Hilfe, Zertifikate oder ungespeicherte Formulare. Die alten Remote-
und Local-files-Umschaltbuttons sowie der pane-übergreifende Local-preview-Button
sind entfernt.
Eine über dem Vergleich geöffnete Verwaltung kehrt mit richtigem Fokus dorthin zurück.

`local-workspace` und `remote-workspace` beginnen mit je einer identisch hohen,
beschrifteten Dateien/Vorschau-Tabzeile, danach Navigation und nativem pane-breitem
Filter. Local-Dateien und lokale Vorschau stehen ausschließlich links, Remote-
Dateien und Remote-Vorschau ausschließlich rechts. Beide starten mit Dateien.
`local_preview` und `remote_preview` sind getrennte native Editor-/Request-Entities;
beide Vorschauen können gleichzeitig sichtbar sein. Ein Tabwechsel ändert nur
Sichtbarkeit/Fokus seiner Seite, erhält geladenen Text/Cursor/Selektion und löst
weder Verbindungstrennung noch Transfer aus. Cache-Wiederverwendung prüft Projekt
und angefragte Datei; Refresh sowie Verzeichnis-/Projekt-/Sitzungswechsel
invalidieren den zuständigen Cache. Filter ohne Treffer, Markieren und bloße
Cursorwechsel verwerfen das geladene Dokument nicht. Ein ausdrückliches Datei-
Öffnen liest frisch; reines Umschalten nutzt den Cache. Pfad-Kopieren verwendet
bei Vorschau deren Dokumentpfad, nicht eine inzwischen abweichende Listenauswahl. Vorschau ist nur mit gültiger Dateiauswahl
oder gültigem Seitencache anwählbar. Beide bleiben beim
Ändern von Split-/Fenstergröße und Kit-Schrift auf gleicher Höhe. Beide Köpfe bestehen damit aus Tabs, Navigation und
Filter; links erscheint nur im Finder zusätzlich die Return-Zeile. Die gesamte Kopfhöhe wird mit der tatsächlichen Breite und nativen Typografie
beider Panes gemessen; die größere Höhe gilt für beide. Danach folgen gleich hohe
Pfadzeilen, gleich beginnende Datei-/Editorflächen und feste Statusfußleisten.
Vorschau-Werkzeuge und Dokumentpfad gehören ebenfalls in diese gemeinsamen Slots,
nicht zusätzlich in den Editorbody. Dropdown-Menüs bleiben außerhalb des geclippten
Kopfes bedienbar. So verändern ungleiche Pane-Breiten und umgebrochene Remote-
Aktionen nicht mehr die gegenseitige vertikale Ausrichtung.
Danach steht entweder der Remote-Browser oder seine eigene Remote-Vorschau;
lokale Dateien beeinflussen die rechte Ansicht nicht mehr. Der bestehende Zwei-
Pane-Split bleibt erhalten. Der Kopf prüft Projekt,
Verbindung und Navigations-Epoch vor Callbacks und Pointer-/Key-Ereignissen. Modal-
Eintritt invalidiert ihn schon vor dem nächsten Frame, auch bei Hide/Show-ABA.
Connect/Disconnect in der Remote-Dateiansicht prüfen ihre eigene
aktive Frame-/Verbindungsidentität; Connect akzeptiert nur noch aktuelle Katalog-
Einträge. F5/Ctrl+F/Copy path wirken auf die fokussierte Seite; p/c und Escape
beziehen sich auch bei zwei Vorschauen nur auf diese Seite. Native Editor-Kopie
mit Ctrl/Cmd+C bleibt unverändert. Lokales Cancel stoppt nur lokale Arbeit;
Remote cancel stoppt nur Remote-Arbeit und Remote-Preview. Ctrl+Esc behält seine
bestehende Tastatursemantik. Sicherheits-/Transferentscheidungen werden nicht berührt.

`ToolbarState::allows` und die Root-Handler prüfen die aktuelle Ansicht auch beim
Callback: Entwürfe, Writes, ausstehende Sync-Pläne, Hilfe, Zertifikate und native
Ordnerdialoge sperren unpassende Routen. Die sichtbare Toolbar ist keine Erlaubnis,
im verdeckten Browser zu navigieren. Ihre Browser-Kürzel bleiben bei Buttonfokus
aktiv, greifen aber nicht durch andere Ansichten; F5, Ctrl/Cmd+F und Ctrl+Esc sind
hierfür gesondert geprüft. Native Enter/Space-Bedeutung und explizite Freigaben bleiben.

### Einstellungen und Tastaturhinweise

Das Zahnrad öffnet eine eigene Settings-Ansicht unter der festen Toolbar. Sie
überdeckt die bisherige Ansicht, statt deren Modelle/Sitzungen zu ersetzen;
Schließen stellt Ansicht und gültigen Fokus wieder her. Home führt zum Browser,
Projects wechselt gezielt in die Projektverwaltung; Hosts schließt Settings und öffnet die
Hostverwaltung. Entwürfe, Writes,
Sync-Pläne und Sicherheitsabfragen behalten dieselben Navigationssperren.

`Shell::settings` hält Ansichtszustand, Fokus-/Owner-Identität, ScrollHandle und
die native Theme-Auswahl samt Subscription. Header/Footer sind fest, der Inhalt
scrollt; bevorzugte Inhaltsbreite 720 wächst mit den vorhandenen Tokens.
Appearance bietet System/Omarchy/Dark/Light als natives Dropdown mit demselben
Writer, atomarer Speicherung und sichtbaren Fehlern. Omarchy liest
`$XDG_STATE_HOME/omarchy/current/theme/colors.toml` (sonst unter `~/.local/state`)
asynchron und prüft ungefähr sekündlich auch atomar ersetzte Dateien/Symlinks.
Fehlende oder ungültige Paletten fallen mit sichtbarer, von Speicherfehlern
unabhängiger Warnung auf System zurück und erholen sich ohne Moduswechsel.
Keine Desktop-Konfiguration wird geschrieben oder ausgeführt; reine Farbänderungen
speichern keine Präferenzen und ersetzen keine Editor-/Sitzungs-Entities.
Window and layout erläutert die bereits automatische Speicherung; es gibt keine
funktionslosen Platzhalterschalter. Die Theme-Zeile unten in Browser/Vergleich ist
entfernt; nur Settings enthält die Auswahl. Speicherfehler erscheinen unabhängig
davon als Warnung im Anwendungsrahmen und werden nicht mit der Zeile versteckt.
Es existiert weiterhin nur eine Preferences-Entity und ein Writer. Verdeckte/veraltete Settings-Frames können
weder Theme ändern noch eine neu geöffnete Settings-Ansicht schließen.

Inline-Keymaps, numerische Dashboard-Hinweise, Tastenzusätze an Buttons/Tooltips,
Bereichsauswahl-/Trust-/Sync-Instruktionen sind auf Nutzerwunsch entfernt.
Fachliche Status- und Sicherheitsangaben sowie alle Bindings bleiben erhalten.
Die ausdrücklich aufgerufene Tastaturhilfe bleibt als Nachschlageansicht bestehen,
jedoch ohne zusätzlichen Esc-Hinweis am Schließen-Button.

### Dateibrowser

```text
Pane header / location / native filter
Disclosure | Mark | File/Folder glyph | Name and semantic suffix
Fixed footer: marked count / range state
```

- Gleichmäßige Zeilen und Einzüge in beiden Browsern; weiterhin virtualisiert.
- Dateinamen nehmen den Platz ein; Icons bleiben sekundär und aus dem vorhandenen
  Asset-Bundle. Ordner-Slash bleibt erhalten. Remote-Symlink-Pfeil und ausdrücklich
  beschrifteter `Unmapped`-Hinweis stehen außerhalb der Dateinamen-Ellipse; tiefe
  Baumeinzüge dürfen diese Hinweise nicht nach rechts verschieben.
- Nachgewiesene schmale Layouts: Remote-Panes 210/420 bei UI-Basisschrift 16/24,
  echte Unicode-Symlinks und Tiefe 9, mit nativer Textmessung und Paint-Clip-Prüfung.
  **Offene Layoutgrenze:** Pane 120 bei UI-Schrift 24 benötigt für beide ungekürzten
  Hinweise samt Einzug/Abstand 160; der Badge kann dort über den Viewport reichen.
  Dieser Extremfall ist nicht als vollständig lesbar abgenommen. Es gibt derzeit
  keinen neuen GUI-Font-/Density-Schalter; solche Kombinationen benötigen vor einer
  entsprechenden Freigabe eine weitere responsive Behandlung.
- Die Markierungszahl und der Bereichsauswahl-Status stehen auf Nutzerwunsch in
  einer festen Metadaten-Fußzeile unter jeder Dateiliste. Der lokale Kopf enthält
  nur noch Local/Project files und den Finder-Rückweg. Versteckte/gefilterte Marks
  bleiben im bisherigen Gesamtzähler enthalten; Zähl- und Markierungslogik ändern
  sich nicht. Die Statuszeilen scrollen nicht mit und besitzen eine A11y-Statusrolle.
- Filter sind bestehende native Input-Entities einschließlich IME, History,
  UTF-16-Selektion, Boundary-Validierung und Clipboard-Guards.
- Lokale und entfernte Pfade bestehen aus klickbaren Vorfahren mit `›`-Trennern;
  das aktuelle Segment ist nicht aktivierbar. Lange Pfade sind horizontal scrollbar.
  Die lokale Pfadzeile gehört zur Browser-Pane: unter der Local-Überschrift und
  über der Dateiliste, auf die tatsächliche Pane-Breite begrenzt. Der lokale Filter
  sitzt nun pane-breit direkt unter der lokalen Navigation, auf gleicher Höhe wie
  der Remote-Filter. Die Pfadleiste wandert somit nicht
  über Vorschau oder entfernten Browser und verschwindet mit der lokalen Pane.
  Ziele behalten die ursprünglichen PathBuf-/POSIX-Werte; nur Labels dürfen lossy sein.
  Registrierte Projektgrenzen und die vorhandene Remote-Session-Pfadprüfung bleiben.
- Maus-Zurück/Vorwärts wirkt im jeweiligen sichtbaren Browserbereich bzw. Header
  auf dessen bestehende Historie. Listing/Tree-Busy, Menüs, versteckte Ansichten und
  überholte Projekt-/Verbindungs-/View-Identitäten sperren alte Callbacks. Modal-
  Eintritt deaktiviert diese Routen schon vor dem nächsten Frame; Hide/Show erhält
  eine eigene Revision. Finder-Back behält die bestehende Rückkehrsemantik.
- Einfacher Linksklick auf Datei/Ordner selektiert nur, ohne automatische Vorschau.
  Doppelklick auf eine Ordnerzeile klappt den Zweig auf/zu; er wechselt nicht hinein.
  Auch Datei-Doppelklick selektiert nur. Beide Zeilen füllen die verfügbare Pane-
  Breite; freie rechte Fläche und Einrückung gehören zur Auswahl-/Hover-Fläche.
  Vorschau erfolgt explizit per Tab/Tastatur; Alt+Enter/Kontextaktion wechselt in
  einen Ordner. Mehrfachauswahl wie im Dateimanager, getrennt von Marks:
  Ctrl/Cmd-Klick nimmt Datei oder Ordner in die Auswahl auf oder heraus,
  Shift-Klick wählt alle sichtbaren Zeilen zwischen Anker und Klickziel,
  Shift+Ctrl/Cmd fügt den Bereich hinzu. Modifizierte Klicks markieren und klappen
  nie; Disclosure-Pfeile behalten die Auswahl. Space und Kontextmenü-Mark wirken
  auf die ganze Auswahl, Copy path kopiert alle Pfade, Local/Remote selection
  vergleicht ohne Marks die Auswahl. Tastaturbewegungen wählen eine Zeile;
  Shift-Pfeile markieren weiter. Die Fußleiste nennt ab zwei Zeilen die Auswahl. Laufende Expansion darf verworfen werden, ohne einen
  alten Zweig wieder einzufügen oder die Remote-Sitzung zu schließen.
- Baum-Disclosure, Marks, Scroll und Kontextmenüs ändern ihre Semantik nicht.
  Keine Netzwerkoperation durch einen Style-/Themewechsel.
- Host-/Mapping-/Transferbedingungen sind keine visuellen Freigaben. Ein schönerer
  Button darf niemals eine erneute Transferbestätigung ersetzen.

## Hauptarbeitsfläche — Vorschau und Vergleich

Branch `feature/rust-workspace-design` erweitert das vorhandene Browserdesign:

- Vorschau: kompakte native Copy-Buttons mit sichtbarem Read-only-Hinweis,
  separate Metadatenzeile für den Pfad. Lange Titel werden nur visuell gekürzt;
  Tooltip, Accessibility-Label und Copy path behalten den vollständigen Wert.
  Die native Editor-Entity und ihre Schrift-/Zeilengeometrie bleiben erhalten.
- Vergleich: getrennte Ansichts-/Navigationszeile und Plan-Zeile. Sync selected/all
  verwenden Secondary und öffnen weiterhin nur den Plan; erst der ausdrücklich
  beschriftete Primary-Button Run sync führt aus. Native Handler und Guards bleiben.
- Dateiliste: tatsächliche Parent-Breite wird während Layout/Prepaint gemessen,
  ohne die native Filter-Entity oder den ScrollHandle auszutauschen. Files, shown
  und total sind separat umbrechende Kopfzeilenelemente; Header/Filter schrumpfen
  nicht zugunsten der Liste. Bei weniger als 240 skalierten Breitenpixeln stehen
  Status und Aktion untereinander. Breite Rows bleiben nominal 48 hoch; schmale
  beginnen bei 80 und wachsen nach nativer Textmessung sämtlicher Status-/Aktions-
  labels, auch für zweizeiliges Delete remote. Nur Pfade werden ellipsiert.
  Tooltip und Accessibility-Label behalten Pfad, Status und Aktion vollständig.
- Beim Wechsel von Schrift oder Dichte bleibt der relative Dateiscroll-Anker
  erhalten, einschließlich einer im selben Frame ausstehenden Nearest-Navigation;
  strikte Scrollanforderungen überschreiben ihn weiterhin gezielt. Hierfür wird
  ausschließlich die vorige Row-Höhe im View gespeichert, keine neue Präferenz.
  Auswahl/Hover verwenden dieselben Rollen wie der Browser; Dateiaktionen ändern
  sich nicht.
- Diff: kompakter Toolbar-Chrome und Richtungsmetadaten, sichtbarer Pane-Fokus.
  Monospace, 25er-Codezeilen, Text-Hit-Tests, Auswahl, Faltung und Kopie sind nicht
  umgestellt. Eine eigene skalierbare Code-Dichte wäre ein separater Arbeitsblock.
- Sync-Bestätigung: Zusammenfassung über einer umbrechenden Aktionszeile statt
  neben Buttons auf eine schmale Restbreite gedrängt. Run sync/Cancel sowie
  bestehende Bestätigungs-, Busy- und Tastaturrouten bleiben erhalten.
- Die Shell-Statuszeile verwendet im Browser und Vergleich dieselbe Darstellung.

Drei neue Headless-GPUI-Regressionen prüfen echte lokale Dateien und einen echten
SFTP-Server: lange Unicode-Pfade, 320/420/900/1200er-Fenster, Kit-Schrift 16/24,
Dark/Light, Kontrollbounds, native Editor-/Filter-/Diff-Identität, Auswahl/Faltung,
Copy und erhaltene ausstehende Sync-Bestätigung ohne Dateimutationen.
Die Resize-Tests aktualisieren vor der Layoutprüfung die tatsächlichen Window-Bounds.
Vollständige lokale Prüfung: **463 Rust-Tests bestanden, 0 fehlgeschlagen oder
übersprungen**, Rustfmt, striktes Workspace-/All-target-Clippy und GUI-Debug-Build.
Das ist keine beliebige Viewport-/Schriftfreigabe. Ergänzend wurde der Debug-Build
begrenzt nativ unter Wayland/Scale 1 mit Vorschau, echtem FTP-Vergleich,
ausstehender Sync-Bestätigung, 1200×800/420×800 und Kit Dark/Light geprüft;
[Aufnahmen, Befunde und Grenzen](rust-workspace-native-check.md). Kein Transfer
ausgelöst. Diese Aufnahmen zeigen den ersten Durchgang, nicht den folgenden
Toolbar-/Schmalpane-Fix.

Der Folgeblock ist mit **464 Rust-Tests** (0 Fehler/Ignore), Rustfmt, striktem
All-target-Clippy und Debug-Build geprüft. Die neue Regression verwendet 63 echte
SFTP-/Dateipaare, physische 120er-Panes auch im breiten Fenster, Kit-Schrift 16/24/32,
beide Themes, native umgebrochene Glyphenmaße und tatsächliche Paint-Clips. Sie
prüft Delete local/remote und Remote only ohne abgeschnittene Metadaten, getrennte
Shown-/Total-Zähler nach echtem Filterinput, Entity-/Entscheidungs-/Fokuserhalt
und denselben sichtbaren Scroll-Anker bei gleichzeitig vorgemerkter Navigation.
Die allgemeinen/lokalen und Remote-Pane-Aktionen werden auf Grenzen, Disabled-Inertheit und nun
auch auf vollständige native Tab-Reihenfolge geprüft. Kein Transfer wird gestartet.

Ein erneuter nativer Versuch konnte den Ziel-Fensterfokus nicht sicher herstellen;
der Guard verhinderte Eingaben/Aufnahmen. Der Testbuild ist daher **nicht neu nativ
abgenommen**. Details stehen im [Nachtrag](rust-workspace-native-check.md#folgeblock-toolbar-und-schmale-dateipanes).
Sehr kurze oder beliebig kleine Viewports, weitere Dialoge und die vollständige
Plattformabnahme bleiben offen.

Der anschließende Nutzer-Feedbackblock für feste Toolbar, Icon-Navigation,
Breadcrumbs, Maustasten und anschließender Home-/Pfadzeilenkorrektur ist vollständig
lokal geprüft, einschließlich Settings, Omarchy, nativen Dropdowns, ausgerichteten
Pane-Köpfen und entfernter Inline-Hinweise:
**557 Rust-Tests**, 0
Fehler/Ignore, Rustfmt, striktes All-target-Clippy und GUI-Debug-Build bestanden.
Neue Fälle verwenden reale lokale Verzeichnisse sowie SFTP/FTP, lange Unicode-
und unverändert übergebene Nicht-UTF-8-Ziele, Root-Grenzen, Historie, Busy/Hidden-
und stale Frame-Abwehr. Git-Klassifikation weist Nicht-UTF-8 weiterhin ab; der
Native-Target-Fall nutzt deshalb Show ignored, ohne die Policy zu ändern.
Root-Fälle prüfen feste Toolbar über Verwaltungsansichten, Form-/Help-/Zertifikats-
und Sync-Plan-Sperren, Session-/Markierungserhalt und Tastatur bei Toolbarfokus.
Der Folge-Review fand einen verlorenen Browser-Shortcut-Kontext nach dem Umhängen
der Toolbar; ein eigener bewachter Kontext und eine Regression beheben ihn.
Die früheren nativen Aufnahmen bilden diesen neuen Anwendungsrahmen nicht ab;
eine neue native Sicht-/Plattformabnahme wird damit nicht behauptet.

Der Home-Button verwendet ausschließlich das zusätzliche House-SVG aus dem bereits
gepinnten Kit-Assets-Paket. `drift-gui/src/assets.rs` komponiert das ausgewählte
Einzelasset mit dem unveränderten Standardbestand, ohne Komplettkatalog, kopierte
SVGs oder zusätzliche Abhängigkeit. Ein Asset-Test prüft diese Grenze. Neue Pane-
Geometrie-/Navigationstests sichern Pfadcontainment, Theme/Resize ohne Zustandsverlust,
natives Breadcrumb-Enter, Maushistorie und Home aus Verwaltung/Finder ab. Der
bestehende reale Vergleichstest prüft Home erst nach expliziter Planverwerfung.
Vier zusätzliche Settings-Fälle prüfen reale Theme-Speicherung, erhaltene SFTP-
Sitzung/Marks/Filter, Rückkehr in Projects/Hosts, Home, Hilfe-/Fokuswechsel,
640×480/420×600-Footer-Bounds, versteckte Browserkürzel und alte Pointer-Releases.
Ein im Review gefundener Fokusfall nach echtem Remote-Preview-Verbindungsabbau
ist korrigiert: geänderte Pane-/Preview-Sichtbarkeit verhindert die Rückkehr zum
verdeckten Editor. Theme-Callbacks prüfen zusätzlich ihre Settings-Identität,
auch wenn eine Aktion ohne physischen Pointer über einen alten Knoten eintrifft.
Fünf zusätzliche Remote-Pane-Regressionen prüfen getrennte native Toolbar-Scopes,
Bounds/Tab-Reihenfolge/A11y/Inertheit, Preview-/Host-/Home-Rückkehr mit echter SFTP-
Sitzung, alte Pointer-Releases einschließlich Hide/Show sowie getrennte Abbrüche.
Ein im Review gefundener indirekter Abbruch einer Remote-Vorschau über lokale
Selection-Events ist korrigiert und nach Verarbeitung der Event-Queue geprüft;
Keyboard-Abbruch kehrt ausdrücklich zu einer sichtbaren Browseransicht zurück.
Elf weitere Regressionen decken den jüngsten Folgeblock ab: sechs Ordnerklick-
Fälle mit realem Local/SFTP/FTP, drei native Toolbar-/Header-/Filter-Fälle, eine
Mouse-History-Prüfung und ein echter Preferences-Schreibfehler ohne Theme-Footer.
Die paarigen Zeilen bleiben bei Split-/Font-/Fensterwechseln ausgerichtet. Nach
Verschieben der Header wurde ein im Review gefundener Maus-Back/Forward-Verlust
korrigiert; Navigation und Filter sowie der rechte Aktionskopf leiten gestempelte
Ereignisse in genau die eigene Historie weiter, ohne Doppelauslösung oder
Sitzungswechsel. Die Breiten-/Font-Fälle sind Headless-Prüfungen, keine neue native
Sichtabnahme.
Ein weiterer realer SFTP-GUI-Fall prüft die festen Auswahl-Fußzeilen mit markiertem
Ordner und Datei, Filter, Scrollen, Fenster-/Fontwechsel und aktiver Bereichsauswahl.
Elf zusätzliche Fälle prüfen feste Seiten und beschriftete Tabs, gleichzeitige
unterschiedliche native Dokumente, Cache-/Entity-Erhalt, getrennten Abbruch,
Fokus/Kopie, Projekt-/Verbindungsinvalidierung, Header-Geometrie und Consent-Sperren.
Die Review-Funde zu Refresh-Bypass, Filter-Cacheverlust und falschem Copy-Pfad sind
korrigiert und durch einen echten SFTP-Workflow mit Dateiinhaltswechsel abgesichert.
Connect/Disconnect stehen im Kopf auch während Remote-Vorschau bereit; die dafür
getrennte Workspace-/Dateilisten-Aktivierung behält die Modal-/Stale-Guards bei.
Lange Hostnamen werden auch im eingebetteten Verbindungsbereich begrenzt. Alle
Befunde hier sind automatisiert/headless; eine neue native Sichtabnahme ist offen.
Fünf weitere Auswahl-/Pointer-Regressionsfälle und migrierte Navigationsfälle
prüfen den korrigierten Einfach-/Doppelklick-Vertrag: keine automatische Vorschau,
Ordner-Doppelklick klappt nur auf/zu, vollständige Zeilen-Hitflächen einschließlich
rechter Leerfläche/Einrückung sowie flächige Hover-/Auswahlhintergründe. Reale
Local-/SFTP-/FTP-Fälle prüfen auch verworfene Expansion/Klassifizierung ohne
Sitzungsverlust; explizite Vorschau und Alt+Enter bleiben gesonderte Aktionen.

Der Omarchy-/Dropdown-/Ausrichtungsblock ergänzt reale Palette-Dateien mit Inhalts-,
atomaren Datei-/Verzeichnis-/Symlinkwechseln, Modus-/Größenvalidierung und Fehler-
Erholung. Native Picker-Fälle prüfen Auswahl/Persistenz, ausdrücklichen Schreib-
Retry, Abbruch ohne I/O, zweites Hostziel, erhaltene gleiche Verbindung und alte
Bestätigungen bei Busy-/Projekt-/Katalog-/Modalwechseln. Zurückkehrende Help/Settings
stellen keinen inzwischen ersetzten Host-Trigger scharf; auch Pointer-Releases
auf außerhalb des Trigger-Hitbereichs liegende Popup-Zeilen sind abgesichert.
Die Root synchronisiert den neuen Workspace-Scope vor dem Aufbau der Kontrollen,
nicht erst über einen während des Zeichnens unterdrückten Observer. Header-Messungen
planen ihre Folgeframes nach Prepaint und erzwingen die tatsächliche Pane-Breite.
Geometriefälle prüfen Pfad-/Datei-/Editor-/Footer-Alignment bei ungleichen Splits,
Schriftwechsel und Dateien/Vorschau-Kombinationen sowie einseitig expandierende
Filterwarnungen. Diese Nachweise sind headless, keine neue native Abnahme.

## Dialoglayouts — Projektformular-Pilot, weitere Dialoge Entwurf

Zeds generisches `Modal` gibt Slots und optionales Body-Scrolling vor, **keine globale
Breite**. `AlertModal` verwendet 440 logische px; eine konkrete Sicherheitsabfrage
verwendet 40 rem, nominal 640 px. Daraus entstehen eigene bevorzugte Drift-Größen:

| Klasse | Bevorzugte Breite | Beispiel |
| --- | --- | --- |
| Confirmation | 440 | Explizite, überschaubare Aktionsbestätigung |
| Form | 560 | Kurze Projekt-/Hostdetails |
| Wide details | 640 | Zertifikats-/Linkdetails, größere Formabschnitte |

Das sind bevorzugte Größen, **keine Mindestbreiten**. Nutzbarer Fensterinhalt ist
maßgeblich; auf kleinen Viewports schrumpft der Dialog und der Inhalt scrollt.
Keine Positionierung anhand eines vermuteten Titlebar-/Dekorationsmaßes.

```text
Header: Titel 16, Erklärung 14
Body: Felder/Sektionen; nur dieser Bereich scrollt
Footer: Cancel / sekundär, danach ausdrücklich benannte Hauptaktion
```

- Gemeinsamer horizontaler Einzug 16; Abschnittsabstand 16; Label/Helper-Abstand 4.
- Header oben 16/unten 8; Body vertikal 16; Footer vertikal 12, Aktionsabstand 8.
- Normalerweise 24 Außenabstand, auf kleinen Viewports 16; kein unzugänglicher Footer.
- Primäraktion beschreibt den Vorgang, z. B. Upload/Delete statt OK; Destruktion
  wird zusätzlich im Text ausgedrückt, nicht nur rot eingefärbt.
- Fokusbesitz, Tab-Reihenfolge, IME-Abschluss und sichere Rückkehr zum Invoker bleiben
  explizit. Zeds generische Enter-/Dismissal-Regeln werden nicht auf Drift übertragen.
- Kein implizites Sync, Retry oder Vertrauen durch Enter, Focus-Fallback oder
  Außerklick. Abbrechen einer Ansicht ist nicht automatisch Abbrechen einer Operation.

### Umgesetzt: Projekt anlegen/bearbeiten

Der Nutzer wählte als nächsten Pilot ausdrücklich das **Projektformular**, nicht
alle Dialoge. `projects/form/view.rs` setzt diese Regeln für Create/Edit um:

- Bevorzugte Breite 560 bei Kit-Basis 16, einmal skalierbar, an tatsächliche
  Parent-Content-Bounds geklemmt und darin zentriert. Canvas verwendet öffentliche
  Layout-/Prepaint-/Paint-APIs; kein geschätztes Fenster-/Titlebar-Maß.
- Titel `Create project`/`Edit project`, getrennte sichtbare Feldlabels; fester
  Header und Footer, ausschließlich `project-details` scrollt. Native Eingaben
  behalten Entities, IDs, Composition/History/UTF-16-Selektion und Boundary-Guards.
- Footer visuell und per Tab: **Name → Path → Cancel → Save project**; Rückwärts-Tab
  entsprechend. Das ist eine bewusste Reihenfolgeänderung gegenüber dem alten
  Formular. `Ctrl/Cmd+S` bleibt explizit; Enter im Input/Fokus-Fallback ist keine
  Speicherung. Save/Cancel behalten IDs, Busy-/Writing-Regeln und bestehende
  Callback-/Validierungssemantik, einschließlich versteckter/blurred Preedit-Felder.
- Normaler Felder-/Chrome-Einzug 16, Footer vertikal 12; außen 16, bei schmalem
  Parent 8. Bei weniger als 240 skalierten Höhenpixeln: kein äußerer Rand,
  Erklärung entfällt, Innenabstand 8 und Chrome vertikal 4. Das sind eigene
  responsive Pilot-Anpassungen, keine kopierten Zed-Layoutregeln.
- Ein neues Formular setzt seinen Body-Scroll zurück; Redraw, Typing, Themewechsel
  und Idle tun dies nicht. Vorhandener FocusReveal bewegt weiter nur zur nächsten
  sichtbaren Kante bei Fokus-/Viewportänderung; Header/Footer bleiben beim Scrollen.
- Registry-Liste, Auswahl, Registrierung und Löschansicht bleiben optisch und
  funktional unverändert; kein neues Preference-Feld, keine SDK-/Dependencyänderung.

Implementierung `029d6d7`, [Draft-PR #98](https://github.com/WariKoda/drift/pull/98),
aufgebaut auf dem korrigierten #97-Stand `f8e5b94`. Lokale Gesamtprüfung: **460 Rust-
Tests, 271 GUI-Binärtests einschließlich 238 Headless-GPUI**, ohne Fehler/Ignore.
Rustfmt, striktes Clippy, Release, display-/Go-freier CLI-Smoke, Store-/Go-Parität,
Go-Test/Vet/Build, 27 Python-Paket-Tests (ein privilegierter Realfall ohne Root
übersprungen) und actionlint bestanden. Alle zehn Exits unter
`/var/tmp/drift-project-dialog-verified-*` sind null. Alle sechs Child-CI-Checks
für `e4fc6c0` bestanden:
[37627684353](https://github.com/WariKoda/drift/actions/runs/37627684353) und
[37627674302](https://github.com/WariKoda/drift/actions/runs/37627674302).
HTML neu headless gerendert und visuell geprüft, weiterhin kein natives GPUI-Bild.
Separate echte Fensteraufnahmen liegen in der begrenzten nativen Prüfung unten.

Sieben neue GPUI-Fälle prüfen tatsächliche Parent-Inset-/Dialog-/Footer-Bounds,
Native-Tab/Cancel ohne Writes, Save mit realem Store erst nach Absicht, Busy-Schutz,
Theme-/Font-/Resize-erhaltene Inputs/IME/History/Fokus/Operation sowie Fußleisten-Wrap.
Bestehende Project-/Input-/Scroll-/Keyboard-Prüfungen bleiben erhalten; Form-Footer-
Assertions beziehen sich jetzt auf den festen Footer, Delete weiter auf den Body.
Read-only Scope-Review: kein neuer handlungsbedürftiger Blocker; kein eigener Testlauf
oder native Abnahme durch den Reviewer.

**Gemessene Grenzen:** Parent `320×120`, Inset 8, Kit 16 lässt einen fokussierten
28er-Input plus Footer vollständig zu. Bei Kit 24 und gleicher 104er nutzbarer Höhe
bleiben 21 für einen 42er-Input: Aktionen sichtbar, Input nur an oberer Kante revealed,
**nicht vollständig lesbar abgenommen**. Font 24 bleibt Test-API, kein GUI-Schalter;
keine Garantie für beliebig kleine Viewports/Schriften. Native Plattform-/IME-/
Accessibility-/Glyph-Abnahme bleibt separat. Host-/Trust-/Transferdialoge sind
weiterhin nur Entwurf; Bestätigungsbesitz und Transferfreigaben werden nicht verändert.

### Ergänzende native Wayland-Prüfung

Create/Edit wurde am 7. Oktober 2026 in echten Hyprland-/Wayland-Fenstern bei
Skalierung 1 mit Kit Dark und Light geprüft. Native Tab-Reihenfolge, ausgewählte
Reverse-Tab-Wege, Cancel/Escape ohne Registryänderung, Input-Enter ohne Save,
Validierungsfehler, explizites Create/Edit-Save und Linux-Undo/Redo funktionierten.
Bei 320×120 bleiben fokussiertes Feld und Footer sichtbar; Tab revealt das andere
Feld im begrenzten Body. Die bekannten größeren Schrift-/Extremfälle sind dadurch
nicht freigegeben. Reale IME, Accessibility, Clipboard, Pointer/Wheel, HiDPI,
X11/macOS und laufende Theme-/OS-Wechsel bleiben offen.

[Befunde und native Aufnahmen](rust-project-dialog-native-check.md) halten Umgebung,
Binary-Hash, geprüfte Registryänderungen, unterbrochenen zusätzlichen Build und
verbleibende Grenzen getrennt von Headless-Tests/CI fest. Keine Produktionsänderung
und keine gesamte Plattform- oder Release-Freigabe.

## Umsetzungs- und Abnahmegrenzen

Der Pilot ist ein eigener Feature-Zweig. Keine SDK-Forks, Vendor-Pakete, neuen Fonts,
Go-Änderungen, Shared-Config-Änderungen oder neuen Preference-Felder. Die bekannten
SSH-/Hostzertifikats-, Monokai-, native Plattform- und Performance-Gates bleiben offen.

Implementierung: [Draft-PR #97](https://github.com/WariKoda/drift/pull/97),
Code-Commit `94972c8`, auf dem korrigierten Paketierungsstand `55b73ad`.
Lokale Gesamtprüfung: 453 Rust-Tests, davon 231 Headless-GPUI (264 GUI-Binärtests);
Rustfmt, striktes Clippy, Release-Build, CLI-Smoke, Store-/Go-Parität und Go-Prüfungen
bestanden. Paket-Tests: 27 bestanden, ein privilegierter Realfall ohne Root übersprungen.
Read-only Folgeprüfung bestätigt die Korrektur des Namens-/Hint-Clipping-P2 im
getesteten/default Layout; sie ist von Testläufen und der genannten Extremgrenze
getrennt. Native GUI-Abnahme bleibt offen.

CI-Nachprüfung für Design-Head `973db61`: beide Go-/Linux-Jobs bestanden, beide
macOS-Jobs scheiterten erst im nicht abgenommenen 120er-Pane-/24er-Font-Extremfall:
Der Test versuchte ein Wheel-Ereignis auf einer unsichtbaren Dateiansicht. Die
210/420er-Prüfungen einschließlich nativer Textbreiten und Paint-Clips bestanden
bereits in beiden macOS-Läufen
([37538741980](https://github.com/WariKoda/drift/actions/runs/37538741980),
[37538736895](https://github.com/WariKoda/drift/actions/runs/37538736895)).
Die Testkorrektur ermittelt die Hinweisbreite im letzten sichtbaren 24er-Font-Frame,
prüft danach weiterhin tatsächliche 120er-Pane-Breite, unveränderte Rem-Größe,
fehlende Platzreserve und erhaltenen Fokus/Session-/Mark-/Auswahlzustand. Kein Wheel
oder virtuelle Row-Abfrage auf unsichtbarer Fläche; keine Sleeps, Retries, Skips,
Timeout-Erhöhung oder Produktionsänderung. Das bleibt eine Platzgrenze, **keine**
Lesbarkeits-/Paint-Abnahme des Extremfalls. Alle sechs korrigierten Go-/Linux-/macOS-
Checks bestanden für `f8e5b94`
([37620612116](https://github.com/WariKoda/drift/actions/runs/37620612116),
[37620605228](https://github.com/WariKoda/drift/actions/runs/37620605228)); keine
native GUI- oder beliebige Viewport-Abnahme dadurch.

Weiter nativ prüfen: lange Namen, kleine Fenster, große Listen, UI-Skalierung/HiDPI, Light/Dark,
Tastatur ohne Maus, Hover/Fokus/Auswahl, Disabled-Aktionen und Themewechsel während
Listing/Vorschau/Vergleich. Bestehende IME-, Modal-, Clipboard-, Resize-, Stale-Result-
und Transfer-Regressionsfälle bleiben Teil der Gesamtprüfung.

## Quellen und Lizenzgrenze

Alle Code-Referenzen sind auf den oben genannten Zed-Commit gepinnt:

- [Typografie](https://github.com/zed-industries/zed/blob/6ec43d631972c29422e1915bd6fd597bffa5a88d/crates/ui/src/styles/typography.rs)
- [Spacing](https://github.com/zed-industries/zed/blob/6ec43d631972c29422e1915bd6fd597bffa5a88d/crates/ui/src/styles/spacing.rs)
- [Icon-Richtlinien](https://github.com/zed-industries/zed/blob/6ec43d631972c29422e1915bd6fd597bffa5a88d/crates/icons/README.md)
- [Toolbar](https://github.com/zed-industries/zed/blob/6ec43d631972c29422e1915bd6fd597bffa5a88d/crates/workspace/src/toolbar.rs)
- [Modal](https://github.com/zed-industries/zed/blob/6ec43d631972c29422e1915bd6fd597bffa5a88d/crates/ui/src/components/modal.rs)
- [AlertModal](https://github.com/zed-industries/zed/blob/6ec43d631972c29422e1915bd6fd597bffa5a88d/crates/ui/src/components/notification/alert_modal.rs)
- [Security-Beispiel](https://github.com/zed-industries/zed/blob/6ec43d631972c29422e1915bd6fd597bffa5a88d/crates/workspace/src/security_modal.rs)
- [UI-Checkliste](https://github.com/zed-industries/zed/blob/6ec43d631972c29422e1915bd6fd597bffa5a88d/docs/src/development/ui-checklist.md)

Zeds UI-/Icon-Crates deklarieren GPL-3.0-or-later; GPUI ist separat Apache-2.0.
Das macht Zed-UI-Code nicht automatisch frei von den UI-Lizenzbedingungen. Wir
übernehmen hier nur beobachtete Designprinzipien und implementieren die Drift-
Konventionen unabhängig. Keine übernommenen UI-Makros, SVGs, Fonts, Paletten oder
Shadow-Rezepte. Eine zukünftige Asset-/Codeübernahme benötigt eine eigene Provenienz-
und Lizenzprüfung.
