# Install target. A GOBIN set in the environment or via `go env -w GOBIN=…` wins,
# so `make install` and `go install <module>@latest` cannot leave two binaries
# shadowing each other in $PATH. Without one, install where the docs say.
BIN ?= $(shell go env GOBIN)
ifeq ($(strip $(BIN)),)
BIN := $(HOME)/.local/bin
endif

VERSION ?= dev

.PHONY: build test vet install update release-build

build:
	go build ./...

test:
	go test ./...

vet:
	go vet ./...

install:
	GOBIN="$(BIN)" go install .

update: install

release-build:
	go build -ldflags "-X github.com/WariKoda/drift/cmd.Version=$(VERSION)" -o drift .
