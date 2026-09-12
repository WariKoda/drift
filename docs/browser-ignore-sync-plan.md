# Implementierungsplan: Hidden Files und Gitignore im Browser und Sync

Status: implementiert auf `docs/browser-ignore-sync-plan`.
Basis: `e8f3880`.
Automatisch geprüft mit LSP, `go test ./...`, `go test -race ./...`,
`go vet ./...` und `go build ./...`. Die manuelle TUI-Abnahme mit einem echten
Deployment-Host bleibt vor dem Merge offen.

## Ziel

Browser-Sichtbarkeit und Sync-Umfang werden getrennt. Wer einen Ordner auswählt,
soll vor der Übertragung erkennen können, welche Dateien rekursiv enthalten sind
und welche ausgeschlossen bleiben. Ein ausgeblendeter oder ausgeschlossener Pfad
darf niemals allein dadurch eine Löschentscheidung auslösen.

## Verhaltensregeln

| Kategorie | Browser-Standard | Rekursiver Ordner-Sync | Direkte Dateiauswahl |
| --- | --- | --- | --- |
| Normale Datei | sichtbar | enthalten | enthalten |
| Hidden, nicht ignored | ausgeblendet | enthalten | enthalten |
| Gitignored | ausgeblendet | ausgeschlossen | ausdrücklich übersteuerbar |
| Fest ausgeschlossener Pfad | ausgeblendet | ausgeschlossen | gesperrt |

- Hidden bedeutet ein mit Punkt beginnender Pfadbestandteil relativ zum Projekt
  beziehungsweise zur Remote-Wurzel. `.env`, `.htaccess` und Dateien unter
  `.well-known/` bleiben beim Ordner-Sync enthalten, sofern keine Ignore-Regel greift.
- `.` schaltet Hidden Files um, `I` gitignored Einträge. Beide Filter gelten
  unabhängig: Eine zugleich versteckte und ignorierte Datei benötigt beide Toggles.
- Einblenden verändert niemals den Sync-Umfang.
- Die direkte Auswahl einer ignorierten Datei erlaubt genau dieses Dateipaar.
  Ein ausgewählter ignorierter Ordner erlaubt nicht automatisch seinen Inhalt.
  Dafür braucht es die Option „Ignored ebenfalls einbeziehen“ für diesen Vorgang.
- Feste Ausschlüsse sind nicht übersteuerbar, auch nicht durch direkte Auswahl
  oder Auswahl eines ihrer Unterpfade.
- Ein Ausschluss entfernt das ganze lokale/remote Dateipaar vor dem Vergleich.
  Er gilt dadurch gleichermaßen für Upload, Download und beide Löschrichtungen.
- Regeln für Mappings und `fs.Root` bleiben immer gültig. Eine Ignore-Ausnahme
  erlaubt weder Pfade außerhalb der Mappings noch Symlink-Escapes.

### Feste Ausschlüsse im ersten Lieferumfang

Die bestehende Liste in `internal/fs/local.go` bleibt erhalten:
`.git`, `.svn`, `.hg`, `node_modules`, `.idea`, `.vscode`.
Das bedeutet insbesondere, dass „Ignored ebenfalls einbeziehen“ derzeit auch
`node_modules` nicht freischaltet. Eine Umstellung dieser Liste auf konfigurierbare
Deployment-Ausschlüsse ist eine separate Produktentscheidung.

VCS-Metadatendateien wie eine `.git`-Datei bei Worktrees zusätzlich sperren.
Die Prüfung muss auch ausgewählte Wurzeln und ausgeschlossene Vorfahren erfassen;
der bisherige Walker überspringt nur untergeordnete Verzeichnisse.

## Ausgangslage im Code

- `internal/fs/local.go`: `ReadDir` zeigt auch Dotfiles. `WalkFiles` kennt feste
  Ausschlüsse, aber kein Gitignore, und überspringt Lesefehler.
- `internal/tui/browser/tree.go`: `filteredEntries` filtert bisher nur nach Namen.
- `internal/tui/browser/finder.go`: eigener asynchroner Index über `fs.WalkFiles`.
  Browser-Baum und Finder haben damit bereits unterschiedliche Ausschlusswirkung.
- `internal/tui/browser/remote.go`: Remote-Listing und eigene Auswahl müssen
  dieselben Sichtbarkeitsregeln erhalten.
- `internal/tui/diffview/model.go`: `LoadCmd` expandiert lokale und remote
  Ordnerauswahlen und durchsucht jeweils die Gegenseite nach zusätzlichen Dateien.
  `addFile` dedupliziert Dateipaare. Hier darf nicht nur eine Walk-Seite filtern.
