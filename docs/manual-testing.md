# Manuelle Tests gegen SFTP und FTPS

Die Go-Tests decken Protokoll- und Ablauflogik ab. Was nur an der TUI sichtbar
wird, prüft diese Checkliste gegen lokale Server: Zertifikatsabfrage,
Richtungen in der Diff-Ansicht, Meldungen bei Verbindungsverlust. Alles läuft
lokal und braucht Docker, Go und Git.

## Aufbau

`scripts/manual-test/mt.sh` steuert das Testkit:

| Befehl | Wirkung |
|---|---|
| `mt.sh up` | startet SFTP auf `127.0.0.1:2222` und FTPS auf `127.0.0.1:2121` |
| `mt.sh drift` | baut drift aus dem aktuellen Checkout und startet es isoliert |
| `mt.sh ftps [bytes/s] [logins]` | startet FTPS neu, optional gedrosselt oder mit Login-Limit |
| `mt.sh rotate-cert` | FTPS bekommt ein neues Zertifikat |
| `mt.sh pause\|unpause sftp\|ftps` | friert einen Server ein oder lässt ihn weiterlaufen |
| `mt.sh kill sftp\|ftps` | beendet einen Server hart |
| `mt.sh status` | zeigt Server und Prüfsummen von `src/` und `assets/` im Projekt, der SFTP- und der FTPS-Seite sowie liegengebliebene Staging-Dateien |
| `mt.sh down` / `mt.sh reset` | stoppt die Server / löscht zusätzlich allen Zustand |

Alles, was das Kit schreibt, liegt in `scripts/manual-test/.state/`:

- `project/` ist das Testprojekt: `src/` mit drei Textdateien, `assets/big.bin`
  (3 MB), `.hidden` und ein per `.gitignore` ignoriertes `.env`.
- `sftproot/` und `ftproot/` sind die Serverseiten. Man kann dort direkt
  Dateien anlegen, ändern und löschen, um Remote-Änderungen zu erzeugen.
- `home/` ist das `HOME` der drift-Sitzung, mit eigener Konfiguration,
  `known_hosts` und `trusted-certificates.toml`. Deine echte Konfiguration
  bleibt unberührt.
- `drift.log` ist das Debug-Log der Sitzung.

Beide Hosts melden sich als `drift` mit Passwort `secret` an und prüfen die
Verbindung alle 5 Sekunden per Keep-alive.

Der FTPS-Server basiert auf `pyftpdlib`. Eine Anpassung weicht vom Original ab:
`MLSD` auf einen nicht vorhandenen Pfad beantwortet er mit `550` statt `501`,
so wie ProFTPD und Pure-FTPd. drift erkennt fehlende Dateien bewusst nur über
`550`, siehe AGENTS.md.

## Bedienung in drift

`Space` markiert im Browser, `s` startet den Vergleich. Im Hostselektor filtert
Tippen die Liste, zum Beispiel `sftp` oder `ftps`. In der Diff-Ansicht wechseln
`n`/`p` die Datei, `Space` ändert die Richtung, `s` synchronisiert die aktuelle
Datei, `S` alle, `u`/`d` laden die aktuelle Datei direkt hoch oder herunter,
`r` lädt neu, `i` bezieht ignorierte Dateien ein, `e` zeigt die Fehlerliste,
`q` geht zurück. Während eines Ladevorgangs bricht `q` ab.

Der Browser merkt sich den zuletzt verbundenen Host und vergleicht mit `s`
direkt dagegen. Für einen Hostwechsel drift mit `q` beenden und `mt.sh drift`
neu starten.

## Vorbereitung

```bash
scripts/manual-test/mt.sh reset   # nur für einen sauberen Neuanfang
scripts/manual-test/mt.sh up
scripts/manual-test/mt.sh drift   # in einem eigenen Terminal
```

In drift `assets/` und `src/` markieren. Die Befehle unten laufen in einem
zweiten Terminal im Repository, `.state` steht für
`scripts/manual-test/.state`.

## Diff-Darstellung

Diese Prüfungen sind zusätzlich zu den Protokolltests nötig und noch nicht
interaktiv abgenommen:

- Eine Textdatei mit mehr als 10.000 Zeilen vergleichen. Zeilennummern dürfen
  den rechten Rand nicht verschieben, auch nicht nach dem Aufklappen von Kontext.
- Tabs nach `日本`, `é` und `👩‍💻` vergleichen. Vierer-Tabstopps müssen sich
  an Terminalspalten ausrichten; Steuerzeichen dürfen nicht ausgeführt werden.
- Mit `v` Unified, Split und Auto durchschalten. Das Fenster vergrößern und
  verkleinern. Auto berücksichtigt die verbleibende Diff-Breite. Local/Remote,
  Vorher/Nachher und die mit Space gewählte Aktion müssen zusammenpassen.
- Eine lange Zeile mit einem Unterschied am Ende über die Pfeiltasten erreichen.
  Zeilennummern bleiben stehen. `?` öffnen: Navigation und Sync dürfen dahinter
  keine Wirkung haben. Mit `?`, Escape oder `q` schließen.
