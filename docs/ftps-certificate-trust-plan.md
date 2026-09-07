# Implementierungsplan: FTPS-Zertifikaten gezielt vertrauen

Status: geplant, noch nicht implementiert.
Branch: `feature/ftps-certificate-trust`
Ausgangspunkt: `fc1f3c0` auf `main`.

## Ziel und Umfang

Die pauschale Option `insecure_tls` durch eine Zertifikatsabfrage ersetzen.
Standard bleibt die Prüfung gegen den System-Zertifikatsspeicher einschließlich
Hostname und Gültigkeit. Eine Ausnahme gilt ausschließlich für ein ausdrücklich
bestätigtes Zertifikat an einem bestimmten FTPS-Endpunkt.

Zum ersten Lieferumfang gehören:

- Ablehnen, für die laufende drift-Sitzung vertrauen oder dauerhaft vertrauen.
- Anzeige der Prüfungsfehler und des SHA-256-Fingerabdrucks.
- Sichere Wiederaufnahme von Verbindungstest, Remote-Browser und Diff-Laden.
- Einheitliche Prüfung aller Steuer-, Daten- und zusätzlichen Diff-Verbindungen.
- Gespeichertes Vertrauen für einen Host im Hostmanager zurücksetzen.
- Tests und Dokumentation einschließlich des Wegfalls von `insecure_tls`.

Nicht Bestandteil dieses Features sind eigene CA-Dateien, Client-Zertifikate,
implizites FTPS, Änderungen an SSH/SFTP und die Freigabe von TLS 1.3.
TLS 1.2 bleibt vorerst fest eingestellt. Eigene CA-Dateien sind ein sinnvoller
Folgeschritt für interne Server und reguläre Zertifikatserneuerungen.

## Bestehende Ansatzpunkte

| Datei | Aktuelles Verhalten und notwendige Anpassung |
| --- | --- |
| `internal/ftp/client.go` | `Connect` baut die TLS-Konfiguration mit `InsecureSkipVerify: host.InsecureTLS`. Hier die neue Prüfungsrichtlinie anwenden. |
| `internal/remote/client.go` | Gemeinsame Verbindungsfabrik. Laufzeit-Vertrauen explizit bis zum FTP-Client durchreichen; kein direkter Protokollzugriff aus der TUI. |
| `internal/config/config.go`, `internal/config/writer.go` | `Host.InsecureTLS` und das Encoding-Feld entfernen; Trust-Store-Persistenz ergänzen. |
| `internal/tui/hostform/{model,update,view}.go` | Toggle `Insecure TLS` entfernen, Fokusreihenfolge und Tests anpassen. |
| `internal/tui/hostmanager/update.go` | `testCmd` und `MsgTestResult` um sichere Fehlerbehandlung und Wiederaufnahme ergänzen. |
| `internal/tui/browser/remote.go` | `loadRemoteCmd` und spätere Verzeichnisabfragen können Zertifikatsfehler liefern. |
| `internal/tui/diffview/model.go` | `LoadCmd`, `forEachCompare`, `loadDiffItems`: Vertrauen weiterreichen und Sicherheitsfehler vollständig hochreichen. |
| `internal/tui/app.go`, `internal/tui/state.go` | Dialog, Request-Zuordnung, Abbruch und Wiederaufnahme koordinieren. |
| `internal/tui/styles.go`, `internal/styles/styles.go` | Dialoggestaltung in den bestehenden Style-Paketen definieren. |

Die verwendete Bibliothek `github.com/jlaffaye/ftp v0.2.0` verwendet dieselbe
TLS-Konfiguration für Steuer- und Datenverbindungen. Der Handshake erfolgt teils
erst beim ersten Lesen oder Schreiben. Ein Zertifikatsfehler kann daher auch aus
`Login`, `ReadDir`, Dateiübertragungen oder dem Schließen eines Transfers kommen.
Nur den Fehler aus `Dial` zu behandeln reicht nicht.

Zusätzliche FTP-Diff-Worker reduzieren derzeit bei Verbindungsfehlern lediglich die
Parallelität. Zertifikatsfehler dürfen auf diesem Weg nicht unbemerkt verschwinden.

