// The only interruption the owner app is allowed to make (spec §14.2).
// M5 wires onApprove to the owner's device key: approval IS a signature —
// the same second key of the two-key model the console produces with
// hold-to-authorize. Nothing physical moves on a tap alone.

import React from "react";

export interface ApprovalRequestProps {
  rung: "ANNOUNCE" | "SHADOW" | "DENY" | "HANDOFF"; // only human-signature rungs reach the owner
  summary: string; // "Operator requests ANNOUNCE at the north fence"
  requestedBy: string; // operator id — the owner sees WHO is asking
  onApprove: () => void; // produces the owner signature (M5: device key)
  onDeny: () => void;
  onCallMe: () => void;
}

export function ApprovalRequest(props: ApprovalRequestProps) {
  return (
    <div accessibilityRole={"alert" as never}>
      {/* RN primitives at M5; TSX sketch for the design contract now. */}
      <h1>{props.summary}</h1>
      <p>
        {props.requestedBy} · rung {props.rung} · requires your key
      </p>
      <button onClick={props.onApprove}>Approve</button>
      <button onClick={props.onDeny}>Deny</button>
      <button onClick={props.onCallMe}>Call me</button>
    </div>
  );
}
