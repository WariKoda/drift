BIN := $(HOME)/.local/bin
VERSION ?= dev

.PHONY: build test vet install update release-build

build:
	go build ./...

test:
	go test ./...

vet:
	go vet ./...

install:
	GOBIN=$(BIN) go install .

update: install

release-build:
	go build -ldflags "-X github.com/WariKoda/drift/cmd.Version=$(VERSION)" -o drift .
