# 0007: Die Fensterpräferenz speichert die normale logische Inhaltsgröße und ein Maximiert-Flag

Status: Angenommen
Datum: 2026-11-20

## Kontext

Die Rust-GUI speichert Breite, Höhe und Maximierung des Fensters in `gui.toml`
(`drift_core::gui_preferences::WindowPreference`). Die Backends liefern dafür
unterschiedliche Rechtecke. Unter X11 meldet ein maximiertes Fenster die
Bildschirmgrenzen und nicht das Rechteck, auf das es beim Wiederherstellen
zurückspringt. Unter macOS öffnet das Fenster mit einem Inhaltsrechteck, während
`bounds()` die native Titelleiste einschließt.

`docs/rust-gui-theme-blockers.md` beschreibt das Abtasten des Fensterzustands und
die Unterscheidung von Maximierungsgeometrie. `docs/rust-port-plan.md` hält fest,
dass die Tests die normale Inhaltsgröße statt des Rahmens prüfen.

## Entscheidung

Die Präferenz speichert auf allen Backends die zeichenbare logische Inhaltsgröße
des letzten normalen, nicht maximierten Fensters. Maximierung steht nur im Flag
`maximized`. Ein maximiertes oder als maximiert gemeldetes Fenster ändert Breite und
Höhe nicht, ein Vollbildfenster ändert die Präferenz gar nicht.

## Begründung

Würde die GUI das gemeldete Rechteck speichern, stünden unter X11 nach einer
Maximierung die Bildschirmgrenzen als Normalgröße in `gui.toml`. Unter macOS
öffnet das Fenster mit der gespeicherten Größe als Inhaltsrechteck, eine aus
`bounds()` gespeicherte Größe enthielte also zusätzlich die Titelleiste. Die Regel
gilt auf jedem Backend gleich, auch wo das gemeldete Rechteck zufällig passen würde.

## Folgen

Die gespeicherte Größe ist zwischen Backends vergleichbar und überlebt
Maximieren und Wiederherstellen. Die GUI muss die Normalgröße messen, bevor sie die
Maximierung anfordert. Sie öffnet deshalb zuerst ein normales Fenster und maximiert
es danach. Solange eine Maximierung nicht bestätigt ist, gelten normale
Geometrieereignisse als mehrdeutig und ändern die Präferenz nicht.

## Fundstellen

- `rust/crates/drift-gui/src/preferences.rs`, Funktion `window_preference`
  (ersetzt die Kommentare im Funktionsanfang und im Zweig `WindowBounds::Windowed`)
- `rust/crates/drift-gui/src/preferences.rs`, Funktionen `initial_window_bounds`,
  `Preferences::with_source` und `WindowState::observe`
- `rust/crates/drift-core/src/gui_preferences.rs`, Struktur `WindowPreference`