- Kontext aufklappen, scrollen, die Datei wechseln und zurückkehren. Danach `r`
  und einen erfolgreichen Sync prüfen. Zugeordnete Lesepositionen bleiben;
  verschwundene Dateien und geänderte Bereiche bekommen einen sicheren Rückfall.
- Eine Konfigurationszeile wie `timeout = 30` gegen `timeout = 60` vergleichen.
  Der geänderte Wert wird zusätzlich betont. Unicode-Zeichen bleiben vollständig;
  sehr lange oder ungleich große Änderungsblöcke bleiben beim Ganzzeilen-Diff.
- Während der Aufbereitung schnell Datei, Projekt und Host wechseln sowie die
  Ansicht schließen. Kein spätes Ergebnis darf in einem neuen Vergleich landen.

## SFTP

**S1. Erstvergleich und Upload.** `s`, Host `local-sftp`.
Erwartet: 4 Paare, alle ↑. Der Hostkey landet ohne Rückfrage in
`.state/home/.ssh/known_hosts`. Dann `S`.
Erwartet: "✓ synced 4 file(s)", danach ein leerer Vergleich. `mt.sh status`
zeigt für `project` und `sftproot` dieselben Prüfsummen.

**S2. Änderungen auf beiden Seiten.**

```bash
printf 'line 1\nline two\nline 3\nline 4\n' > .state/project/src/changed.txt
printf 'local\n' > .state/project/src/local-new.txt
head -c 3000000 /dev/urandom > .state/sftproot/assets/big.bin
printf 'remote\n' > .state/sftproot/src/remote-new.txt
```

In drift `r`. Erwartet: `big.bin` ↓ (gleiche Größe, anderer Inhalt, remote
neuer), `changed.txt` ↑ mit sichtbarem Textdiff, `local-new.txt` ↑,
`remote-new.txt` ↓. Dann `S`. Erwartet: `mt.sh status` zeigt beide Seiten
identisch.

**S3. Löschen.**

```bash
rm .state/project/src/same.txt .state/sftproot/src/nested/deep.txt
```

`r`, dann bei beiden Einträgen genau einmal `Space`: Der Pfeil wird zu ✗.
Ein weiteres `Space` stellt auf "keine Aktion" (—). Dann `S`.
Erwartet: "✓ synced 2 file(s)". `same.txt` fehlt remote, `deep.txt` lokal.

**S4. Hängender Server.** Diff-Ansicht offen lassen, `mt.sh pause sftp`.
Erwartet: nach etwa 20 Sekunden (5 s Intervall plus 15 s Timeout) die Meldung
"SSH keepalive timed out after 15s". `s`, `S`, `u` und `d` bewirken danach
nichts mehr. Zum Schluss `mt.sh unpause sftp`.

## FTPS

drift vorher beenden und neu starten, damit der Browser keinen SFTP-Host mehr
kennt.

**F1. Zertifikatsabfrage.** `s`, Host `local-ftps`.
Erwartet: das Modal "Certificate verification failed" mit
"unknown certificate authority". Den Fingerprint mit dem Serverlog vergleichen:

```bash
docker compose -f scripts/manual-test/compose.yml logs ftps | grep SHA-256
```

"Trust for this session" wählen. Erwartet: Der Vergleich lädt ohne Fehler pro
Datei. Ein `r` fragt nicht erneut.

**F2. Bulk-Upload.** `S`. Erwartet: `mt.sh status` zeigt für `project` und
`ftproot` dieselben Prüfsummen.

**F3. Dauerhaftes Vertrauen.** drift neu starten und vergleichen. Das Modal
erscheint wieder, weil das Sitzungsvertrauen weg ist. "Trust permanently"
wählen. Erwartet: Eintrag in `.state/home/.config/drift/trusted-certificates.toml`.
drift erneut neu starten: Diesmal lädt der Vergleich ohne Modal.

**F4. Zertifikatswechsel.** `mt.sh rotate-cert`, dann in drift `q` und `s`.
Erwartet: Das Modal erscheint mit neuem Fingerprint, das dauerhafte Vertrauen
gilt nicht für das neue Zertifikat. Mit `Esc` ablehnen. Erwartet: "FTPS
certificate was not trusted", kein Vergleich.

**F5. Abbruch mitten im Upload.**

```bash
head -c 3500000 /dev/urandom > .state/project/assets/big.bin
scripts/manual-test/mt.sh ftps 100000    # 100 KB/s, der Upload dauert etwa 35 s
```

In drift `q`, `s`, das Zertifikat bei Bedarf für die Sitzung vertrauen, dann
`S`. Nach einigen Sekunden `mt.sh kill ftps`.
Erwartet:

- "Remote disconnected … Reconnect and compare before syncing again",
  Sync-Tasten gesperrt
- in `e` oder `drift.log` genau ein Upload-Versuch mit "outcome unknown"
- `mt.sh status` zeigt für `ftproot/assets/big.bin` weiter die alte
  Prüfsumme und unter "staging files left behind" die abgebrochene
  `.big.bin.drift-tmp-…`

