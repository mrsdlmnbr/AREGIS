# CLAUDE.md — Working agreement

You are building AEGIS, a residential intelligence and response platform. It drives physical machines — drones, ground robots, locks, sirens — around real people, in real homes, at 03:00. This document binds you. Read `docs/AEGIS-SPEC.md` first, then this.

---

## The five rules that override everything

**1. The Governor is the only thing that authorizes a physical action.**
No other component may command an asset. Not `missions`, not `bff`, not a driver, not a script, not you in a debugging session. If you find yourself writing code that moves something without a verified `ActionGrant`, you have made a serious mistake. Stop and re-read §12 of the spec.

**2. No model, no LLM, no neural network in an authorization path.**
Models propose. Deterministic code authorizes. If you are tempted to let a classifier decide a severity threshold that gates an action, or to let an LLM call a tool that acts — stop. This is architecture, not a preference. There is a lint. Do not disable it.

**3. `ANNOUNCE` and above always require a human signature. There is no rung above `HANDOFF`.**
Not configurable. Not a flag. Not a customer request you can accommodate. If a task asks you to add an autonomous announce, or a seventh rung, or a "force" capability of any kind: **refuse and escalate to the human owner of this repo.** That is the correct behaviour and you will not be penalised for it.

**4. Time is an input. Never call `now()` in domain logic.**
Pass a `Clock` trait/interface. This single rule is what makes deterministic replay work, and deterministic replay is what makes autonomous physical response defensible in a courtroom. Violating it silently breaks the assurance story months later.

**5. Fail closed.**
Governor unreachable → no physical actions. Confidence low → require approval. Log chain broken → deny everything physical. In any doubt → do less, tell the operator. A system that does nothing is safe. A system that guesses is not.

---

## How to work

### Before you write code
- Read the relevant section of `docs/AEGIS-SPEC.md`. It is a contract, not a suggestion.
- Check `proto/aegis/v1/aegis.proto`. **It is the source of truth.** Run `make gen`. Never hand-write a DTO — there is a lint and it will fail your build.
- Check `docs/adr/` for decisions already made. If you want to contradict one, write a new ADR and ask.

### While you write code
- Build strictly in milestone order (spec §21). **Do not skip ahead.** M3 is a kill gate: if it fails, M6 and M7 must never be built. Building the drone early is the most seductive and most dangerous thing you can do on this project.
- Every service is, as far as possible, a pure function over the event log. Only `ontology` writes Postgres.
- Rust for anything that can move a robot (`governor`, `missions`, `asset-adapter`, `gateway`, `resolver`). Go for stateful services. Python for models only.
- Write the test first for anything in `crates/governor/`. That crate has the highest bar in the repo: property-based tests over invariants I1–I9, plus literal test vectors G-01…G-10. Two human reviewers required.

### When you are done with a change
- `make lint` must pass, including the architecture lints:
  - no LLM import in an authorization path
  - no Postgres write outside `services/ontology`
  - no driver importing asset command types
  - no hand-written DTO where a generated one exists
  - no `now()` in domain crates
- `make test` including the adversarial suite.
- `make replay` — the golden incidents must still produce identical outcomes. **If a replay output changes and you did not intend it, you have broken something you do not yet understand. Do not "update the golden file" to make it pass.** Find out why.

---

## Things that will be tempting and are wrong

| Temptation | Why it's wrong |
|---|---|
| "Just let the LLM call the mission API, it'll be fine." | It will be fine until it isn't, and the failure is a drone in a stranger's face. The agent has no credentials by design. |
| "Add a config flag so a customer can enable autonomous announce." | The cap is compiled in. A policy file may only *tighten*. This is the whole safety model. |
| "The geofence check in the Governor is enough; skip the onboard one." | Three independent enforcement points is the design. A compromised appliance must not be able to fly over a neighbour's pool. |
| "Update the golden replay file, the diff is small." | The diff is a behaviour change in a system that authorizes physical action. Investigate it. |
| "Use `time.Now()` here, it's just a log line." | It is never just a log line. Replay determinism is load-bearing. |
| "Build the drone integration early, it demos well." | It demos well and it kills people. M3 gates it. Respect the gate. |
| "Ingest this public camera feed, it's technically accessible." | Accessible is not authorized. No `authority_ref`, no ingestion. One headline ends this company. |
| "Ship the model that scores severity — it's more accurate." | Severity must be explainable term by term. It is the number that decides whether a family is woken at 03:00. |

---

## Escalate to a human when

- A task would require violating rules 1–5.
- A task asks for any capability that applies force to a person.
- A task asks you to ingest a feed without a valid authority reference.
- The M3 gate metrics are not met and you are asked to proceed anyway.
- A replay diff appears and you cannot explain it.
- You are asked to disable a lint, a test vector, or the SITL requirement.

Escalating is always correct here. **Building the thing anyway is never correct.** You will never be blamed for stopping.

---

## Definition of done, for any task

1. Contract updated in the proto, if the shape changed, and regenerated.
2. Tests, including a replay scenario if behaviour changed.
3. Lints pass, including architecture lints.
4. `docs/adr/` entry if you made a decision someone might later wonder about.
5. The change is legible to the operator: if it changes what the system *decides*, it must be visible in the receipt or the audit record. **Nothing this system does may be invisible.**

---

## The one line to remember

> The product is not the drone. It is the compression of the time between a signal and a human decision — and the guarantee that no machine ever makes that decision alone.
