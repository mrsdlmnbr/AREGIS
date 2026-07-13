# dal-conformance — the acceptance test for hardware that does not exist yet

This tool is deliberately in the tree from day one (spec §5). It is the
executable form of spec §10.3 — the five things you will wish for and never
get from third-party hardware — and it is the requirements document for the
devices AEGIS eventually builds (M9). We live inside this contract for two
years with third-party gear that fails it; our hardware ships when it passes.

```sh
go run ./tools/dal-conformance <driver-manifest.yaml>...
```

A manifest declares what the hardware actually does; the checker catches the
manifest whose declared properties cannot honestly back its claimed
capabilities:

- `SIGNED_CAPTURE` ⇒ a secure element signs **at capture** (§10.3.1) — chain
  of custody begins at the sensor, not the server.
- `PTP_CLOCK` ⇒ `clock_sync: PTP` (§10.3.2) — the most underrated hardware
  requirement in the system; sloppy timestamps silently destroy fusion.
- `LOCAL_BUFFER` ⇒ records through a cut wire and reconciles (§10.3.4).
- `MOBILE_ASSET` ⇒ a **mission** API (goto/orbit/observe/follow/return),
  never a joystick, **and** onboard geofence enforcement (§10.3.5, §3.2) —
  the asset must be incapable of the forbidden thing, independent of the
  Governor and the mission executor.
- The capability set is **closed** (§10.1). Extending it is a spec change
  with an ADR, not a manifest convenience.

Every violation is reported, not just the first: a vendor gets the whole gap
list in one run.
