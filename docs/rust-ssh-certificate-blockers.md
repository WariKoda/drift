# SSH-Hostzertifikate: gesperrter Prototyp

Stand: 5. Oktober 2026. **Keine Release-Freigabe, kein freigegebener PR.**

Der lokale Branch `feature/rust-ssh-host-certificates` enthält einen Prototyp
auf Basis von `5f8015c`. Er wird nicht in die folgenden Arbeitsblöcke übernommen.
Der reguläre Rust-Stand weist Hostzertifikate weiterhin ab. Go bleibt unverändert.

## Vorbereitet und gezielt geprüft

- Hosttyp, kryptografische CA-Signatur, Gültigkeitsgrenzen, exakte angefragte
  Hostname-Principals ohne Port/IP-Fallback; leere Principals bleiben erlaubt.
- Endpoint-scoped `@cert-authority`, Hashes, Wildcards/Negation und Ports;
  vollständiges Parsen vor Annahme, globale CA-/Leaf-Revocation.
- Normale Zertifikats-Pins ersetzen weder CA-Vertrauen noch einen Raw-Key-Pin.
  Zertifikatszeilen und Zertifikats-Revocations werden dennoch eingelesen.
- Kein Zertifikats-TOFU, keine Trust-Datei/-Verzeichnisanlage nach Ablehnung,
  keine TLS-Freigabe, kein automatischer Retry, kein neues Konfigurationsschema.
- Unabhängige Leaf-/CA-Algorithmen und sichere Zertifikatsangebote; bestehende
  Raw-Key-Präferenzen bleiben erhalten. Ein unbekanntes Zertifikat wird geprüft,
  statt bei verfügbarem Raw-Key-Angebot still in Raw-TOFU zu wechseln.
- Alle nicht implementierten Critical Options werden abgewiesen, einschließlich
  `source-address`. Revocation eines Zertifikats sperrt auch dessen Leaf-Key.
  Beides ist bewusst strenger als die eingebundene Go-Version `x/crypto v0.49.0`.

**42 gezielte Tests bestanden:** 23 generierte echte Schlüssel/Zertifikate,
15 echte Core-SFTP-Serverfälle, 2 App- und 2 Headless-GPUI-Fälle.
Die GUI-Fälle benutzen reale Verbindungen und native Controls. Das ist weder
vollständige Workspace-/CI-Prüfung noch native Plattform- oder Sicherheitsabnahme.

## Abhängigkeitsblocker

Die statische Prüfung des gepinnten `russh 0.63.3` und `ssh-key 0.7.0-rc.11`
fand drei Probleme, die der Anwendungscallback nicht vollständig beheben kann.
Sie sind auch im bei dieser Prüfung abgerufenen Upstream-Code enthalten.
Es wurde kein Exploit-/Crash-Nachweis ausgeführt und kein Advisory veröffentlicht.

1. Der Zertifikats-Signer wird über allgemeines `KeyData::decode` eingelesen.
   Zertifikatswertige Signer können rekursiv geparst werden, bevor der
   Anwendungscallback läuft. Tiefe bzw. Ressourcenverbrauch sind nicht begrenzt;
   die tatsächliche Absturzschwelle ist nicht nachgewiesen.
2. KEX prüft die Exchange-Signatur gegen den Leaf-Key, gleicht den
   Signaturalgorithmus aber nicht mit dem ausgehandelten Algorithmus ab.
   Die eingebundenen RSA-Features erlauben SHA-1-Verifikation; ein SHA-2-
   Zertifikatsangebot allein erzwingt damit keine SHA-2-Exchange-Signatur.
3. Beim Rekey wird `check_server_key` nicht erneut aufgerufen. CA, Scope,
   Principal, Gültigkeit, Critical Options und Revocation eines Ersatz-
   Zertifikats werden dadurch nicht erneut geprüft.

Referenzstellen in den lokalen Registry-Quellen:
`russh/src/client/kex.rs`, `russh/src/client/mod.rs`,
`ssh-key/src/certificate.rs`, `ssh-key/src/public/key_data.rs`.
Algorithmusprüfung und Rekey betreffen auch bereits vorhandene Raw-Key-Abläufe;
der SSH-Release-Gate darf nicht nur den neuen Zertifikatspfad prüfen.

## Entscheidung und Wiederaufnahme

Der Nutzer hat entschieden: **Block dokumentieren, Prototyp separat sichern,
SFTP-Zielersatz fortsetzen.** Kein Dependency-Vendoring/-Fork, kein Umgehen der
Prüfung und keine Freischaltung über einen vermeintlich sicheren Callback.

Wiederaufnahme verlangt korrigierte Upstream-Abhängigkeiten und Regressionen
für begrenztes Signer-Decoding, ausgehandelten Exchange-Signaturalgorithmus
und erneute Host-/Zertifikatsprüfung bei jedem Rekey. Danach folgen vollständige
Workspace-/CI-Prüfung und native Plattformabnahme. Der SSH-Arbeitsblock bleibt offen.
