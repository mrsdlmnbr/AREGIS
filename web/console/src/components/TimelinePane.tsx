// Incident replay (spec §21 M1: "unified timeline; incident replay works").
// Renders a timeline.json exported by `aegis-sim` — the same artifact the
// assertion engine checked, so what the operator reviews is what the build
// verified. Load any incident with the file picker; ↑/↓ scrub the cursor.

import { useCallback, useEffect, useState } from "react";
import sample from "../sample-timeline.json";

interface TimelineDoc {
  schema: string;
  scenario: string;
  property: string;
  start_time: string;
  metrics: {
    first_signal_t: number | null;
    signal_to_first_receipt_s: number | null;
    signal_to_visual_s: number | null;
    signal_to_human_decision_s: number | null;
  };
  items: { t: number; kind: string; source: string; summary: string }[];
}

const SAMPLE = sample as TimelineDoc;

export function TimelinePane({ onClose }: { onClose: () => void }) {
  const [doc, setDoc] = useState<TimelineDoc>(SAMPLE);
  const [cursor, setCursor] = useState(0);

  const load = useCallback((file: File) => {
    file.text().then((text) => {
      const parsed = JSON.parse(text) as TimelineDoc;
      if (parsed.schema !== "aegis.sim.timeline/v1") {
        throw new Error(`not a timeline export: ${parsed.schema}`);
      }
      setDoc(parsed);
      setCursor(0);
    });
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        setCursor((c) => Math.min(c + 1, doc.items.length - 1));
        e.preventDefault();
      } else if (e.key === "ArrowUp") {
        setCursor((c) => Math.max(c - 1, 0));
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [doc.items.length]);

  const m = doc.metrics;
  return (
    <div className="timeline-pane">
      <div className="timeline-head">
        <span className="timeline-title">
          INCIDENT REPLAY · {doc.scenario} · {doc.start_time}
        </span>
        <label className="timeline-load">
          load timeline.json
          <input
            type="file"
            accept=".json"
            style={{ display: "none" }}
            onChange={(e) => e.target.files?.[0] && load(e.target.files[0])}
          />
        </label>
        <button className="timeline-close" onClick={onClose}>
          close · t
        </button>
      </div>
      <div className="timeline-metrics">
        <Metric label="signal → receipt" v={m.signal_to_first_receipt_s} emphasis />
        <Metric label="signal → visual" v={m.signal_to_visual_s} />
        <Metric label="signal → human decision" v={m.signal_to_human_decision_s} />
      </div>
      <div className="timeline-items">
        {doc.items.map((it, i) => (
          <div
            key={i}
            className={`timeline-item ${it.kind} ${i === cursor ? "cursor" : ""}`}
            onClick={() => setCursor(i)}
          >
            <span className="timeline-t">{it.t.toFixed(1)}s</span>
            <span className={`timeline-kind ${it.kind}`}>{it.kind}</span>
            <span className="timeline-source">{it.source}</span>
            <span className="timeline-summary">{it.summary}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function Metric({ label, v, emphasis }: { label: string; v: number | null; emphasis?: boolean }) {
  return (
    <span className={`timeline-metric ${emphasis ? "emphasis" : ""}`}>
      {label}: <strong>{v === null ? "—" : `${v.toFixed(1)}s`}</strong>
    </span>
  );
}
