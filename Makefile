VERSION ?= dev

.PHONY: build test vet install update release-build snapshot

build:
	go build ./...

test:
	go test ./...

vet:
	go vet ./...

install:
	go install .

update: install

release-build:
	go build -ldflags "-X github.com/WariKoda/drift/cmd.Version=$(VERSION)" -o drift .

# All release archives in dist/, as the release workflow builds them; publishes nothing.
snapshot:
	goreleaser release --snapshot --clean
