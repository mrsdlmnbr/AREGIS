# py/ — the model tier. Models only, never authority.

Three packages, one doctrine (axiom A2): **stochastic proposes, deterministic
authorizes.** Nothing in this tree can act on the world, and the tests treat
that as an invariant, not an aspiration.

| Package | What it is | The binding constraint |
|---|---|---|
| `perception/` | Detections → Sightings with ground-plane positions | Uncalibrated camera ⇒ degraded flag, never a guessed position. Time is an injected `Clock` (A7). |
| `pol/` | Pattern-of-life baselines and rarity scores | **Shadow-only in M0.** There is no code path that raises severity; the threat scorer consumes the number as one explainable receipt term. |
| `agent/` | LLM copilot scaffold | Read-only tool registry frozen at import: `query_ontology`, `search_events`, `summarize_incident`, `propose_coa`. No credentials, no write path, no client of anything that acts. `propose_coa` returns an object a human must act on. |

Prompt injection (spec §9.11, §16.5): external text enters as `Untrusted` and
is data, never instructions. A successful injection yields a wrong sentence,
not a moved machine — `py/agent/test_architecture.py` red-teams exactly this,
and `scripts/archlint.sh` independently greps the package for mutating RPC
names.

Run the suite from the repo root: `make py-test` (stdlib `unittest` only — no
third-party dependencies in this tier at M0, deliberately: no model weights,
no network clients, nothing to leak).
