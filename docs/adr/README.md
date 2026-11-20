# Architecture Decision Records

Dieses Verzeichnis hält Entwurfsentscheidungen fest, die aus dem Code allein nicht
hervorgehen. Jede Entscheidung steht in einer eigenen Datei `NNNN-kurzer-titel.md`,
fortlaufend nummeriert. Eine überholte Entscheidung bleibt stehen; der Nachfolger
verweist auf sie und ihr Status wechselt auf „Ersetzt durch NNNN“.

## Vorlage

```markdown
# NNNN: Titel als Aussage

Status: Angenommen
Datum: JJJJ-MM-TT

## Kontext

Welches Problem oder welche Randbedingung die Entscheidung auslöst.

## Entscheidung

Was gilt, in einem oder wenigen Sätzen.

## Begründung

Warum diese Lösung und nicht die naheliegende Alternative.

## Folgen

Was dadurch leichter, schwerer oder ausgeschlossen ist.

## Fundstellen

Die Dateien und Funktionen, die die Entscheidung umsetzen.
```

## Verzeichnis

| Nr. | Entscheidung |
|---|---|
| [0001](0001-projektspeicher-nach-slug.md) | Der Projektspeicher liegt im Konfigurationsverzeichnis und ist nach dem Registry-Slug benannt |
| [0002](0002-server-verknuepfung-ohne-verbindungsfelder.md) | Ein Projekt-Host verknüpft einen globalen Server und setzt kein eigenes Verbindungsfeld |
| [0003](0003-diff-vergleich-nutzt-bestehende-verbindung.md) | Der Diff-Vergleich verwendet die bestehende Verbindung weiter |
| [0004](0004-groesse-und-mtime-schnellpfad.md) | Gleiche Größe und gleiche Änderungszeit gelten als unverändert |
| [0005](0005-upload-bei-unklaren-zeitstempeln.md) | Bei unklaren Zeitstempeln schlägt drift den Upload vor |
| [0006](0006-diffview-close-weist-befehle-ab.md) | Close im Diff-Screen weist eingereihte Befehle ab, statt auf sie zu warten |