**F6. Staging-Reste bleiben außen vor.** `mt.sh up`, in drift `q` und `s`.
Erwartet: Die Staging-Datei steht nicht in der Liste, die Statuszeile meldet
"1 fixed excluded".

**F7. Login-Limit.** `mt.sh ftps 0 2`, in drift `q` und `s`.
Erwartet: Der Vergleich lädt vollständig. `drift.log` enthält "extra diff
worker connect failed, reducing parallelism".

**F8. Server nicht erreichbar.** `mt.sh down`, in drift `q` und `s`.
Erwartet: eine Fehlermeldung mit "connection refused". drift läuft weiter.

## Allgemein

**A1. Ignorierte Dateien.** Im Vergleich `i`. Erwartet: `.env` erscheint,
Statuszeile "include ignored: on". Erneut `i` schaltet zurück.

**A2. Abbruch beim Laden.** `r` und sofort `q`. Erwartet: zurück im Browser,
kein verspätetes Ergebnis, "cancelled network activity" in `drift.log`.

## Externer Remote-Editor

Diese Fälle sind zusätzlich mit SFTP und FTPS, nach Möglichkeit auch mit einem
FTP-Testhost, manuell zu prüfen. Das Kit setzt für drift einen eigenen
`XDG_CACHE_HOME` unter `.state/home/.cache`; Arbeitskopien landen nicht im
Testprojekt oder im echten Benutzer-Cache.

In `.state/home/.config/drift/config.toml` einen Editor-Abschnitt eintragen,
beispielsweise:

```toml
[editor]
command = ["nano", "{file}"]
terminal = true
```

Für GUI-Tests den Abschnitt durch `command = ["zed", "--wait", "{file}"]` und
`terminal = false` ersetzen. Dasselbe mit `code --wait` prüfen, falls installiert.
Nach Konfigurationsänderungen drift neu starten; keine echten Zugangsdaten nötig.

**E1. Unverändert.** Mit `@` verbinden, per `Tab` ins Remote-Pane wechseln, eine
Datei auswählen und `e` drücken. Ohne Änderung schließen. Erwartet: kein Upload,
Browser-Cursor, offene Verzeichnisse und Markierungen bleiben erhalten. Bei Nano
und Neovim Alt-Screen und Maus sowohl mit als auch ohne Mausunterstützung prüfen;
Terminal während der Bearbeitung verkleinern/vergrößern.

**E2. Bestätigter Upload.** Eine Remote-Datei bearbeiten und speichern, den Editor
noch offen lassen. Erwartet: Serverseite und lokale Projektdatei unverändert.
Datei/Editor schließen. `Enter` öffnet erneut, ohne Upload. Wieder schließen und
`u` drücken. Erwartet: nur die Remote-Datei ändert sich; Metadaten und eine vorher
aktive Preview werden aktualisiert. Im GUI-Test muss erst das Schließen des
Dokuments, nicht schon das Ende des Launchers, zur Bestätigung führen.

**E3. Konflikt.** Während der Bearbeitung dieselbe Datei in `.state/sftproot/`
beziehungsweise `.state/ftproot/` anderweitig ändern. Auch gleich lange Inhalte
verwenden. Nach Editor-Ende `u`. Erwartet: Konflikt, kein Überschreiben. `k` erhält
die Arbeitskopie; `r` fragt vor dem Verwerfen und Neuladen. Dasselbe mit beobachteter
Löschung oder entzogenem Leserecht prüfen; ein FTP `550` ist kein Löschungsbeweis.

**E4. Prozess- und Verbindungsfehler.** Einen ungültigen Editor-Programmnamen
eintragen oder den Editor mit Fehler beenden. Erwartet: kein Upload, Arbeitskopie
bleibt erhalten. Während einer weiteren Sitzung den Server stoppen. Nach Rückkehr
Fehler anzeigen, nicht automatisch übertragen. Server neu starten und `c` drücken:
Verbindung/Prüfung allein laden nichts hoch. Bei neuem FTPS-Zertifikat gilt das
auch nach Trust-Zustimmung. Erst ein neues `u` darf erneut übertragen.

**E5. Behalten, Verwerfen und Grenzen.** `Esc`/`q` ohne Entscheidung verliert keine
Änderungen. `k` zeigt den erhaltenen Cache-Pfad; `d` verlangt eine separate
Bestätigung. Mehr als 10 MiB, Verzeichnisse, Symlinks und ungemappte Pfade starten
keine normale Bearbeitung. Fehlgeschlagene oder unklare Uploads dürfen weder Erfolg
melden noch die Arbeitskopie löschen. Auf diesem Testpfad kein verlässliches
sofortiges Rückgängigmachen bereits begonnener Transfers erwarten.

## Aufräumen

```bash
scripts/manual-test/mt.sh reset
docker image rm drift-manual-test-ftps atmoz/sftp:alpine   # optional
```