## Sicherheitsregeln

1. Regulär gültige Zertifikate werden ohne Dialog akzeptiert. Gespeicherte
   Ausnahmen sind kein verpflichtendes Pinning für ansonsten gültige Zertifikate.
2. Eine Ausnahme ist an `ftps`, den normalisierten konfigurierten Hostnamen,
   den tatsächlich verwendeten Steuer-Port und SHA-256 über das vollständige
   DER-Blattzertifikat gebunden. Kein Vertrauen nach Anzeigename, Projekt,
   aufgelöster IP, Wildcard oder bloßem öffentlichen Schlüssel.
3. Der effektive Port wird nach der vorhandenen Konfigurationsauflösung bestimmt.
   Nicht pauschal 21 als Trust-Schlüssel verwenden. Passive Datenports bilden
   keine eigenen Vertrauenseinträge; die Identität bleibt der Steuer-Endpunkt.
4. Bei DNS-Namen Groß-/Kleinschreibung und einen abschließenden Punkt eindeutig
   normalisieren; IP-Adressen kanonisieren. Für Netzwerkadressen `net.JoinHostPort`
   verwenden. DNS-Name und IP-Adresse bleiben verschiedene Identitäten.
5. Nur ausdrücklich bestätigte Prüfungsprobleme dürfen übergangen werden.
   Unbekannte CA, falscher Hostname, abgelaufen und noch nicht gültig werden
   getrennt ausgewiesen. Ein später hinzukommender Fehler verlangt eine neue
   Entscheidung, auch wenn der Fingerabdruck gleich bleibt.
6. Nicht unterstützte Prüfungsfehler, fehlende/unlesbare Zertifikate,
   Protokollfehler und fehlgeschlagene Handshake-Signaturen sind nicht freigebbar.
   Kein Rückfall auf unverschlüsseltes FTP oder pauschales Überspringen der Prüfung.
7. Ein anderes, weiterhin regulär ungültiges Zertifikat verlangt erneut eine
   Entscheidung. Bei vorhandenem Eintrag alten und neuen Fingerabdruck zeigen.
   Eine neue dauerhafte Bestätigung ersetzt den bisherigen Endpunkt-Eintrag;
   alte Zertifikate bleiben nicht zusätzlich freigeschaltet.
8. Beim Wiederverbindungsversuch direkt nach Bestätigung muss das Zertifikat
   exakt dem gerade angezeigten entsprechen. Auch ein inzwischen regulär gültiges,
   anderes Zertifikat wird in diesem Versuch nicht stillschweigend übernommen.
9. Sitzung bedeutet die Laufzeit einer drift-Instanz, auch über Projektwechsel
   hinweg. Sitzungseinträge werden niemals in Host-Konfigurationen geschrieben.
10. Eine fehlgeschlagene dauerhafte Speicherung erteilt kein Sitzungsvertrauen
    als Ersatz. Fehler anzeigen und eine neue explizite Auswahl ermöglichen.
11. Zurücksetzen entfernt Sitzungs- und dauerhafte Ausnahmen des Endpunkts und
    schließt dessen noch offene Verbindungen. Bereits laufende Transfers vorher
    abbrechen bzw. geordnet beenden; kein nachträglicher Erfolgsanspruch.

Das ist Ausnahmeverwaltung, kein vollwertiger Zertifikat-Manager. Insbesondere
Zertifikatswiderruf über OCSP/CRL wird hier nicht neu implementiert oder versprochen.
Für wechselnde selbst signierte Zertifikate hinter einem Loadbalancer gibt es
keine automatische Sammelfreigabe; eine vertrauenswürdige CA ist dafür geeigneter.

## Dialog und Bedienung

Neue Screen-Komponente `internal/tui/certtrust/{model,update,view}.go`, vom Root-Modell
als Modal mit gespeichertem Rückkehr-Screen verwaltet. UI-Texte bleiben wie im
bestehenden Programm Englisch.

