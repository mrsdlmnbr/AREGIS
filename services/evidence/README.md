# services/evidence — chain of custody, sensor to courtroom

Spec §9.9. This service exists for one moment: months after an incident, in a
room where nobody trusts us, someone asks *"how do we know this footage is
what the system actually saw, when it says it saw it?"* Everything here is
built so the answer does not depend on our word.

## The chain of custody story

1. **Content addressing.** Every media artifact is stored under the SHA-256
   of its bytes — the hash *is* the filename (`media/<sha256-hex>`). You
   cannot swap bytes without changing the name, and you cannot change the
   name without breaking the manifest.
2. **The seal.** When an alert triggers a `seal_on_trigger` playbook step, an
   `EvidencePackage` is built: a Merkle tree whose leaves are, in order, the
   SHA-256 of each media artifact, then the SHA-256 of each contributing
   event id, then the SHA-256 of each audit record id. Parents are
   `sha256(left || right)`; an unpaired last node is promoted unchanged. An
   empty tree is invalid — a seal must attest to something.
3. **The signature.** The 32 raw bytes of the Merkle root are signed with
   Ed25519. In production the appliance HSM signs and the private key never
   leaves the secure element (spec §19). In the deterministic sim, the
   signing seed arrives *in the seal request* (see
   `docs/contracts/sim-cli.md` §2) — and the sealer **refuses** a request
   without one. There is no code path that invents a key: a seal under a key
   nobody controls would look like custody without being custody.
4. **The bundle.** `Export` produces a portable directory:

   ```
   bundle/
     manifest.json        # ids, timestamps, media hashes, merkle_root,
                          # signature, signer_pubkey — all hex
     media/<sha256-hex>   # one file per artifact
   ```

   Tampering with any byte of any artifact, any id, the root, or the
   signature is detectable from the bundle alone.

Timestamps (`sealed_at`) are inputs carried in the seal request — axiom A7.
Nothing in this service reads a clock, which is what makes sealing
deterministic and replayable: the same request produces byte-identical
`manifest.json`, forever.

## Sim CLI

```
go run ./services/evidence/cmd/evidence-cli seal --out <bundle-dir> \
    < seal-request.json > package.json
```

stdout is the exact bytes of the bundle's `manifest.json`. The request and
bundle formats are contract, defined in `docs/contracts/sim-cli.md` §2.

## Third-party verification — zero trust in us

`tools/evidence-verify` is the standalone verifier that police, an insurer,
or opposing counsel runs. It is deliberately **not** built from this
service's code: it imports only the Go standard library and re-implements
the hashing, the Merkle fold, and the signature check from the written
contract. If the sealer had a bug, the verifier would expose it rather than
share it. It reads one directory and never touches the network.

A third party needs from us: the bundle directory. Nothing else — no
account, no API, no server of ours has to be up or honest.

```
go build -o evidence-verify ./tools/evidence-verify   # or use a provided binary,
                                                      # or build from audited source
./evidence-verify <bundle-dir>
```

* Exit `0`, `{"valid":true,"failed_artifacts":[]}` — every artifact's
  SHA-256 recomputed from disk matches the manifest and its filename, the
  Merkle root rebuilt from the manifest's leaves matches the sealed root,
  and the Ed25519 signature over the root verifies against
  `signer_pubkey`.
* Exit `1`, `{"valid":false,"failed_artifacts":[...]}` — the list names
  **every** failing artifact or manifest field, not just the first. Flip a
  single byte of any artifact and the verdict names that artifact.

What verification proves: the bundle's media, event ids, and audit record
ids are bit-for-bit what the holder of the signing key sealed, at the
`sealed_at` instant the manifest claims. What it does not prove: *who* holds
the key. That last link is closed out of band — the appliance's public key
is enrolled at install time and can be published or escrowed, so a court can
check `signer_pubkey` against a record we cannot rewrite after the fact.
