// The supervised-launch abort window: 8 seconds, prominent, one key
// (Escape). The sweet spot — response in seconds, human still in the loop
// (spec §11 HUMAN_SUPERVISED, §14.1).

import { useEffect, useState } from "react";

export function AbortWindow({
  seconds,
  label,
  onAbort,
  onElapsed,
}: {
  seconds: number;
  label: string;
  onAbort: () => void;
  onElapsed: () => void;
}) {
  const [remaining, setRemaining] = useState(seconds);

  useEffect(() => {
    const id = setInterval(() => {
      setRemaining((r) => {
        if (r <= 0.1) {
          clearInterval(id);
          onElapsed();
          return 0;
        }
        return r - 0.1;
      });
    }, 100);
    return () => clearInterval(id);
  }, [onElapsed]);

  return (
    <div className="abort-window" role="alertdialog" aria-live="assertive">
      <div className="abort-label">{label}</div>
      <div className="abort-count">{remaining.toFixed(1)}s</div>
      <div className="abort-bar">
        <div className="abort-fill" style={{ width: `${(remaining / seconds) * 100}%` }} />
      </div>
      <button className="abort-button" onClick={onAbort} autoFocus>
        ABORT — Esc
      </button>
    </div>
  );
}
