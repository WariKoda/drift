# 0006: Close im Diff-Screen weist eingereihte Befehle ab, statt auf sie zu warten

Status: Angenommen
Datum: 2026-11-20

## Kontext

Der Diff-Screen besitzt die Remote-Verbindung und das lokale `fs.Root` des
Projekts. Sync-Befehle und das Laden von Bildern laufen als `tea.Cmd` außerhalb
von `Update` und greifen auf beide Ressourcen zu. Bubble Tea führt eingereihte
Befehle beim Beenden des Programms möglicherweise nie mehr aus.

`AGENTS.md` legt bereits fest, dass die Close-Methoden von Browser und
Diff-Screen einen Befehl zurückgeben: `Update` löst nur den Zustand, der
Transport wird im zurückgegebenen `tea.Cmd` geschlossen.
`docs/diffview-orchestration-plan.md` hält fest, dass `commandLifetime`,
`trackCommand` und `Close` in `diffview` bleiben.

## Entscheidung

Jeder Befehl, der Verbindung oder Root benutzt, läuft über `trackCommand`.
Beim Start prüft er unter dem Mutex von `commandLifetime`, ob `Close` schon
gelaufen ist. Ist das der Fall, kehrt er ohne Arbeit mit `nil` zurück.
Andernfalls zählt er in einer `WaitGroup` als laufend.

`Close` setzt unter demselben Mutex die Markierung `closed`, löst Verbindung und
Root sofort vom Modell und bricht laufende Aktivität ab. Der zurückgegebene
Befehl schließt die Verbindung, wartet auf die laufenden Befehle und schließt
erst danach das Root.

## Begründung

Die naheliegende Alternative wäre, dass `Close` auf alle ausgegebenen Befehle
wartet. Ein Befehl, den Bubble Tea beim Beenden nicht mehr ausführt, meldet aber
nie sein Ende, und das Warten bliebe hängen. Deshalb weist `Close` eingereihte
Befehle ab und wartet nur auf Befehle, die bereits laufen. Ein Befehl, der erst
nach `Close` an die Reihe kommt, kann das freigegebene Root nicht mehr
berühren, weil Zulassung und Abschluss denselben Mutex verwenden.

Das Schließen des Netzwerks kann auf eine aktive Operation warten. Deshalb
läuft die Freigabe der Ressourcen im zurückgegebenen Befehl und nicht in
`Close` selbst.

## Folgen

`Close` blockiert `Update` nicht. Ein vor `Close` erzeugter, aber erst danach
ausgeführter Befehl liefert keine Nachricht, auch keine Fehlermeldung. Nur
Befehle, die über `trackCommand` laufen, sind vor einem bereits geschlossenen
Root geschützt; das sind heute `quickSyncCmd`, `bulkSyncCmd` und das Laden der
Bilder in `startImages`.

## Fundstellen

- `internal/tui/diffview/model.go`: `commandLifetime` (Typkommentar, aus dem
  diese Begründung stammt), `Model.trackCommand`, `Model.Close`
- `internal/tui/diffview/images.go`: `Model.startImages`
- `internal/tui/diffview/connection_test.go`:
  `TestCloseWaitsForRunningCommandBeforeClosingRoot`,
  `TestCloseDetachesImmediatelyAndRejectsQueuedCommand`
