# Plan: Konfiguration vollständig in `~/.config/drift`

## Ziel

Im Projekt-Repository landet nichts. Kein `.drift/`, keine `.drift.toml`, keine
Datei, die jemand versehentlich committen könnte. Alles, was drift über ein
Projekt weiß, liegt unter `~/.config/drift`.

Das ersetzt [`config-scope-plan.md`](config-scope-plan.md) ab Phase 2. Phase 1
daraus (Trennung von Umgebung und Zugang) bleibt als Maschinerie nützlich: der
projektbezogene Store und seine Migration existieren, sie bekommen nur ein
anderes Ziel. Die Zweiteilung selbst fällt weg, denn sie existierte
ausschließlich, damit eine Hälfte committbar ist.

Was dabei verschwindet: `internal/config/gitguard.go`, `ensureProjectGitignore`,
die Exposure-Meldung, die Statuszeilen-Warnung, die Gruppenüberschriften im
Host-Formular. Der gesamte Apparat aus 0.1.5 und 0.1.6 war die Antwort auf eine
Datei im Repo. Ohne die Datei gibt es die Frage nicht.

Ein Nebeneffekt: das Identitätsproblem aus Phase 1 löst sich. Weil das Finden
des Projekt-Roots ohne Marker-Datei über die Registry laufen muss, kann der
Store auf den **Slug** keyen statt auf den absoluten Pfad. Ein verschobenes
Projekt verliert dann nicht mehr seine Zugänge, solange die Registry den neuen
Pfad kennt.

Der Preis, damit es später niemand übersieht: Mappings reisen nicht mehr mit dem
Clone. Jeder gibt sie pro Maschine neu ein, und wenn jemand `plugins/plugin1`
verschiebt, gibt es keinen Commit, der das für alle repariert.

## Zielbild

```
~/.config/drift/config.toml            globale Hosts + [ui], unverändert
~/.config/drift/projects.toml          Registry: slug, name, path, unverändert
~/.config/drift/projects/<slug>.toml   pro Projekt, alles:
                                         [defaults]
                                         [[hosts]]   vollständig, inkl. auth
                                         [[mappings]]
```

Eine Datei pro Projekt statt einer großen: sie lässt sich einzeln ansehen,
kopieren und löschen, und sie liegt neben der Registry, die dieselben Slugs
verwendet. `access.toml` und `secrets.toml` verschwinden nach der Migration.

## Drei Vorentscheidungen

### 1. `config.Load` bekommt die Projekt-Identität übergeben

`internal/project` importiert `internal/config` (für `config.Dir()`), also darf
`config` die Registry nicht befragen. Der Aufrufer kennt beides und löst auf:

```go
// Load merges the global config with the store of the project rooted at root.
// An empty slug means the caller found no registered project: only global
// hosts are available.
func Load(root, slug string) (*MergedConfig, error)
```

Damit bleibt `internal/config` abhängigkeitsfrei, und das Auffinden des Projekts
liegt an einer Stelle statt in zwei Paketen.

### 2. Rooting über die Registry, nicht über eine Marker-Datei

Neu in `internal/project`: `Registry.FindByPathPrefix(dir)` liefert das
registrierte Projekt mit dem längsten Pfad-Präfix von `dir`. `FindByPath`
(exakter Treffer) bleibt für die Fälle, die ihn brauchen.

Ein nicht registriertes Verzeichnis hat kein Projekt, also auch keine
Projekt-Hosts. Das ist konsistent, weil es dann auch keinen Store hätte. Der
Registrierungs-Prompt existiert bereits (`ScreenRegisterPrompt`,
`shouldPromptRegister`); er ist ab jetzt der einzige Weg in einen Projekt-Store.
Als Vorschlag für den Root nimmt er den git-Root, wenn es einen gibt, sonst das
aktuelle Verzeichnis.

### 3. Die Migration braucht einen Slug, also läuft sie über den Aufrufer

