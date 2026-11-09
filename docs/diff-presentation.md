# Diff-Darstellung

## Grenzen

Die Änderungen betreffen die Go-TUI. Vergleich, vorgeschlagene Sync-Aktionen,
Bestätigungen und übertragene Bytes bleiben maßgeblich. `LineAdded` bedeutet
weiterhin „nur remote“, `LineRemoved` „nur lokal“. Upload dreht die Darstellung
von Vorher/Nachher um, nicht diese Bedeutung.

`internal/architecture/architecture_test.go` prüft Produktionsquellen über den
Go-AST. Anwendung und Sync dürfen weder Bubble Tea noch TUI-Pakete importieren.
Screens dürfen keine Protokollimplementierungen importieren und weder
`remote.Connect` noch `diff.Compare` direkt aufrufen. Browser-Verbindungsaufbau
und Hosttests verwenden `app.BrowseRemote` beziehungsweise `app.TestHost`.

Die Aufteilung orientiert sich an den untersuchten Hunk-Konzepten, verwendet
aber keinen Hunk-Code und keine TypeScript- oder OpenTUI-Abhängigkeit.

## Geometrie und Anzeigeplan

- Zeilennummern erhalten mindestens vier Stellen. Die größte Nummer des gesamten
  Vergleichs bestimmt die Breite, auch wenn die betreffende Zeile eingeklappt ist.
- Tabs laufen auf Vierer-Tabstopps in Terminalspalten. Die Messung berücksichtigt
  breite Zeichen, kombinierende Zeichen, Emoji und die escaped Darstellung von
  Steuerzeichen. Unvertrauter Text geht weiterhin durch `styles.SafeText`.
- Der aktive Anzeigeplan wird nach Vergleichswechsel, Dateiwechsel, Falten,
  Richtungswechsel oder Unified/Split-Wechsel neu erzeugt. Zeichnen, Navigation,
  Scrollgrenzen und Maus verwenden dieselben Zeilen.
- `Flatten` reserviert Speicher für die angezeigten Zeilen, nicht pauschal für
  jede Quellzeile. Zeichnen verarbeitet weiterhin nur den sichtbaren Ausschnitt.

Es gibt keinen globalen Cache aller Anzeigepläne. `DiffResult` hält weder
Anzeigezeilen noch Wortmarkierungen oder Syntaxfarben.

## Alle Dateien in einer Ansicht

`m` schaltet zwischen einer Datei und allen Dateien um; `continuous = true` in
`[ui.diff]` legt den Start fest. Der Gesamtplan (`streamPlan`) reiht je Datei
eine Titelzeile, ihre Anzeigezeilen oder Hinweise (binär, Fehler) und eine
Leerzeile aneinander. Er wird nur neu gebaut, wenn sich Falten, eingeklappte
Abschnitte, Layout, eine Vorschau-Richtung oder ein Dateifehler ändern. Vor dem
Neubau wird die oberste Zeile als Anker (Datei, Zeilenart, Quellzeile) gemerkt
und danach wieder nach oben gestellt.

Gezeichnet werden nur die sichtbaren Zeilen; aufeinanderfolgende Zeilen einer
Datei gehen gemeinsam durch `RenderUnifiedRows`, jeweils mit Richtung, Aktion,
Wort- und Syntaxdaten dieser Datei. Der horizontale Versatz gilt für alle
Abschnitte gemeinsam.

Die aktive Datei steht im festen Titel über dem Diff und ist in ihrer
Abschnittsüberschrift markiert. Manuelles Scrollen macht die oberste Datei aktiv,
aber nur, wenn sich die Ansicht tatsächlich bewegt hat. Tab, Klick in die
Dateiliste und `]`/`[` setzen die aktive Datei ausdrücklich, auch wenn der Abschnitt
wegen des Endes der Liste nicht ganz oben stehen kann. Space, `s`, `u` und `d`
wirken auf diese Datei. `z` oder ein Klick auf eine Abschnittsüberschrift klappt
die Datei auf ihren Titel zusammen; das ist Lesezustand und ändert keine Aktion.

Wort- und Syntaxdaten werden je Ergebnis gehalten, für höchstens 32 Dateien;
darüber hinaus bleiben nur die aktive und die sichtbaren Dateien. Es läuft immer
höchstens eine Aufbereitung. Ihr Ergebnis startet die nächste fehlende sichtbare
Datei; eine Anfrage für eine Datei, die den Bildschirm verlassen hat, wird
abgebrochen. Auch in der Einzeldateiansicht sind die Daten einer zuvor besuchten
Datei dadurch beim Zurückwechseln sofort wieder da.

## Dateiliste

