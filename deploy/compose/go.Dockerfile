# Go services + safety tools. Multi-stage: build once, ship static binaries.
FROM golang:1.24 AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY gen ./gen
COPY services ./services
COPY tools ./tools
COPY drivers ./drivers
RUN CGO_ENABLED=0 go build -o /out/threat-cli ./services/threat/cmd/threat-cli \
 && CGO_ENABLED=0 go build -o /out/evidence-cli ./services/evidence/cmd/evidence-cli \
 && CGO_ENABLED=0 go build -o /out/evidence-verify ./tools/evidence-verify \
 && CGO_ENABLED=0 go build -o /out/playbook-lint ./tools/playbook-lint \
 && CGO_ENABLED=0 go build -o /out/rebuild-check ./services/ontology/cmd/rebuild-check \
 && CGO_ENABLED=0 go build -o /out/sim-camera ./drivers/sim-camera

FROM gcr.io/distroless/static-debian12
COPY --from=build /out/ /usr/local/bin/
# No default entrypoint: one image, many tools (compose picks the command).
