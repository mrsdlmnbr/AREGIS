# AEGIS build entrypoints — spec §5. Keep targets boring and deterministic.

SHELL := /bin/bash
GOBIN ?= $(CURDIR)/bin

.PHONY: all gen build test lint replay sim clean rebuild-from-log go-build go-test rust-build rust-test py-test

all: build

# ── code generation ─────────────────────────────────────────────────────────
# proto/aegis/v1/aegis.proto is the SOURCE OF TRUTH (spec §8).
# Rust types are generated at build time by crates/aegis-proto/build.rs (prost).
# Go + Python generated code is committed; this target refreshes it.
gen:
	protoc -I proto \
	  --go_out=gen/go --go_opt=paths=source_relative \
	  --go_opt=Mproto/aegis/v1/aegis.proto=github.com/mrsdlmnbr/aregis/gen/go/aegis/v1\;aegisv1 \
	  proto/aegis/v1/aegis.proto
	@mkdir -p py/aegis_proto
	@if python3 -c 'import grpc_tools' 2>/dev/null; then \
	  python3 -m grpc_tools.protoc -I proto --python_out=py/aegis_proto proto/aegis/v1/aegis.proto; \
	else echo "grpcio-tools not installed — skipping Python gen (pip install grpcio-tools)"; fi
	cargo build -p aegis-proto

# ── build ────────────────────────────────────────────────────────────────────
build: rust-build go-build

rust-build:
	cargo build --workspace

go-build:
	mkdir -p $(GOBIN)
	go build ./...
	go build -o $(GOBIN)/threat-cli ./services/threat/cmd/threat-cli
	go build -o $(GOBIN)/evidence-cli ./services/evidence/cmd/evidence-cli
	go build -o $(GOBIN)/evidence-verify ./tools/evidence-verify
	go build -o $(GOBIN)/playbook-lint ./tools/playbook-lint

# ── test ─────────────────────────────────────────────────────────────────────
test: rust-test go-test py-test

rust-test:
	cargo test --workspace

go-test:
	go test ./...

py-test:
	cd py && python3 -m unittest discover -v -s . -p 'test_*.py'

# ── lint ─────────────────────────────────────────────────────────────────────
# Includes the architecture lints (CLAUDE.md). Failing these is failing the build.
lint: go-build
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	go vet ./...
	./scripts/archlint.sh
	$(GOBIN)/playbook-lint playbooks/*.yaml

# ── simulation & replay ──────────────────────────────────────────────────────
# `make sim` runs every scenario; `make replay SCENARIO=S-001-night-perimeter`
# replays one and asserts its `expect:` block. Deterministic, byte-identical (A7).
sim: build
	cargo run -p aegis-sim-harness --bin aegis-sim -- run-all \
	  --scenarios sim/scenarios --fixtures sim/fixtures --playbooks playbooks --bin $(GOBIN)

replay: build
	cargo run -p aegis-sim-harness --bin aegis-sim -- replay \
	  --scenario sim/scenarios/$(SCENARIO).yaml --fixtures sim/fixtures --playbooks playbooks --bin $(GOBIN)

# Drop the materialised views and rebuild from the log; must be byte-identical (A1).
rebuild-from-log: go-build
	go run ./services/ontology/cmd/rebuild-check

clean:
	cargo clean
	rm -rf $(GOBIN) sim/out