```text
Certificate verification failed

Server:        staging.example.com:21
Problems:      Unknown certificate authority
Subject:       staging.example.com
DNS/IP names:  staging.example.com
Issuer:        Development CA
Valid from:    2026-01-01 00:00 UTC
Valid until:   2026-12-31 23:59 UTC
SHA-256:       AB:CD:...

Verify this fingerprint through a trusted channel.
Encryption alone does not confirm the server's identity.

[Reject]  [Trust for this session]  [Trust permanently]
```

- `Reject` ist vorausgewählt; Enter bestätigt nur die sichtbare Auswahl.
- Tab/Pfeiltasten wechseln die Auswahl. Esc lehnt ab; Ctrl+C beendet wie vorgesehen
  den Vorgang bzw. die Anwendung, ohne Vertrauen zu speichern.
- Abgelaufene Zertifikate, Hostnamenfehler und Zertifikatswechsel deutlich markieren.
  Alle erkannten Probleme anzeigen, nicht nur den ersten Fehler aus `x509.Verify`.
- Lange Zertifikatsdetails scrollbar darstellen. Der vollständige Fingerabdruck
  muss auch in kleinen Terminals erreichbar sein.
- Zertifikatsfelder sind fremde Eingaben: Steuerzeichen und ANSI-Sequenzen vor
  Darstellung und Logging entschärfen; angezeigte Datenmengen begrenzen.
- Tastatur, Maus und Resize nur an das Modal weitergeben. Keine Klicks auf den
  darunterliegenden Screen. Bestehenden Loading-Dialog vorher beenden.
- Im Hostmanager eine Aktion `Reset certificate trust` für FTPS-Endpunkte anbieten.
  Vor dem Entfernen bestätigen und darauf hinweisen, dass dieselbe Adresse auch
  von anderen Host-Einträgen/Projekten verwendet werden kann.

## Technik und Datenfluss

### Prüfungslogik und Fehler

Ein kleines Paket `internal/tlstrust` bündelt Zertifikatsprüfung, Fingerabdruck,
normalisierten Endpunkt und typisierte Prüfungsfehler. Keine Abhängigkeit von
Bubble Tea oder den Protokoll-Clients; keine allgemeine Authentifizierungsplattform.

- Prüfungsfehler enthalten Endpunkt, Zertifikatsdaten, freigebbare Problemcodes
  und die ursprüngliche Ursache. `errors.As` und `Unwrap` müssen durch alle
  Fehlerhüllen bis zur TUI funktionieren.
- Regulär mit Go `crypto/x509` prüfen: System-Roots, gelieferte Intermediate-CAs,
  ServerAuth, Hostname und aktuelle Zeit. Problemcodes strukturell bestimmen,
  nicht durch Vergleich englischer Fehlermeldungen.
- Für gezielte Ausnahmen ist `tls.Config.VerifyConnection` geeignet. Die normale
  automatische Prüfung läuft vor diesem Callback und kann nicht darin aufgehoben
  werden. Falls dafür intern `InsecureSkipVerify: true` nötig ist, muss der
  Callback die vollständige Prüfung selbst übernehmen. Das Flag ist dann ein
  Implementierungsdetail, niemals eine Nutzeroption oder ein ungeschützter Pfad.
- Tests müssen insbesondere belegen, dass das Freigeben eines Problems keine
  anderen Prüfungen ausschaltet. Vor Implementierung der TUI zuerst diesen
  Prüfungsalgorithmus samt Fehlerklassifikation fertigstellen.
- Callback führt keine UI-Interaktion, Schreibzugriffe oder wartende Rückfrage aus.
  Er entscheidet nur anhand der Zertifikate und eines unveränderlichen
  Vertrauens-Snapshots. Auch Session-Resumption muss die Prüfung durchlaufen.

Das Root-Modell besitzt den Laufzeit-Vertrauenszustand. Die Verbindungsfabrik erhält
explizit eine konkrete Trust-Policy/Snapshot als zusätzlichen Parameter. Alle
Aufrufer einschließlich zusätzlicher Diff-Worker und Tests werden angepasst.
Keine globale veränderliche Variable, kein Transport über `config.Host`, kein
versteckter Context-Wert und kein alter Signatur-Wrapper. FTP/SFTP bleiben von
der FTPS-Policy unberührt. Die aktuelle API-Regel in `AGENTS.md` mit aktualisieren.

