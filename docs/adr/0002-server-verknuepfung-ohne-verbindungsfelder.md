# 0002: Ein Projekt-Host verknüpft einen globalen Server und setzt kein eigenes Verbindungsfeld

Status: Angenommen
Datum: 2026-11-20

## Kontext

Mehrere Projekte deployen oft auf dieselbe Maschine, jeweils in ein anderes Verzeichnis.
Ohne gemeinsame Verbindung müsste jedes Projekt Hostname, Port, Benutzer und Zugangsdaten
selbst speichern, und ein geändertes Passwort bräuchte eine Änderung pro Projekt.

`AGENTS.md` (Abschnitt „Config locations“) und `docs/guide/configuration.md` (Abschnitt
„Sharing a server between projects“) beschreiben das Verhalten bereits. Die Begründung
gegen teilweise Überschreibungen stand bisher nur in einem Kommentar in
`internal/config/servers.go`.

## Entscheidung

Ein Projekt-Host kann einen globalen Host über `server = "<name>"` verknüpfen. Der globale
Host ist dann ein Server: Hostname, Port, Benutzer, Auth, Protokoll und Keep-alive gelten
für jeden Projekt-Host, der ihn nennt. `root_path` und `mappings` bleiben beim Projekt-Host.

```toml
# ~/.config/drift/projects/shop-b.toml
[[hosts]]
name      = "prod"
server    = "kunde-x"
root_path = "/var/www/shop-b"
```

Eine Verknüpfung setzt kein Verbindungsfeld selbst. `validateLinks` lehnt einen solchen
Eintrag ab.

## Begründung

Dürfte eine Verknüpfung einzelne Verbindungsfelder überschreiben, hieße ein leerer
Benutzer oder ein Port 0 in einem Eintrag „geerbt“ und im nächsten „nicht gesetzt“.
Ohne Überschreibungen hat jedes Feld in jedem Eintrag dieselbe Bedeutung.

## Folgen

- Zwei Projekte, die in verschiedene Verzeichnisse derselben Maschine deployen, teilen
  sich einen Satz Zugangsdaten.
- Im Speicher trägt eine aufgelöste Verknüpfung die Verbindung ihres Servers (`Resolve`).
  Beim Schreiben entfernen `hostsOut` und `linkFields` sie wieder.
- Ein Server, den ein Projekt noch verknüpft, lässt sich nicht löschen oder umbenennen
  (`ServerInUseError`).

## Fundstellen

- `internal/config/servers.go`: Kommentarblock vor `IsLink` (ersetzt), `hasConnection`,
  `Resolve`, `linkFields`, `validateLinks`
- `internal/config/writer.go`: `hostsOut`
