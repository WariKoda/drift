# Drift GUI — Designsystem, Browser-Pilot

## Ziel und Geltungsbereich

Zed ist die Referenz für eine ruhige, dichte Entwickleroberfläche: Inhalt vor
Chrome, kleine typografische Hierarchie, geordnete Werkzeugleisten und klar
getrennte Flächen. Drift übernimmt diese Prinzipien, nicht Zeds Branding oder
Implementierung. Die Oberfläche bleibt ein Dateivergleichs-/Sync-Werkzeug, kein
Editor-Nachbau.

Der Nutzer hat **Design-Tokens plus Pilotansicht** gewählt. Der Code-Pilot umfasst
lokalen und entfernten Browser, die gemeinsame Toolbar und die Browser-Statuszeile.
Diff, Vorschau, Projekt-/Hostverwaltung und Dialoge werden nicht pauschal umgestaltet.
Die Dialogregeln weiter unten sind ein Entwurf für spätere Schritte.

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

Light/Dark/System bleiben erhalten. Dieses Designsystem implementiert **keine
Monokai-Palette**; deren [Weitergabe-Gate](rust-gui-theme-blockers.md) bleibt offen.
Kontrast und sichtbarer Fokus sind mit beiden Kit-Modi und nativ auf den Zielsystemen
zu prüfen. Eine Token-Zuordnung ist noch kein WCAG- oder OS-Accessibility-Nachweis.

## Komponentenregeln

### Werkzeugleiste

```text
Drift | Project | Session | Compare | Navigation | Visibility | Cancel
```

- Project: Projects, Hosts, Open folder.
- Session: Remote, Local files.
- Compare: Project, Local selection, Remote selection und Marked. Der bestehende
  Marked-Workflow erhält einen zusätzlichen beschrifteten Toolbar-Zugang; alle
  bisherigen 16 Kontrollen bleiben erhalten. Das ist keine neue Transferfreigabe.
- Navigation: Back, Forward, Up, Refresh, Find files.
- Visibility: Show/Hide hidden und ignored; bleibt operation-spezifisch.
- Cancel bleibt ausdrücklich beschriftet und an dieselben Operationen gebunden.

Flache native Ghost-/Small-Buttons, klare Labels und zusätzliche Icons. Compare-
Buttons dürfen kürzere sichtbare Labels verwenden, behalten aber volle Scope-Namen
als Accessibility-Label und Tooltip. Keine icon-only Transfer-/Delete-Aktionen.

Gruppen und Zeilen dürfen bei geringer Breite umbrechen. Keine neue versteckte
Overflow-Aktionsliste, kein verlorener Tastaturweg und kein künstlich fixer
Toolbar-Höhenwert, der Aktionen abschneidet. Vorhandene IDs, Listener, Disabled-
Bedingungen und Stale-Scene-Sperren bleiben maßgeblich.

### Dateibrowser

```text
Pane header / location / native filter
Disclosure | Mark | File/Folder glyph | Name and semantic suffix
Status / selection information
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
- Filter sind bestehende native Input-Entities einschließlich IME, History,
  UTF-16-Selektion, Boundary-Validierung und Clipboard-Guards.
- Navigation, Finder-Rückkehr, Baum-Disclosure, Marks, Scroll und Kontextmenüs ändern
  ihre Semantik nicht. Keine Netzwerkoperation durch einen Style-/Themewechsel.
- Host-/Mapping-/Transferbedingungen sind keine visuellen Freigaben. Ein schönerer
  Button darf niemals eine erneute Transferbestätigung ersetzen.

## Dialoglayouts — Entwurf, noch nicht implementiert

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
Lesbarkeits-/Paint-Abnahme des Extremfalls. Frische korrigierte macOS-CI steht aus.

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
