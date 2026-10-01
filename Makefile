VERSION ?= dev

.PHONY: build test vet install update release-build rust-build rust-test rust-check rust-install rust-run

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

rust-build:
	cd rust && cargo build --locked --release --workspace

rust-test:
	cd rust && cargo test --locked --workspace

rust-check:
	cd rust && cargo fmt --all -- --check
	cd rust && cargo clippy --locked --workspace --all-targets -- -D warnings

rust-install:
	cd rust && cargo install --locked --path crates/drift-gui

rust-run:
	cd rust && cargo run --locked -p drift-gui
