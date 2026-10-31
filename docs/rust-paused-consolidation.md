# Pausierte Rust-Arbeiten: gemeinsamer Sicherungsbranch

`chore/rust-paused` bündelt die verbliebenen Rust-Arbeiten auf Basis von
`main` (`ada5fad`). Rust-/GPUI-Entwicklung bleibt pausiert. Das Zusammenführen
ist eine Sicherung, keine Wiederaufnahme, Release-Freigabe oder Installation.
`main` und das installierte Go-Binary bleiben unverändert.

## Enthaltene Stände

| Herkunft | Gesicherter Stand | Übernahme |
| --- | --- | --- |
| `fix/rust-window-identity` | `0ba1ae7` | Fenstertitel/App-ID, Desktop-Metadaten, Tests und begrenzte native Wayland-Nachweise; Merge `e6cfb8e`. |
| `feature/rust-ssh-host-certificates` | `7db6671`, `b605dda` | Gesperrter Hostzertifikats-Prototyp samt Transportcode, echten Verbindungs-/GUI-Tests und Blocker-Dokumentation; Merge `b8e2326`. |
| `feature/rust-gui-packaging` | lokal `55b73ad`, zuletzt remote `f5dd53d` | Bereits vollständig in der `main`-Basis enthalten; kein erneuter Code-Merge erforderlich. |
| `feature/rust-workspace-design` | `676069f` | Bereits zuvor über `1af07c6` in `main` gesichert; dadurch ebenfalls enthalten. |

Die ursprünglichen Commits bleiben über die Merge-Historie erreichbar. Nach
Sicherung des Sammelbranches können die alten Branch-Namen und die zusätzlichen
Worktrees entfallen; historische Pfade/Branch-Namen in nativen Prüfnachweisen
beschreiben weiterhin deren damalige Umgebung.

## Integrationsanpassungen

- Dokumentationskonflikte behalten den neueren Workspace-/Theme-/Paketierungsstand
  und die zusätzlichen Fensteridentitätsnachweise. Veraltete Hinweise auf separate
  aktive Branches verweisen jetzt auf den Sammelbranch.
- Der vorhandene SFTP-Lifecycle-Test ergänzt den vom Prototyp eingeführten
  angefragten Hostnamen im echten Verifier.
- Die beiden älteren SSH-GUI-Tests bedienen den aktuellen nativen Hostpicker und
  die unabhängige Remote-Vorschau, statt inzwischen entfernter Controls/Felder.
- Zwei verschachtelte Bedingungen im Prototyp wurden ohne Verhaltensänderung für
  das aktuelle strikte Clippy zusammengefasst.

Keine Dependency-Aktualisierung, kein Fork, keine neue Funktion und keine
Umgehung der [SSH-Sicherheitsblocker](rust-ssh-certificate-blockers.md).
Insbesondere enthält dieser Sicherungsbranch experimentellen Zertifikatscode,
der wegen Parser-, Exchange-Signatur- und Rekey-Problemen **nicht für produktive
Nutzung oder Releases freigegeben** ist. Grüne Tests beheben diese Blocker nicht.

## Lokale Validierung des zusammengeführten Standes

- `make rust-check`: Rustfmt und striktes Workspace-/All-target-Clippy bestanden.
- `make rust-test`: **601 bestanden, 0 fehlgeschlagen, 0 ignoriert**; einschließlich
  realer SFTP-/FTP-/FTPS-Verbindungen und der beiden übernommenen SSH-GUI-Tests.
- Python-Paketierung: **29 Tests ausgeführt, davon 28 bestanden und 1 übersprungen**;
  der privilegierte Prozessfall benötigt weiterhin einen Root-Supervisor.
- `make test vet build` und `go test -race -count=1 ./...`: bestanden.
- Ausführende lokale Rust-/Cargo-Version: **1.99.1**. Das eingecheckte
  `rust-toolchain.toml` bleibt unverändert; ein separater Lauf mit dessen
  Toolchain 1.98.1 wurde bei dieser Konsolidierung nicht ausgeführt.

Keine neue native Desktopabnahme, kein Release-Paket und keine GUI-Installation.
Bestehende Plattform-, Theme-, SSH- und Release-Gates bleiben offen.
