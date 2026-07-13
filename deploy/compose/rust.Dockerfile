# Rust spine + sim harness. Needs protoc for crates/aegis-proto (spec §8)
# and the Go stage's CLIs so the harness can drive the real scorer/sealer.
FROM golang:1.24 AS gotools
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY gen ./gen
COPY services ./services
COPY tools ./tools
RUN CGO_ENABLED=0 go build -o /out/threat-cli ./services/threat/cmd/threat-cli \
 && CGO_ENABLED=0 go build -o /out/evidence-cli ./services/evidence/cmd/evidence-cli \
 && CGO_ENABLED=0 go build -o /out/evidence-verify ./tools/evidence-verify

FROM rust:1.94 AS build
RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /aegis
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY sim/harness ./sim/harness
COPY proto ./proto
RUN cargo build --release -p aegis-sim-harness

FROM debian:bookworm-slim
WORKDIR /aegis
COPY --from=build /aegis/target/release/aegis-sim /usr/local/bin/aegis-sim
COPY --from=gotools /out/ /usr/local/bin/
COPY playbooks ./playbooks
COPY sim/scenarios ./sim/scenarios
COPY sim/fixtures ./sim/fixtures
ENTRYPOINT ["/usr/local/bin/aegis-sim"]