Ein `.drift/config.toml` kann in einem Projekt liegen, das nie registriert
wurde. Die Migration muss es dann zuerst registrieren, und die Registry liegt
in `internal/project`. Also: `config.MigrateProjectToStore(root, slug)` macht
die Dateiarbeit, das Registrieren erledigt der Aufrufer davor. Dieselbe Stelle
in `app.go`, an der heute `MigrateProjectSecrets` hängt.

---

## Schritt 1 — Per-Projekt-Store

### Format

```toml
# ~/.config/drift/projects/kalieber.toml
[defaults]
  user = "deploy"

[[hosts]]
  name = "DEV-Kalieber"
  hostname = "example.com"
  port = 21
  user = "kalwqvmw"
  root_path = "/var/www"
  protocol = "ftp"
  [hosts.auth]
    type = "password"
    password = "…"

  [[hosts.mappings]]
    local = "custom/plugins/Foo"
    remote = "custom/plugins/Foo"

[[mappings]]
  local = "src"
  remote = "html"
```

Mode `600`, Verzeichnis `~/.config/drift/projects/` mit `700`. Der Store trägt
Credentials im Klartext, wie `access.toml` heute.

### Schritte

1. `internal/config/store.go`: `projectStorePath(slug)`, `loadProjectStore(slug)`,
   `writeProjectStore(slug, ProjectConfig)`. `ProjectConfig` bleibt als Typ, es
   ist genau die Struktur, die jetzt in den Store wandert.
2. `Load(root, slug)`: globale Config wie bisher, Projekt-Teil aus dem Store.
   `loadProject` (Walk-up) und `decodeProjectConfig` fallen weg, ebenso
   `HasProjectContext`.
3. `writer.go`: `SaveProjectHost`/`DeleteProjectHost` schreiben den Store.
   `splitAccess`, `applyAccess`, `projectFileBase`, `environmentsOf`,
   `ensureProjectGitignore`, `hostsOut`-Sonderfälle für die Projekt-Datei
   entfallen. Ein Host ist wieder ein Host.
4. `MergedConfig`: `ProjectSecretsInFile`, `LegacySecretStore` und
   `ProjectRoot`-Semantik prüfen. `ProjectRoot` bleibt, er ist die Basis für
   Mappings und den lokalen Walker, kommt aber jetzt aus der Registry.
5. Aufrufer nachziehen: `cmd/root.go`, `cmd/projects.go`, `internal/tui/app.go`
   (`openProject`), jeweils Slug plus Root aus der Registry.

### Migration

Quellzustände, die es draußen gibt:

| # | Zustand |
| --- | --- |
| a | `.drift/config.toml` mit Klartext-Credentials (vor 0.1.6) |
| b | `.drift/config.toml` + `secrets.toml` (0.1.6) |
| c | `.drift/config.toml` + `access.toml` (0.1.7) |
| d | nur `~/.config/drift/projects/<slug>.toml` (Ziel) |

Ein Durchgang, Reihenfolge wie gehabt, Neues zuerst und Löschen zuletzt:

1. Projekt registrieren, falls nötig (Name = Verzeichnisname, Slug = `Slugify`).
2. `.drift/config.toml` lesen, `access.toml` und `secrets.toml` lesen, alles zu
   einem vollständigen `ProjectConfig` zusammensetzen. Bei Konflikt gewinnt der
   spezifischere Wert: was in der Projekt-Datei steht, hat ein Mensch dort
   hingeschrieben.
3. `projects/<slug>.toml` schreiben (atomar, `writeToml` kann das).
4. `.drift/config.toml` löschen. `.drift/` löschen, wenn darin höchstens noch
   drifts eigenes `.gitignore` liegt, byte-identisch mit `projectGitignore`. Ein
   `.drift/` mit fremdem Inhalt bleibt stehen.