### Persistenz

Neue Datei `<config.Dir()>/trusted-certificates.toml`, keine Datei im Projekt.
Persistenz in `internal/config/trusted_certificates.go` mit eigenen Datenstrukturen;
`internal/tlstrust` darf diese verwenden, `config` importiert `tlstrust` nicht.

Pro Eintrag speichern:

- Protokoll, normalisierten Hostnamen und effektiven Steuer-Port.
- SHA-256-Fingerabdruck des Blattzertifikats.
- Ausdrücklich akzeptierte Problemcodes.
- Bestätigungszeit in UTC.

Zertifikatsdetails für den Dialog stammen immer vom aktuellen Verbindungsversuch,
nicht aus möglicherweise veralteten gespeicherten Anzeigefeldern.

`writeToml` für atomare Ersetzung und Dateimodus 0600 wiederverwenden; Verzeichnis
bei Bedarf mit 0700 erstellen. Fehlende Datei bedeutet keine Ausnahmen. Parse-,
Validierungs- und I/O-Fehler sichtbar melden, nicht als leeren Store behandeln.
Unbekannte Problemcodes und mehrdeutige doppelte Endpunkt-Einträge ablehnen.

Schreiben und Zurücksetzen zwischen drift-Instanzen sperren, dann den Store neu
lesen, gezielt ändern und atomar ersetzen. Atomisches Rename allein schützt nicht
vor verlorenen Änderungen oder dem Wiederherstellen gerade gelöschter Einträge.
Die Sperre muss bei Prozessende freigegeben werden; eine plattformgerechte
Implementierung in diesem Store halten, keine allgemeine Storage-Abstraktion bauen.
Laufende andere Instanzen können vorhandene TLS-Verbindungen behalten; die
Dokumentation muss diese Grenze nennen. Vor neuen Verbindungsversuchen den
persistenten Stand neu einlesen statt ihn unbegrenzt zu cachen.

### TUI und Wiederaufnahme

1. Der gestartete `tea.Cmd` beendet sich bei einem Zertifikatsfehler und räumt
   seine Ressourcen auf. Das Root-Modell erhält einen typisierten Fehler über den
   bestehenden Ergebnispfad, stoppt den Loader und öffnet das Modal.
2. Einen konkreten ausstehenden Vorgang speichern: Ursprung, Request-ID,
   Host-Snapshot, Projektidentität und für Lesevorgänge unveränderliche Auswahl/
   Pfade. Keine beliebigen Retry-Closures und kein Wiederverwenden bereits
   abgebrochener Contexts oder geschlossener Clients.
3. Vor Öffnen und vor Anwenden einer Entscheidung die Request-ID prüfen.
   Verspätete Ergebnisse abgebrochener Browser-, Hosttest- oder Diff-Anfragen
   dürfen weder ein Modal öffnen noch Vertrauen speichern. Fehlende Identitätsdaten
   in `MsgTestResult` und `MsgRemoteChildrenLoaded` ergänzen.
4. Ablehnen beendet den Vorgang mit verständlichem Status. Nichts wird gespeichert.
5. Sitzungsbestätigung aktualisiert den Laufzeit-Zustand. Dauerhafte Bestätigung
   schreibt zuerst in einem `tea.Cmd`; erst ein erfolgreiches, noch aktuelles
   Speicherergebnis erlaubt die Wiederaufnahme. Eine schon erfolgreich gespeicherte
   Ausnahme wird durch einen späteren Vorgangsabbruch nicht heimlich zurückgerollt.
6. Neue Request-ID, neuer Tracker und neuer Timeout starten den Lesevorgang bzw.
   Verbindungstest erneut. Wartezeit im Dialog verbraucht kein Netzwerk-Timeout.
   Auswahl und Ursprung bleiben erhalten; kein Start im inzwischen gewählten Projekt.
7. Gleichzeitig auftretende Fehler eines laufenden Auftrags sammeln bzw. identische
   Herausforderungen zusammenfassen. Ein Modal zur Zeit, keine Dialogserie pro Worker.
   Abbruch und Projektwechsel machen die ausstehende Wiederaufnahme ungültig.

