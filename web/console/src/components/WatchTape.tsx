// The watch tape: the append-only log, visible at all times (spec §14.1).
// Every panel above it is a projection of it, and the operator can SEE
// that. It never truncates and never reorders.

import { useEffect, useRef } from "react";
import type { TapeEvent } from "../types";

export function WatchTape({ events }: { events: TapeEvent[] }) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth" });
  }, [events.length]);

  return (
    <div className="tape">
      <span className="tape-title">WATCH TAPE · append-only</span>
      <div className="tape-scroll">
        {events.map((e, i) => (
          <span key={i} className={`tape-event ${e.kind}`}>
            <span className="tape-at">{e.at}</span> {e.text}
          </span>
        ))}
        <div ref={end} />
      </div>
    </div>
  );
}