5. Den Projekt-Eintrag aus `access.toml` entfernen; ist die Datei danach leer,
   löschen. `secrets.toml` genauso.
6. In der Statuszeile melden, was wohin gewandert ist, und **einmal** darauf
   hinweisen, dass eine committete `.drift/config.toml` weiterhin in der
   git-History steht: das Passwort darin ist verbrannt und gehört rotiert.
   Das ist der letzte Zweck von `ProjectConfigExposure`; danach kann
   `gitguard.go` weg.

Die Migration ist einmalig und nicht rückwärts fahrbar. Ältere drift-Versionen
finden nach ihr kein Projekt mehr, weil ihr Walk-up eine Datei sucht, die es
nicht mehr gibt. Das ist ein Breaking Change und gehört in den CHANGELOG, nicht
in eine Fußnote.

### Tests

- Load liest Hosts und Mappings aus dem Store, leerer Slug liefert nur globale Hosts
- Save/Delete schreiben den Store, andere Hosts bleiben unangetastet
- Migration aus (a), (b), (c); (d) schreibt nichts
- Konflikt Projekt-Datei gegen Store: Projekt-Datei gewinnt
- `.drift/` mit nur drifts `.gitignore` wird gelöscht, mit fremder Datei nicht
- `access.toml`/`secrets.toml` werden geleert und dann gelöscht
- Migration registriert ein unregistriertes Projekt
- `FindByPathPrefix`: längster Treffer gewinnt, Unterverzeichnis findet das Projekt,
  ein nicht registriertes Verzeichnis findet nichts

---

## Schritt 2 — Rooting und Registrierung

1. `Registry.FindByPathPrefix` (siehe Vorentscheidung 2).
2. `cmd/root.go`: Startmodus-Entscheidung ohne `HasProjectContext`. Registrierte
   Projekte kommen aus der Registry, sonst Dashboard oder Registrierungs-Prompt.
3. Registrierungs-Prompt schlägt den git-Root als Projekt-Root vor.
4. `internal/tui/app.go`: `openProject` arbeitet mit Slug plus Root.

### Tests

Start in einem Unterverzeichnis eines registrierten Projekts findet das Projekt;
Start in einem unregistrierten Verzeichnis führt zum Prompt; der vorgeschlagene
Root ist der git-Root, wenn es einen gibt.

---

## Schritt 3 — Aufräumen

Weg damit, wenn Schritt 1 und 2 stehen:

- `internal/config/gitguard.go` samt `ProjectConfigExposure` und `HasPlaintextSecret`
- `ensureProjectGitignore` und die `projectGitignore`-Konstante
- `msgSecretsMigrated`-Warnungstexte über git-Erreichbarkeit
- die Gruppenüberschriften im Host-Formular (`sectionHeader`) und der
  Access-Hinweis in der `PROJECT HOSTS`-Kopfzeile: es gibt keine zwei Schichten
  mehr, die man auseinanderhalten müsste
- die README-Abschnitte über Projekt-Config, `.gitignore` und Credential-Ablage,
  ersetzt durch einen Abschnitt über `~/.config/drift`

`HostScope` bleibt: global gegen projektbezogen ist weiter eine echte
Unterscheidung, nur liegen beide Seiten jetzt im selben Verzeichnis.

---

## Nicht Teil dieses Plans

- **Keyring.** Auf Servern und in SSH-Sessions gibt es keinen Secret Service,
  der Datei-Fallback bleibt nötig. Erst den richtig haben.
- **Ein gemeinsames Format für Registry und Store.** `projects.toml` bleibt die
  Liste, `projects/<slug>.toml` der Inhalt. Beides in eine Datei zu ziehen macht
  die Registry-Schreibvorgänge teurer und gewinnt nichts.
- **Team-Sharing über einen anderen Weg.** Wenn Mappings später doch geteilt
  werden sollen, ist das ein Export/Import-Kommando und keine Datei, die drift
  im Repo pflegt.