- `internal/tui/diffview/load_activity.go`: zusätzlicher lokaler Walker mit
  Aktivitätsüberwachung und festen Ausschlüssen.
- `internal/ftp/client.go` und `internal/sftp/client.go`: Remote-Walker verwenden
  ebenfalls `fs.ShouldSkipDir`; FTP besitzt parallele Walk-Worker.
- `internal/tui/app.go` und `internal/tui/app_certtrust.go`: Auswahl, Hostwechsel,
  Diff-Anfragen und FTPS-Vertrauensretry müssen die neuen Vorgangsoptionen tragen.
- `internal/config/config.go`: globale UI-Konfiguration existiert bereits.

## Gitignore-Semantik

Lokale Regeln bestimmen den Sync-Umfang. Remote-Dateien werden zuerst durch
`pathmap` auf ihren lokalen Projektpfad abgebildet und dann gleich bewertet.
Remote-`.gitignore`-Dateien werden nicht als zusätzliche Regeln geladen.
Das gilt auch für remote-only Dateien und lokal noch nicht existente Verzeichnisse.
Nicht abbildbare Remote-Pfade bleiben außerhalb des Sync-Umfangs und werden nicht
als fehlende lokale Dateien interpretiert.

Die Evaluierung vorhandener Go-Matcher ergab Abweichungen bei ausgeschlossenen
Eltern, `**`, Indexbehandlung und Worktrees. Die Implementierung verwendet deshalb
Git selbst als Matching-Engine. Jeder Scan übergibt alle noch nicht klassifizierten
Pfade gemeinsam und NUL-separiert an `git check-ignore --stdin -z --verbose
--non-matching`. Es gibt keinen Git-Prozess pro Datei.

In Repositories gelten damit verschachtelte `.gitignore`, `.git/info/exclude`,
`core.excludesFile`, der Index, übergeordnete Repository-Regeln und Linked
Worktrees genau wie in Git. Getrackte Dateien bleiben enthalten. Außerhalb eines
Repositories initialisiert drift ein temporäres Bare-Repository außerhalb des
Projekts und wertet die projektinternen `.gitignore`-Dateien mit `--no-index` aus;
globale Regeln sind dort absichtlich abgeschaltet.

Git ist eine dokumentierte Laufzeitabhängigkeit. Fehlt es oder liefert es
fehlerhafte beziehungsweise unvollständige Daten, bricht die Klassifizierung
sichtbar ab. Drift fällt niemals still auf einen größeren Sync-Umfang zurück.

Ein fehlendes Ignore-File ist normal. Ein vorhandenes, aber unlesbares Regelwerk
ist ein Fehler. Kein unbemerkter Fallback auf einen größeren Sync-Umfang.
Regeln für einen Scan als unveränderlichen Stand erfassen. Refresh und ein neuer
Sync laden sie erneut; Projektwechsel verwirft alle zugehörigen Caches.

## Umsetzungsschritte

### 1. Gemeinsame Pfadbewertung

- Einen kleinen Nicht-UI-Baustein für Pfadklassifizierung und Ignore-Auswertung
  einführen, genutzt von Baum, Finder und Sync-Ermittlung.
- Ergebnis unterscheidet hidden, gitignored und fest ausgeschlossen, mit Grund
  und nach Möglichkeit Herkunft der Ignore-Regel. Hidden kann parallel zu einem
  Ausschluss vorliegen.
- Native lokale Pfade und slash-basierte Remote-/Matcher-Pfade sauber trennen.
  Keine String-Präfixprüfung als Ersatz für Pfadgrenzen verwenden.
- Bestehende feste Ausschlüsse zentral halten. `fs.WalkFiles` nicht um beliebige
  Flags oder Callbacks zur Konfiguration seiner Ausschlussliste erweitern.
- Ignore-Dateien projektintern über den geöffneten `fs.Root` lesen. Zugriffe auf
  Git-Metadaten außerhalb des Projekts, etwa bei Worktrees, gesondert begrenzen.

### 2. Browser, Remote-Pane und Finder

- Zwei unabhängige Sichtbarkeitsschalter ergänzen. Neue globale Optionen unter
  `[ui]`: `show_hidden = false` und `show_ignored = false`.
- Laufzeit-Toggles gelten für beide Panes und Finder. Sie schreiben nicht
  automatisch Konfiguration und ändern keine laufende Diff-Anfrage.
