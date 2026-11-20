# 0001: Der Projektspeicher liegt im Konfigurationsverzeichnis und ist nach dem Registry-Slug benannt

Status: Angenommen
Datum: 2026-11-20

## Kontext

drift braucht pro Projekt einen Ort für Hosts, Mappings und Zugangsdaten. Naheliegend
wäre eine Datei im Projektverzeichnis oder eine Datei, die nach dem Projektpfad benannt
ist. Die Registry in `internal/project` kennt bereits Slug und Pfad jedes Projekts.

`AGENTS.md` (Abschnitt „Nothing in the project directory“) und
`docs/architecture-target.md` (Abschnitt `internal/config`) beschreiben den Speicherort
bereits. Die Begründung stand bisher nur in einem Kommentar in `internal/config/store.go`.

## Entscheidung

Die Konfiguration eines Projekts liegt vollständig unter `<config.Dir()>/projects/`, eine
Datei pro Projekt, benannt nach dem Registry-Slug, zum Beispiel
`~/.config/drift/projects/shop-a.toml`. drift schreibt nichts in das
Projektverzeichnis. Die Datei hat die Rechte 0600, das Verzeichnis 0700.

## Begründung

Der Speicher ist nach dem Slug benannt und nicht nach dem Pfad, weil die Registry den
Pfad verwaltet. Wird ein Projekt verschoben, behält es seine Hosts, solange die Registry
aktualisiert wird; `Registry.Update` ändert den Pfad eines Eintrags und lässt den Slug
stehen.

Die Datei enthält die Hosts samt Zugangsdaten. Deshalb schreibt `WriteTOML` sie mit 0600,
und `writeProjectStore` legt das Verzeichnis mit 0700 an.

## Folgen

- Ein Arbeitsverzeichnis bleibt frei von drift-Dateien.
- `config.Load` braucht den Slug vom Aufrufer, weil `internal/project` das Paket
  `internal/config` importiert und die Registry deshalb nicht von dort befragt werden kann.
- Der Slug wird in `projectStorePath` zum Dateinamen und muss dort geprüft werden.
- Zwei Tests mit demselben Slug teilen sich eine Datei; `isolate(t)` gibt jedem Test ein
  eigenes Konfigurationsverzeichnis.

## Fundstellen

- `internal/config/store.go`: Kommentarblock vor `projectsDir` (ersetzt), `projectStorePath`,
  `writeProjectStore`
- `internal/config/writer.go`: `WriteTOML`
- `internal/project/project.go`: `Registry.Update`