`app.Load` sortiert die Vergleiche als Verzeichnisbaum: zuerst die Dateien eines
Verzeichnisses, dann dessen Unterverzeichnisse. Vorher hing die Reihenfolge vom
Scan ab; rein lokale Dateien standen am Ende. Die Seitenleiste fügt bei jedem
Verzeichniswechsel eine Überschrift ein. Pfade sind relativ zum Projekt, sonst
zum Host-Wurzelpfad, sonst absolut. Die Liste entsteht einmal pro Modell.

Status (`~`, `L`, `R`, `B`, `!`) beschreibt den Vergleich, die letzte Spalte die
geplante Aktion. Die Zeilenzahlen folgen derselben Vorher/Nachher-Reihenfolge
wie die Diff-Zeilen der jeweiligen Datei; bei Upload-Vorschau tauschen `+` und
`-`. Reicht der Platz nicht für mindestens vier Zeichen Dateiname, entfallen die
Zahlen. `<` und `>` ändern die Breite nur für die Sitzung.

Der Dateititel über dem Diff ist eine Zeile: relativer Pfad, Statusbezeichnung,
Zeilenzahlen und Vorher/Nachher-Orientierung. Bei wenig Platz fällt zuerst der
Zusatz „(before → after)“ weg, dann die Zahlen; die Kurzform `Local → Remote`
bleibt, der Pfad wird links gekürzt. `f` zeigt zusätzlich beide absoluten Pfade
in Vorher/Nachher-Reihenfolge. `chromeRows()` liefert die Höhe dieses Kopfs;
Viewport, `contentTop()` und Trefferprüfung rechnen damit.

## Kontext

`+` (auch `=`) und `-` wählen für alle Dateien der Sitzung 0, 1, 3, 5, 10 oder 25
Kontextzeilen; Standard sind drei. `L` zeigt an der ersten sichtbaren Faltung
zehn weitere Zeilen auf jeder Seite, die an eine Änderung grenzt. Das ist
zusätzlicher Kontext für diese Lücke (`diff.FlattenWith`, `reveal` je GapID),
kein neuer Faltzustand: Deckt der Kontext die ganze Lücke ab, verschwindet die
Faltung. `Enter`, `l` und Klick öffnen weiterhin die ganze Lücke; `h` und `c`
setzen auch zusätzlich gezeigten Kontext zurück. Kontextstufe und zusätzlicher
Kontext gehören zum Lesezustand und werden beim Refresh wie aufgeklappte
Bereiche nur bei eindeutig gleichem Inhalt übernommen.

## Beschriftung der Hunk-Köpfe

`diff.LabelHunks` setzt nach `FlattenWith` und vor der Split-Paarung die
Beschriftung jedes Hunk-Kopfs: die nächste Zeile vor dem Hunk auf der
Vorher-Seite, die mit einem Buchstaben, `_` oder `$` beginnt (Gits
Standardregel ohne Sprachtreiber). In `.md`, `.markdown` und `.mdx` zählen nur
Überschriften `#` bis `######` mit Leerzeichen. Die Zeilen werden einmal von
vorn gelesen; es gibt keine weiteren Lesezugriffe und keine Sprachanalyse.
Eingerückte Methoden erkennt die Regel deshalb nicht; dann steht dort die
nächste passende Zeile darüber oder nichts.

Gespeichert werden höchstens 200 Bytes an einer Zeichengrenze. Beim Zeichnen
läuft die Beschriftung durch dieselbe Tab- und `SafeText`-Aufbereitung wie
Code und wird vor den Zeilenbereichen gekürzt.

## Lange Zeilen

Hat eine geänderte Zeile mindestens 4.096 Bytes (`DiffResult.LongestChange`,
einmal mit den übrigen Statistiken berechnet), zeigt die Ansicht statt des Diffs
eine Zusammenfassung. Unveränderte lange Zeilen lösen das nicht aus. `o` öffnet
und schließt den Diff der aktiven Datei; der Zustand gehört zum Lesezustand und
bleibt beim Refresh erhalten. Solange der Diff verborgen ist, gibt es keine
Anzeigezeilen, keine Aufbereitung und keine Hunk-Sprünge in die Datei. Status,
Zahlen und Sync-Aktion bleiben unverändert.

## Leseposition und Refresh

Besuchte Dateien behalten Falten, Quellanker und horizontalen Versatz. Eine neue
Datei beginnt am ersten Unterschied. Resize und Layoutwechsel ordnen den oberen
Quellanker dem neuen Anzeigeplan zu.

`]` und `[` springen über Dateigrenzen hinweg zum nächsten beziehungsweise
vorherigen Hunk. Beim Dateiwechsel zählt der erste beziehungsweise letzte Hunk,
nicht die gespeicherte Leseposition. Vergleiche mit Fehlern, ohne Ergebnis oder
ohne Text-Hunks werden übersprungen, ebenso Binärdateien. Die Navigation läuft
am Anfang und Ende nicht um; normales Scrollen bleibt innerhalb einer Datei.

