VERSION ?= dev
GUI_VERSION ?= 0.1.0-dev.1
GUI_TARGET ?= $(shell cd rust && rustc -vV | awk '/^host:/ {print $$2}')
GUI_CARGO_TARGET_DIR ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),$(CURDIR)/rust/target)
GUI_DIST_DIR ?= $(CURDIR)/dist/gui

.PHONY: build test vet install update release-build rust-build rust-test rust-check rust-install rust-run rust-package rust-package-test

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
	cd rust && cargo run --locked -p drift-gui -- "$(CURDIR)"

rust-package:
	target_dir="$$(python3 -c 'import os, sys; print(os.path.abspath(sys.argv[1]))' "$(GUI_CARGO_TARGET_DIR)")" && \
	(cd rust && DRIFT_GUI_VERSION="$(GUI_VERSION)" CARGO_TARGET_DIR="$$target_dir" cargo build --locked --release -p drift-gui --target "$(GUI_TARGET)") && \
	python3 rust/packaging/package.py --binary "$$target_dir/$(GUI_TARGET)/release/drift-gui" --version "$(GUI_VERSION)" --target "$(GUI_TARGET)" --output "$(GUI_DIST_DIR)"

rust-package-test:
	python3 -m unittest discover -s rust/packaging -p 'test_*.py'
