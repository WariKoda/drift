BIN := $(HOME)/.local/bin

.PHONY: install update

install:
	GOBIN=$(BIN) go install .
	ln -sf $(BIN)/drift-tui $(BIN)/drift

update: install