- Ignorierte Einträge gedimmt und mit `ignored` kennzeichnen. Styles zentral
  definieren, nicht im View. Nicht abbildbare Remote-Pfade nicht als vermeintlich
  „nicht ignored“ ausgeben, sondern als außerhalb des Mappings kennzeichnen.
- Cursor, Scrollposition, Elternnavigation, Suchfilter und Mausauswahl nach
  Sichtbarkeitsänderungen konsistent halten.
- `V`, `*` und visuelle Auswahl betreffen nur sichtbare Einträge. Bereits markierte
  Dateien behalten beim Ausblenden ihre Markierung; Anzahl ausgeblendeter
  Markierungen sichtbar anzeigen und vor dem Sync aufführen.
- Finder und Baum verwenden dieselben Regeln. Neue Indizes und Regelermittlung
  laufen in `tea.Cmd`; Ergebnisse tragen Projekt-/Anfrageidentität.
- Hilfe und Statuszeile ergänzen. Keine Dateisystem- oder Git-Abfragen in `View`.

### 3. Explizite Auswahl und rekursiven Umfang trennen

- Browser-Auswahl beim Start in einen unveränderlichen Auftrag kopieren.
  Direkte Dateipfade, Ordnerwurzeln und Vorgangsoptionen getrennt erhalten.
- „Ignored ebenfalls einbeziehen“ startet pro Auftrag mit `false`. Im ersten
  Lieferumfang kein dauerhaftes `respect_gitignore = false`, um versehentliche
  globale Freigaben zu vermeiden.
- Direkte ignorierte Dateiauswahl als Ausnahme für das Dateipaar behandeln,
  unabhängig davon, ob ein zusätzlich markierter Elternordner zuerst besucht wird.
- Überlappende lokale/remote Auswahlen deduplizieren. Ausnahmen müssen vor der
  rekursiven Filterung bekannt sein und dürfen nicht von der Walk-Reihenfolge abhängen.
- Optionen auch durch Hostauswahl und FTPS-Zertifikatsretry erhalten. Bei neuem
  Auftrag oder Projektwechsel keine Freigabe aus einem alten Auftrag übernehmen.

### 4. Symmetrische Sync-Ermittlung

- Umfangsermittlung aus der großen `LoadCmd`-Funktion in `internal/sync`
  herauslösen, soweit für die gemeinsame Bewertung und Tests erforderlich.
  Keine allgemeine neue Sync-Engine bauen.
- Lokale und remote Kandidaten zunächst mappen und klassifizieren; nur erlaubte
  Paare an den eigentlichen Vergleich übergeben.
- Alle vier Wege testen: lokale Datei, lokaler Ordner samt Remote-Gegenseite,
  remote Datei, remote Ordner samt lokaler Gegenseite.
- Ignore-Regeln auch auf Kandidaten aus dem Gegen-Walk anwenden. Ein lokal
  übersprungener Pfad darf durch den Remote-Walk nicht wieder aufgenommen werden.
- Vorhandene Transportparallelität und Keep-alive-Aktivitätsmeldungen erhalten.
  Für Version 1 darf Remote-Gitignore nach dem Listing filtern; Transportclients
  erhalten dafür weder lokale Pfade noch einen lokalen Matcher.
- Fest ausgeschlossene Verzeichnisse weiterhin früh überspringen. Zusätzliche
  Gitignore-Pruning-Optimierung erst nach korrekten Negations- und Mappingtests.
- Lesefehler im betroffenen Scanpfad nicht als Abwesenheit behandeln. Betroffene
  Bereiche von Sync-Entscheidungen ausschließen und Fehler sichtbar melden;
  im Zweifel den Scan abbrechen. Keine FTP-550-Neuklassifizierung in diesem Change.

### 5. Übersicht vor der Übertragung

Den bestehenden Diff-Schritt nutzen, keinen zusätzlichen Pflichtdialog einführen.
Der fertig ermittelte Umfang bleibt dort vor Quick-Sync und Bulk-Sync sichtbar:

```text
Sync public/
  128 Dateipaare im Umfang, davon 6 hidden
   12 ignorierte Dateien übersprungen
    3 ignorierte Verzeichnisse nicht durchsucht
    1 fest ausgeschlossenes Verzeichnis
    2 direkt ausgewählte ignored Dateien enthalten

Ignored ebenfalls einbeziehen: aus
```

- Zahlen sind Beispiele. Dateipaare nach Deduplizierung zählen, auch identische
  Dateien, die später nicht in der Diff-Liste stehen. Hidden ist eine Teilmenge.