Auch Zertifikatsfehler aus Verzeichnisscans, einzelnen Diff-Sessions und zusätzlichen
Workern müssen den Root-Pfad erreichen. Sicherheitsfehler dürfen weder als fehlende
Datei noch nur als reduzierte Parallelität erscheinen. Dazu Fehler-Rückgaben von
`forEachCompare`/`loadDiffItems` bei Bedarf erweitern, Worker beenden und neue
Jobs stoppen. Sonstige optionale Worker-Verbindungsfehler dürfen weiterhin die
Parallelität reduzieren.

Bei Sync-Fehlern aus Datenverbindungen die betroffene Verbindung verwerfen und
fehlgeschlagene oder möglicherweise teilweise ausgeführte Aktionen sichtbar halten.
Der Dialog darf Vertrauen erteilen, aber Uploads, Downloads und Löschvorgänge werden
**nicht automatisch wiederholt**. Vor einem neuen Sync erneut vergleichen und
explizit bestätigen. Bereits erfolgreiche Dateien nicht als fehlgeschlagen umdeuten.

Verbindungs-, Prüfungs-, Speicher- und Sync-Fehler über `internal/log` protokollieren.
Vertrauensentscheidungen mit Endpunkt, Fingerabdruck und Dauer dokumentieren, nicht
mit Passwörtern oder vollständigen Zertifikats-Dumps. Bei deaktiviertem Logging
weiterhin keine Logdatei und keine Ausgabe in stdout/stderr.

## Umsetzungsschritte

### 1. Prüfungsregeln und Tests

- `internal/tlstrust` mit Endpunkt, Fehlerdaten und enger Ausnahmeprüfung anlegen.
- Testzertifikate zur Laufzeit erzeugen; Gültigkeitszeiten relativ zur Testzeit.
- Vollständige Standardprüfung und Fehlerkombinationen vorab absichern.
- Festlegen und testen, welche Fehler freigebbar sind und welche zwingend abbrechen.

Abnahme: Eine Bestätigung schaltet ausschließlich das angezeigte Zertifikat mit
den bestätigten Problemen am ausgewählten Endpunkt frei.

### 2. Trust-Store und Laufzeit-Policy

- TOML-Lesen, Validierung, atomare Speicherung und Zurücksetzen implementieren.
- Sitzungszustand und persistente Ausnahmen getrennt halten.
- Unveränderliche Snapshots und synchronisierte Store-Änderungen verwenden.
- Alle Tests auf temporäres `XDG_CONFIG_HOME` ausrichten.

Abnahme: Persistentes Vertrauen überlebt Neustarts, Sitzungsvertrauen nicht.
Speicherfehler und konkurrierende Schreibzugriffe erweitern Vertrauen nicht.

### 3. Verbindungsfabrik und FTPS integrieren

- `remote.Connect` und alle Aufrufer auf die konkrete Laufzeit-Policy umstellen.
- Prüfungs-Callback auf alle FTPS-Verbindungen anwenden, Fehlerketten erhalten.
- Prüfung bis zum tatsächlichen Handshake, auch im Datenkanal, testen.
- Sicherheitsfehler aus parallelen Vergleichen und Transfers hochreichen.
- `Host.InsecureTLS`, Encoding-Feld und Formular-Toggle im selben Schritt entfernen.

Abnahme: Ohne TUI-Bestätigung scheitern ungültige Zertifikate sicher. Plain FTP und
SFTP verhalten sich unverändert. Kein Codepfad besitzt eine pauschale TLS-Freigabe.

### 4. Dialog, Wiederaufnahme und Zurücksetzen

- `certtrust`-Screen, Root-Routing, Größen-/Mausbehandlung und Styles ergänzen.
- Request-gebundene Entscheidungen und Wiederaufnahme für die drei Einstiegspfade
  implementieren: Hosttest, Remote-Browser und Diff-Laden.
