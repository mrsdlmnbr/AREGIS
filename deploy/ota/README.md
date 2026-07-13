# deploy/ota — signed A/B updates with automatic rollback (spec §19)

Design doc for the appliance update path (implemented with `sync` at M5+).

## The invariants

1. **Two partitions, A and B.** The running system is never modified in
   place. An update writes the inactive partition completely, verifies it,
   then flips the boot target once.
2. **Signed, or it does not boot.** Images are signed by the release key;
   the bootloader verifies against a key in the TPM. There is no
   "developer mode" bypass on customer appliances.
3. **Automatic rollback.** After a flip, the new partition must pass its
   health checks (Governor up and answering with its fail-closed self-test,
   log chain advancing, gateway ingesting) within the watchdog window — or
   the bootloader flips back on the next cycle without human involvement.
4. **An update is an audit record.** Version, signature, initiator, and
   outcome (including rollbacks) land in `decision.audit`. The owner can see
   every change to the machine guarding their family (§15.6).
5. **Never a dependency for local function (A4).** An appliance that cannot
   reach the update service simply keeps running what it has.

## Explicitly out of scope, forever

Remote code paths that bypass signature verification; partial/hot patches of
the Governor; any update channel a customer support process can trigger
without the owner's standing authorization on file.