- Für übersprungene Unterbäume keine erfundene Dateianzahl anzeigen. Dateien und
  nicht durchsuchte Verzeichnisse getrennt zählen. Keine Zusatz-Walks nur zum Zählen.
- Bei leerem Ergebnis die Übersicht behalten und den Grund anzeigen.
- Umschalten der Vorgangsoption berechnet Umfang und Vergleich erneut, nicht nur
  die Anzeige. Bisherige manuelle Sync-Richtungen dabei verwerfen und dies anzeigen.
- Solange die Neuberechnung läuft, keine Übertragung oder Löschung zulassen.
- Normales Diff-Refresh behält die Vorgangsoption, lädt aber Regeln und Umfang
  erneut. Wurden Dateien ausgeschlossen, müssen ihre alten Aktionen verschwinden.
- Request-IDs, Verbindungsidentität, Ressourcenbesitz, Abbruch und Trust-Retry
  beibehalten. Fehler als typisierte Nachrichten und über `internal/log` melden.

### 6. Persistenz und Dokumentation

- UI-Optionen in `internal/config/config.go`, Loader und Writer aufnehmen.
  Roundtrip und bisherige Config-Dateien ohne neue Optionen testen.
- Keine Datei in den Projektbaum schreiben, insbesondere keine `.driftignore`.
- `README.md` um Defaults, Toggles und Ordner-Sync-Regeln ergänzen.
- `CHANGELOG.md` um die sichtbare Verhaltensänderung ergänzen. `AGENTS.md` erst
  bei Umsetzung anpassen, falls sich Zuständigkeiten oder feste Regeln ändern.

## Tests und Abnahme

Keine Mocks. Temporäre Dateibäume und Git-Repositories verwenden; Remote-Tests
über die bestehenden echten lokalen FTP-, FTPS- und SSH/SFTP-Testserver ausführen.

- [ ] Ordner-Sync enthält ausgeblendete `.htaccess` und `.well-known`.
- [ ] Eine ignorierte `.env` bleibt beim Ordner-Sync auf beiden Seiten ausgeschlossen.
- [ ] Remote-only und local-only ignorierte Dateien erhalten keine Session und
      keine Upload-, Download- oder Löschaktion.
- [ ] Direkte ignorierte Datei ist enthalten; ihr ignorierter Nachbar bleibt draußen.
- [ ] Ignorierter Ordner allein gibt seinen Inhalt nicht frei; Vorgangsoption tut es.
- [ ] Gleichzeitige Auswahl von Elternordner und Datei ist reihenfolgeunabhängig.
- [ ] `.git` als Datei, Verzeichnis, ausgewählte Wurzel und Vorfahr bleibt gesperrt.
- [ ] Sichtbarkeitstoggles ändern niemals den rekursiven Sync-Umfang.
- [ ] Verschachtelte Regeln, Negationen, getrackte Dateien und Sonderzeichen
      einschließlich Leerzeichen und Zeilenumbrüchen im Pfad funktionieren.
- [ ] Remote-only Pfade, Host-Mappings und Projekt-Fallback-Mappings werden korrekt
      auf lokale Regeln bezogen; Mapping-Grenzen bleiben wirksam.
- [ ] Lesefehler, Symlink-Escapes und nicht abbildbare Pfade führen nicht zu
      vermeintlich fehlenden Gegenstücken oder Löschaktionen.
- [ ] Übersicht zählt überlappende Selektionen nur einmal und unterscheidet
      übersprungene Dateien von nicht durchsuchten Verzeichnissen.
- [ ] Regeländerung, Refresh, Projektwechsel, Trust-Retry, Verbindungsabbruch und
      veraltete asynchrone Ergebnisse behalten beziehungsweise verwerfen Optionen korrekt.
- [ ] Config-Roundtrip bleibt im isolierten temporären Config-Verzeichnis.

Nach Umsetzung: aktive LSP-Diagnostik für geänderte Go-Dateien,
`go test ./...`, `go test -race ./...`, `go vet ./...`, `go build ./...`.
Anschließend manuelle TUI-Abnahme mit lokalen und remote Ordnerauswahlen,
Finder, beiden Toggles, versteckten Markierungen und der Vorgangsoption.

## Nicht Teil dieses Plans

- Eigenes konfigurierbares Drift-Ignore-Format.
- Automatische Übernahme remote gespeicherter Ignore-Regeln.
- Umstellung der bisherigen festen Ausschlussliste.
- Automatische Löschungen, Transfers oder Wiederholungen nach Verbindungsfehlern.
- Allgemeiner Umbau der Browser- oder Transportarchitektur.
