# 0004: Gleiche Größe und gleiche Änderungszeit gelten als unverändert

Status: Angenommen
Datum: 2026-11-20

## Kontext

`diff.CompareWithActivity` entscheidet für jedes Dateipaar, ob sich die Inhalte
unterscheiden. Um den entfernten Inhalt zu vergleichen, muss drift die Datei
herunterladen. Die Metadaten beider Seiten liegen nach den beiden `Stat`-Aufrufen
dagegen schon vor. `docs/architecture-target.md` (Abschnitt `internal/diff`)
nennt diesen Schnellpfad bereits als umgesetzt.

## Entscheidung

Sind Größe und Änderungszeit beider Seiten exakt gleich und ist die lokale
Änderungszeit nicht der Nullwert, gilt das Paar als unverändert. drift liest dann
keinen Inhalt. Sonst vergleicht drift den Inhalt: Dateien bis 2 MiB je Seite
vollständig, größere Dateien gleicher Größe über SHA-256. Größere Dateien
unterschiedlicher Größe gelten ohne Lesen als verschieden.

## Begründung

Der Schnellpfad erspart den Download des entfernten Inhalts für jede Datei, deren
Metadaten übereinstimmen.

## Folgen

- Eine Änderung, die Größe und Änderungszeit unverändert lässt, erkennt der
  Vergleich nicht.
- Fehlt die lokale Änderungszeit, greift der Schnellpfad nicht, und drift
  vergleicht den Inhalt.
- Der Schnellpfad verlangt exakt gleiche Zeiten. Die Toleranz von 2 Sekunden aus
  `sync.AutoDecision` gilt hier nicht.

## Fundstellen

- `internal/diff/engine.go`: Kommentar „Fast path“ in `CompareWithActivity`
