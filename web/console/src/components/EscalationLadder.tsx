// The escalation ladder with visible Governor state. OBSERVE/ILLUMINATE
// execute on click; ANNOUNCE AND ABOVE REQUIRE PRESS-AND-HOLD, which is
// what produces the operator signature — the UI makes the doctrine
// physical (spec §14.1, invariant I1).
//
// There are six rungs. There is no seventh slot in this component and there
// never will be: no force, not a setting, not a licence tier, not a
// customer request (spec §3.1, CLAUDE.md rule 3).

import { useRef, useState } from "react";
import { requiresHold, Rung, RUNGS } from "../types";

const HOLD_MS = 1200;

export function EscalationLadder({
  focusedRung,
  onExecute,
  onSignature,
}: {
  focusedRung: Rung | null;
  onExecute: (rung: Rung, signed: boolean) => void;
  onSignature: (rung: Rung) => void;
}) {
  return (
    <div className="ladder">
      <h2>Escalation</h2>
      {RUNGS.map((rung, i) =>
        requiresHold(rung) ? (
          <HoldButton
            key={rung}
            rung={rung}
            index={i + 1}
            focused={focusedRung === rung}
            onComplete={() => {
              onSignature(rung);
              onExecute(rung, true);
            }}
          />
        ) : (
          <button
            key={rung}
            className={`rung click ${focusedRung === rung ? "focused" : ""}`}
            onClick={() => onExecute(rung, false)}
          >
            <span className="rung-num">{i + 1}</span> {rung}
            <span className="rung-note">click · autonomous up to on-loop</span>
          </button>
        ),
      )}
      <div className="ladder-footnote">rungs end at HANDOFF — there is no 7</div>
    </div>
  );
}

function HoldButton({
  rung,
  index,
  focused,
  onComplete,
}: {
  rung: Rung;
  index: number;
  focused: boolean;
  onComplete: () => void;
}) {
  const [progress, setProgress] = useState(0);
  const timer = useRef<number | null>(null);
  const startedAt = useRef(0);

  const start = () => {
    startedAt.current = performance.now();
    const tick = () => {
      const p = Math.min(1, (performance.now() - startedAt.current) / HOLD_MS);
      setProgress(p);
      if (p >= 1) {
        stop();
        onComplete();
      } else {
        timer.current = requestAnimationFrame(tick);
      }
    };
    timer.current = requestAnimationFrame(tick);
  };

  const stop = () => {
    if (timer.current !== null) cancelAnimationFrame(timer.current);
    timer.current = null;
    setProgress(0);
  };

  return (
    <button
      className={`rung hold ${focused ? "focused" : ""}`}
      onPointerDown={start}
      onPointerUp={stop}
      onPointerLeave={stop}
    >
      <span className="rung-num">{index}</span> {rung}
      <span className="rung-note">press &amp; hold — produces your signature</span>
      <span className="hold-progress" style={{ width: `${progress * 100}%` }} />
    </button>
  );
}
