# 0005: Bei unklaren Zeitstempeln schlägt drift den Upload vor

Status: Angenommen
Datum: 2026-11-20

## Kontext

`sync.AutoDecision` schlägt für jede Datei im Diff eine Richtung vor, und
„sync all“ übernimmt diese Vorschläge. Bei geänderten Dateien auf beiden Seiten
entscheidet der Vergleich der Änderungszeiten. Diese Zeiten sind oft ungenau:
FAT32 speichert sie nur auf 2 Sekunden genau, Uhren laufen leicht auseinander,
und über FTP und FTPS ist die Auflösung von MDTM grob und oft unzuverlässig.
`docs/architecture-target.md` (Abschnitt `internal/sync/policy.go`) führt den
„Umgang mit Ambiguität bei mtime“ als Aufgabe dieser Datei.

## Entscheidung

Eine geänderte Datei wird nur dann zum Download vorgeschlagen, wenn die entfernte
Kopie mehr als 2 Sekunden neuer ist. In allen anderen Fällen schlägt
`AutoDecision` den Upload vor. Das gilt auch, wenn die Zeiten weniger als
2 Sekunden auseinanderliegen oder fehlen. `DecisionNone` gibt es nur für
unveränderte Dateien und für Sitzungen ohne Ergebnis oder mit Fehler.

## Begründung

Die Schwelle von 2 Sekunden fängt die Auflösung von FAT32 und kleine
Uhrabweichungen ab. Würde `AutoDecision` bei unklaren Zeiten `DecisionNone`
liefern, fiele die Datei still aus „sync all“ heraus. Für ein Werkzeug, das auf
Deployments ausgerichtet ist, wäre das genau das falsche Verhalten.

## Folgen

- Eine geänderte Datei geht nie still verloren: Sie bekommt immer eine Richtung.
- Ist die entfernte Seite höchstens 2 Sekunden neuer oder fehlen die Zeiten,
  überschreibt der vorgeschlagene Upload die entfernte Änderung, wenn niemand den
  Vorschlag prüft.

## Fundstellen

- `internal/sync/policy.go`: Doc-Kommentar von `AutoDecision` (Begründung zur
  Schwelle und zum Upload im Zweifel)
- `internal/sync/policy_test.go`: `TestAutoDecision`, Fälle „ambiguous mtime
  defaults to upload“ und „zero mtimes default to upload“
