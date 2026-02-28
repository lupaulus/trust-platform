set shell := ["bash", "-lc"]

fmt:
	cargo fmt

clippy:
	cargo clippy --all-targets --all-features

test:
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cargo nextest run -p trust-runtime --lib; \
	else \
		echo "cargo-nextest missing; falling back to cargo test -p trust-runtime --lib"; \
		cargo test -p trust-runtime --lib; \
	fi

test-integration:
	cargo test -p trust-runtime --tests

test-e2e:
	cargo test -p trust-runtime --test complete_program

test-all:
	cargo test -p trust-runtime --test complete_program
	cargo test --all

test-fast:
	cargo test -p trust-runtime --lib

test-runtime:
	cargo test -p trust-runtime

test-ui:
	cargo test -p trust-runtime --test web_io_config_integration

test-nextest:
	@if ! command -v cargo-nextest >/dev/null 2>&1; then \
		echo "cargo-nextest is not installed. Install with: cargo install cargo-nextest"; \
		exit 1; \
	fi
	cargo nextest run -p trust-runtime --lib

check:
	cargo check --all

editor-smoke:
	./scripts/check_editor_integration_smoke.sh

lint: fmt clippy

readme-media:
	./scripts/prepare-readme-media.sh --dir editors/vscode/assets

plant-demo-media:
	./scripts/capture-plant-demo-media.sh

plant-demo-media-pro:
	./scripts/capture-plant-demo-media-pro.sh

filling-line-media-pro:
	./scripts/capture-filling-line-media-pro.sh

filling-line-debug-scene:
	./scripts/capture-filling-line-debug-scene.sh
