#!/usr/bin/env bash
# ============================================================================
#  Architecture lints (CLAUDE.md "When you are done with a change").
#  These are the structural guarantees of the safety doctrine. Weakening or
#  bypassing one of these is never a fix — it is the bug. Do not disable.
# ============================================================================
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
err() { echo "ARCHLINT FAIL: $*" >&2; fail=1; }

# ── 1. No ambient time in domain logic (axiom A7) ───────────────────────────
# Domain crates must take time as an input via the Clock trait / explicit
# timestamps. The only permitted wall-clock reads live in aegis-common's
# clock module (SystemClock) and binary entrypoints (cli edges).
PATTERN='SystemTime::now\(|Instant::now\(|Utc::now\(|Local::now\('
HITS=$(grep -RnE "$PATTERN" crates/ sim/harness/src --include='*.rs' \
       | grep -v 'crates/aegis-common/src/clock.rs' \
       | grep -v 'ARCHLINT-ALLOW' || true)
if [[ -n "$HITS" ]]; then
  err "wall-clock call in domain code (A7 — time is an input):"$'\n'"$HITS"
fi

HITS=$(grep -RnE 'time\.Now\(' services/ tools/ --include='*.go' \
       | grep -v '_test.go' | grep -v 'ARCHLINT-ALLOW' || true)
if [[ -n "$HITS" ]]; then
  err "time.Now() in Go service code (A7 — time is an input):"$'\n'"$HITS"
fi

HITS=$(grep -RnE 'datetime\.now\(|time\.time\(' py/ --include='*.py' \
       | grep -v test_ | grep -v 'ARCHLINT-ALLOW' || true)
if [[ -n "$HITS" ]]; then
  err "wall-clock call in Python code (A7 — time is an input):"$'\n'"$HITS"
fi

# ── 2. Only services/ontology writes Postgres (axiom A1) ────────────────────
HITS=$(grep -RlE 'database/sql|jackc/pgx|lib/pq' services/ tools/ --include='*.go' \
       | grep -v '^services/ontology/' || true)
if [[ -n "$HITS" ]]; then
  err "Postgres driver import outside services/ontology (A1):"$'\n'"$HITS"
fi

# ── 3. No driver imports asset command types (spec §10.2) ───────────────────
HITS=$(grep -RnE 'ActionGrant|TaskRequest|asset_adapter|asset-adapter' drivers/ \
       --include='*.rs' --include='*.go' --include='*.py' 2>/dev/null \
       | grep -v 'ARCHLINT-ALLOW' || true)
if [[ -n "$HITS" ]]; then
  err "driver references asset command types (§10.2 — Command may not move an asset):"$'\n'"$HITS"
fi

# ── 4. The LLM agent has no path to any mutating client (spec §9.11, A2) ────
HITS=$(grep -RnE 'MissionService|GovernorService|AssetService|RequestAction|Authorize|UpsertPerson|SetPosture|GrantAccess' \
       py/agent --include='*.py' 2>/dev/null | grep -v test_ | grep -v 'ARCHLINT-ALLOW' || true)
if [[ -n "$HITS" ]]; then
  err "py/agent references a mutating client (A2 — the agent proposes, never executes):"$'\n'"$HITS"
fi

# ── 5. There is no rung 7 anywhere (spec §3.1, CLAUDE.md rule 3) ─────────────
# The six rungs, and only the six rungs, may appear as escalation values.
# Tests that PROVE a seventh rung is rejected are excluded by path or carry
# an explicit ARCHLINT-ALLOW on the line.
HITS=$(grep -RnEi 'RUNG_(FORCE|SEVEN|7)|EscalationRung::(Force|Seven)|"FORCE"' \
       crates/ services/ py/ tools/ sim/ playbooks/ 2>/dev/null \
       | grep -vE '(/tests/|_test\.go|_test\.rs|/testdata/|test_)' \
       | grep -v 'ARCHLINT-ALLOW' || true)
if [[ -n "$HITS" ]]; then
  err "a seventh rung / force capability appears in the tree (THERE IS NO RUNG 7):"$'\n'"$HITS"
fi

# ── 6. Sim-only key seed MATERIAL stays in sim fixtures and tests ────────────
# Matches literal 32-byte hex strings (a seed on disk), not the seed-handling
# APIs. Production keys live in the HSM and never appear as literals anywhere
# (spec §19).
HITS=$(grep -RnE "[\"'][0-9a-fA-F]{64}[\"']" crates/ services/ tools/ py/ deploy/ playbooks/ 2>/dev/null \
       | grep -vE '(/tests/|_test\.go|_test\.rs|/testdata/|test_)' \
       | grep -v 'ARCHLINT-ALLOW' || true)
if [[ -n "$HITS" ]]; then
  err "literal key seed outside sim fixtures/tests (spec §19 — keys live in the HSM):"$'\n'"$HITS"
fi

if [[ "$fail" -ne 0 ]]; then
  echo "architecture lints FAILED" >&2
  exit 1
fi
echo "architecture lints OK"
