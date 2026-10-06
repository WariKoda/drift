# GUI-Themes: Palette und native Abnahme offen

Stand: 6. Oktober 2026.

## Entscheidung

`gui.toml`, Fenster-/Pane-Präferenzen und System/Dark/Light werden zuerst
umgesetzt. Die Oberfläche bezeichnet die aktuellen Farben ausdrücklich als
**Kit-Themes**. Monokai Pro Dark und Monokai Pro Light Sun bleiben das geplante
Ziel; ihre Paletten und Assets werden nicht ungeklärt mitgeliefert.

Diese Begrenzung wurde vom Nutzer ausdrücklich gewählt. Sie ist weder eine
Umbenennung der Kit-Themes in Monokai noch eine Freigabe der GUI-Veröffentlichung.

## Weitergabe klären

Die [offiziellen Community-Edition-Bedingungen](https://monokai.pro/contribute)
verlangen unter anderem den Namen **Monokai Pro (CE)**, ausschließlich den
Standardfilter, ein freies, nichtkommerzielles Open-Source-Paket und die
Verwendung des [offiziellen Templates](https://github.com/monokai-pro/community-edition).
Diese Erlaubnis deckt die geplante Kombination einschließlich **Light Sun**
nicht ab. Die [offizielle Lizenz](https://monokai.pro/license) ist zusätzlich
zu berücksichtigen.

Für die geplante Weitergabe brauchen wir eine passende ausdrückliche Erlaubnis
bzw. entsprechend freigegebene Paletten. Eine individuelle Nutzungslizenz wird
nicht als Weitergabe-Erlaubnis behandelt. Die Bedingungen werden vor einer
Integration erneut geprüft; dies ist eine Projektentscheidung, keine allgemeine
juristische Bewertung des Urheberrechtsschutzes einzelner Farbwerte.

## Wiederaufnahme

1. Weitergabe der beiden vorgesehenen Paletten und die zulässige Benennung klären.
2. Kit- und Base-Tokens konsistent zuordnen, ohne SDK-Fork oder vendorte Assets.
3. Fokus, Auswahl, Fehler, Diff-Addition/-Deletion und Textkontrast in beiden
   Paletten prüfen; Sitzungen, Entities, Auswahl, IME und Scrollzustand erhalten.
4. Native Darstellung auf Wayland/X11 und macOS Intel/Apple Silicon abnehmen.

## Bereits vorhandener Mechanismus, noch keine native Abnahme

Die GUI speichert nur den Modus `system`, `dark` oder `light`. System verwendet
`Window.appearance()` und dessen laufende Beobachtung; feste Modi ändern die
Kit-/Base-Tokens unabhängig vom OS. Die Plattform-API erhält die entsprechende
Darstellungspräferenz bzw. `None` für System. Native Titelbalken folgen nur dort,
wo das SDK/Backend die API unterstützt; serverseitige Linux-Dekorationen bleiben
vom Window Manager abhängig.

Fensterzustand wird alle 250 ms nativ gelesen, weil reine X11-State-Notifications
Bounds-Callbacks auslassen können. Geometrie muss zuvor 150 ms ruhig sein.
Startup-/Restore-Übergänge behalten die gewünschte Normalgröße bis zur Bestätigung;
bei ausbleibender oder anderer Restore-Geometrie bleibt ein Hinweis sichtbar statt
einer geratenen Größe. Bei Schließen innerhalb von 150 ms nach einem echten Resize
kann die letzte Größe ungespeichert bleiben: sie ist nicht sicher von einer
vorlaufenden Maximierungsgeometrie unterscheidbar.

Headless-Tests prüfen Modusauflösung, Token-Projektion, native Eingabe-Entities,
Sitzungen, Pane-/Fensterzustandsmodelle und ausdrückliche Sync-Bestätigung.
Das gepinnte SDK stellt die native OS-Darstellungsinjektion nicht als öffentliche
Test-API bereit. Diese Tests ersetzen deshalb keinen echten laufenden OS-Wechsel,
keine native Maximierung/Dekoration und keine Fokus-/IME-/Accessibility-Abnahme.