Das zuletzt erreichte Hunk-Ziel wird getrennt vom begrenzten Scrollversatz
gehalten. Dadurch geht der nächste Sprung weiter, auch wenn ein kurzer Vergleich
vollständig ins Fenster passt. Manuelles vertikales Scrollen oder ein neuer
Anzeigeplan verwerfen dieses temporäre Ziel. Es gehört nicht zum Refresh-Snapshot
und hält keinen alten Anzeigeplan über dessen Austausch hinaus fest.

`ViewSnapshot` enthält ausschließlich Lesezustand und Darstellungspräferenzen.
Dateien werden über ihr lokales/entferntes Pfadpaar zugeordnet. Beim Refresh
müssen Art, Text und der Kontext um einen Anker eindeutig passen. Erweiterte
Gleichbereiche werden nur bei eindeutig identischem Inhalt übernommen. Gelöschte,
veränderte oder mehrdeutige Anker fallen auf den ersten Unterschied zurück.

Das Root-Modell bindet den Snapshot an genau eine Reload-Anfrage. Abbruch,
Projekt-/Hostwechsel, ersetzte Anfragen und verspätete Ergebnisse dürfen ihn
nicht auf einen anderen Vergleich anwenden. Ein Snapshot enthält keine
Sync-Richtungen, Pläne, Bestätigungen oder Transferberechtigungen.

## Wortmarkierungen

`WordHighlights` erstellt zusätzliche UTF-8-Bytebereiche, ohne `DiffLine.Text`
zu ändern. Gleich große entfernte/hinzugefügte Blöcke werden zeilenweise
verglichen; ungleiche, unähnliche und große Ersetzungen behalten die normale
Ganzzeilen-Darstellung.

Grenzen: höchstens 64 Zeilenpaare pro Block, 8.192 kombinierte Bytes pro Paar,
5 ms Diff-Timeout pro Paar und ein kooperatives Gesamtbudget von 50 ms.
Markierungen umfassen vollständige Grapheme. Ihre Betonung kommt zu den
Aktionsfarben hinzu; „Compare only“ zeigt keine Aktions- oder Wortmarkierung.

## Layout und Konfiguration

Unified ist der Standard. `v` schaltet Unified, Split und Auto durch. Auto
verwendet Split ab 160 verbleibenden Diff-Spalten, nach Abzug der Dateiliste.
Unter 40 Terminalspalten verschwindet die Dateiliste; Tab und Shift+Tab bleiben
verfügbar. Split richtet Änderungszeilen nach ihrer Reihenfolge aus. Das ist
keine Behauptung semantischer Gleichwertigkeit zwischen gepaarten Zeilen.

Pfeiltasten verschieben den Code um acht Terminalspalten. Die Gutters bleiben
stehen. Die maximale Breite wird asynchron mit derselben Textprojektion wie beim
Zeichnen gemessen. Der Versatz wird nach Messung und Resize begrenzt. `?` zeigt
Hilfe; dahinter werden keine Datei- oder Sync-Aktionen ausgeführt.

Nur die globale Benutzerkonfiguration legt die Ausgangswerte fest:

```toml
[ui.diff]
layout = "unified" # unified, split oder auto
syntax = false
continuous = false # alle Dateien in einer Ansicht
```

Die Tasten ändern den Sitzungszustand, schreiben aber keine Konfiguration.
Go- und Rust-Store-Schreibvorgänge erhalten alle drei Felder. Projekt- und Hostdaten
können sie nicht überschreiben. Zeilenumbruch ist nicht implementiert.

## Optionale Syntaxfarben

Chroma v2.27.0 wurde nach einem separaten Vorversuch aufgenommen. Das Paket und
seine neue Regex-Abhängigkeit sind MIT-lizenziert; die Hinweise stehen in
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md). Importiert werden Lexer
und Tokenisierung, keine Formatter, Fonts oder Projektprogramme.

Die Sprache ergibt sich allein aus dem Dateinamen. Die vollständigen,
normalisierten Zeilen beider Seiten werden aus dem vorhandenen Vergleich
rekonstruiert. Das erhält mehrzeilige lexikalische Zusammenhänge auch dann, wenn
eine unveränderte Zeile lokal und remote verschieden eingeordnet werden muss.
Es gibt keine weiteren lokalen oder entfernten Lesevorgänge.

Syntaxfarben sind standardmäßig aus. Die Aufbereitung läuft wie die
Wortmarkierung in einem abbrechbaren `tea.Cmd`; Anfrageidentitäten verhindern
späte Übernahmen. Aktionshintergründe, Marker und Wortbetonung bleiben erhalten.
Der Renderer färbt nur den bereits sanitisierten Originaltext, niemals vom Lexer
zurückgelieferten Text.

Arbeitsgrenzen:

- 128 KiB rekonstruierter Quelltext über beide Seiten zusammen;
- 2.048 Bytes je Quellzeile und 4.000 Zeilen je Seite;
- 32.000 Lexer-Tokens je Seite;
- 150 ms kooperatives Tokenisierungsbudget über beide Seiten.

Der Zeitabbruch wird zwischen Tokens geprüft. Eine laufende Chroma-Lexeroperation
kann damit nicht hart unterbrochen werden; Chroma begrenzt einzelne Regex-Matches
selbst auf 250 ms. Das Budget ist deshalb keine garantierte Wandzeitgrenze.
Die TUI wartet nicht auf den Worker. Unbekannte Sprachen, ungeeignete Quellen und
Budgetüberschreitungen bleiben gewöhnlicher Text. Tatsächliche Lexerfehler
werden als typisierte Ergebnisse gemeldet, in der Statuszeile angezeigt und bei
aktiviertem Logging protokolliert; Vergleich und Sync bleiben benutzbar.

Ein mutexgeschützter Cache hält genau ein begrenztes Ergebnis. Sein Schlüssel
enthält Inhalt und Sprache. Die gespeicherten Tokenklassen sind unabhängig von
der Palette: Farben werden erst beim Zeichnen aus `internal/styles` angewandt,
sodass ein Themewechsel keine alten Farben aus einem Cache übernehmen kann.

## Messungen

Linux amd64, Ryzen 5 5600X, Go 1.25; synthetische Eingaben, keine Aussage über
spürbare Verzögerungen an anderen Terminals. Keine festen CI-Zeitgrenzen.

Bei einer Änderung je 100 Zeilen und gleichem sichtbaren Ausschnitt:

| Quellzeilen | View vor dem Umbau, B/op | View nach dem Umbau, B/op | nachher, ms/op |
|---:|---:|---:|---:|
| 1.000 | 153.694 | 76.656 | 0,255 |
| 20.000 | 1.521.993 | 77.581 | 0,261 |
| 40.000 | 2.945.257 | 76.880 | 0,256 |

Die Vorherwerte stammen aus der externen Untersuchung, die Nachherwerte aus
`BenchmarkDiffView`. Häufige Änderungen (jede zweite Zeile) ergaben rund
79 KiB und 0,30 ms je Frame. `Flatten` bleibt linear, läuft aber nicht mehr je
Frame; 20.000 Zeilen benötigten rund 270 KiB pro Plan statt zuvor 1,43 MB.

Für die Gesamtansicht (`BenchmarkContinuous`, je Datei 1.000 Zeilen mit einer
Änderung je 100 Zeilen, 120×40 Zellen): Ein Frame kostete bei 20 Dateien rund
0,36 ms und 91 KiB, bei 200 Dateien rund 0,42 ms und 99 KiB. Der Neubau des
Gesamtplans kostete bei 20 Dateien rund 0,16 ms und 0,4 MB, bei 200 Dateien
rund 1,4 ms und 4,8 MB; er läuft nur nach den oben genannten Änderungen.

Der separate Chroma-Vorversuch erkannte 297 Lexer. Für 34 KB synthetischen Text
lagen Go, PHP, JavaScript, Rust und TOML bei ungefähr 16–30 ms, Python bei 83–87 ms.
Das sind Tokenisierungsmessungen, keine Sprachkorrektheits- oder UI-Tests.
`BenchmarkSyntaxHighlights` mit zwei etwa 10-KB-Seiten ergab 18,0 ms und 13,0 MB
Allokationen ohne Cachehit, 1,43 ms und 120 KB mit Cachehit. Auch Cachehits sind
Worker-Arbeit, keine Frame-Arbeit.

Ein normaler Linux-Build wuchs im Vergleich direkt vor/nach dem Syntax-Schritt
von 15.389.679 auf 19.386.179 Bytes, etwa 3,81 MiB beziehungsweise 26 Prozent.
Diese Kosten fallen im Binary auch bei ausgeschalteten Syntaxfarben an.

Reproduzierbare Checks:

```bash
umask 022
go test ./...
go test -race ./...
go vet ./...
go build ./...
go test ./internal/tui/diffview -run '^$' \
  -bench 'Benchmark(DiffView|Flatten|Continuous)$' -benchmem
go test ./internal/diff -run '^$' -bench BenchmarkSyntaxHighlights -benchmem
cd rust
cargo fmt --all -- --check
cargo test --locked -p drift-core
cargo clippy --locked -p drift-core --all-targets -- -D warnings
```

Die interaktive Abnahme steht noch aus; ihre Fälle stehen in
[`manual-testing.md`](manual-testing.md). Suche, Auswahl/Kopieren, Watch-Modus,
Agentenanmerkungen, ein neuer Diff-Algorithmus und Zeilenumbruch sind nicht Teil
dieser Umsetzung.