- Späte Datenkanalfehler und Sync-Abbruch ohne automatischen Transfer-Retry behandeln.
- Hostmanager-Aktion zum Zurücksetzen samt Verbindungsschließung ergänzen.

Abnahme: Ein vollständiger Ablauf von Prüfungsfehler über Entscheidung bis zur
Wiederaufnahme funktioniert ohne blockierendes `Update` und ohne verlorene Auswahl.

### 5. Dokumentation und Gesamtprüfung

- `README.md`, `CHANGELOG.md` und `AGENTS.md` aktualisieren.
- Wegfall von `insecure_tls` ausdrücklich als Verhaltensänderung dokumentieren.
  Alte Einträge erteilen kein Vertrauen, auch wenn der TOML-Decoder unbekannte
  Felder ignoriert. Keine automatische Übernahme und kein Kompatibilitäts-Shim.
- Speicherort, Sitzungsgültigkeit, Zurücksetzen, Fingerabdruckprüfung und Grenzen
  anderer bereits laufender Instanzen erklären.
- Eigene CA-Dateien und TLS 1.3 als getrennte Folgethemen belassen.

## Test- und Abnahmematrix

Keine gemockten Remote-Clients. Für Transporttests echte Loopback-Verbindungen mit
lokalem TLS/FTPS-Testserver verwenden; vorhandene Integrationstest-Konventionen
übernehmen. Externe Server nur optional und mit dokumentiertem Skip. Die
Sicherheits-Kerntests dürfen nicht von externen Zugangsdaten abhängen.

| Bereich | Pflichtfälle |
| --- | --- |
| Standardprüfung | Vertrauenswürdige Test-CA ohne Dialog, unbekannte CA, falscher DNS-/IP-Name, abgelaufen, noch nicht gültig, unvollständige Kette. |
| Ausnahmen | Richtiger Fingerabdruck/Endpunkt akzeptiert; anderer Port, anderer Host oder anderes Zertifikat abgelehnt; mehrere Probleme nur nach vollständiger Bestätigung. |
| Lebenszyklus | Unverändertes Zertifikat läuft später ab und fragt erneut; ungültige Erneuerung fragt erneut; regulär gültige Erneuerung braucht keine Ausnahme. |
| Retry | Zertifikat wechselt zwischen Dialog und Wiederverbindung; kein stilles Vertrauen, keine Login-Daten vor erfolgreicher Steuerkanalprüfung. |
| Transport | Explizites TLS, geschützte Datenverbindung, abweichendes Datenzertifikat, leere Datei, Fehler aus Read/Write/Close, Session-Resumption sofern aktiviert. |
| Store | Roundtrip, 0600, fehlende/defekte Datei, unbekannte Codes, Schreibfehler, konkurrierende Änderungen, Zurücksetzen, isoliertes Config-Verzeichnis. |
| TUI | Alle drei Entscheidungen, Enter auf Reject, Esc/Ctrl+C, Maus, Resize, lange und manipulierte Zertifikatsfelder, ausbleibender Schreib-Erfolg. |
| Request-Sicherheit | Veraltete Hosttest-/Browser-/Diff-Ergebnisse, Abbruch während Verbindung und Speicherung, Projekt-/Hostwechsel, zusammengefasste Worker-Fehler. |
| Diff/Sync | Fehler bei Scan/Compare/zusätzlichem Worker sichtbar; Ressourcen geschlossen; keine automatische Wiederholung schreibender Vorgänge; Teilerfolge bleiben erkennbar. |
| Regression | Alter `insecure_tls = true`-Eintrag umgeht keine Prüfung; FTP/SFTP unverändert; Hostformular-Fokus und Konfigurations-Roundtrips. |

Nach der späteren Implementierung:

```sh
go test ./...
go test -race ./...
go vet ./...
go build ./...
make update
```

Vor Build und Tests die geänderten Go-Dateien per LSP prüfen. Abschließend manuell
mit einem lokalen FTPS-Server die Ablehnung, Sitzungsausnahme, dauerhafte Ausnahme,
Zertifikatsänderung und das Zurücksetzen durchspielen. Erst nach erfolgreicher
Validierung den Feature-Branch als Pull Request nach `main` anbieten.
