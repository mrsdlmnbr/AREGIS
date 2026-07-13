# apps/staff — zones, schedule, and the most valuable boring feature in the product

React Native, M5. Three surfaces, and the third is the point (spec §14.3):

1. **Their zones.** Staff see the zones their access grants cover — nothing
   else. RBAC is enforced in the bff; the app never even fetches beyond it.
2. **Their schedule.** Shifts and access windows.
3. **Pre-registration of expected visitors.** The single largest false-alarm
   reduction in the system (spec §6.2), shipped in M3 before any model work.
   The estate manager registers the contractor on Tuesday morning; at 09:05
   the resolver stamps the arrival `expected = true`; the threat scorer's
   `expectation_discount` term does the rest, visibly, in the receipt.

`PreRegisterVisitor.tsx` maps 1:1 to the `Expectation` proto message — the
form is the wire contract with a keyboard.
