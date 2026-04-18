# Contributing to drift

Thanks for your interest in contributing.

## Development setup

Requirements:

- Go 1.25+
- Linux or macOS

Clone the repository and run:

```bash
go test ./...
go vet ./...
go build ./...
```

For a local install:

```bash
make install
```

## Project guidelines

Please keep changes small and focused.

Important conventions in this codebase:

- Do not block Bubble Tea `Update()` with network or filesystem work
- Run async work in `tea.Cmd`
- Reuse existing styles from `internal/styles/styles.go` and `internal/tui/styles.go`
- Prefer typed messages for screen transitions and async results
- Do not add speculative abstractions
- Do not swallow errors silently

## Tests

There are currently only a few unit tests. If you change config handling, path mapping, or sync behavior, please add or update tests where practical.

Run before opening a PR:

```bash
go test ./...
go vet ./...
go build ./...
```

## Git workflow

Please keep `main` stable and use short-lived branches for changes.

Recommended branch names:

- `feature/...` for new functionality
- `fix/...` for bug fixes
- `docs/...` for documentation changes
- `chore/...` for maintenance tasks
- `refactor/...` for internal cleanups

Typical flow:

```bash
git switch main
git pull
git switch -c feature/my-change

# implement change

go test ./...
go vet ./...
go build ./...

git add .
git commit -m "Add my change"
git push -u origin feature/my-change
```

Then open a pull request into `main`.

## Pull requests

Please include:

- a short description of the problem
- the chosen solution
- how you validated the change

## Issues

Bug reports should ideally include:

- your OS
- Go version or drift version
- protocol used (`sftp`, `ftp`, or `ftps`)
- relevant config snippets with secrets removed
- reproduction steps
