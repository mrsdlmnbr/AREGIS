// Posture, Governor state, and the north star: median seconds from first
// signal to a human decision with full context (spec §23). Instrumented
// from M1; optimise nothing else until it is the best number in the
// industry.

import type { TwinView } from "../types";

export function Header({ twin, live }: { twin: TwinView; live: boolean }) {
  return (
    <header className="header">
      <span className="brand">AEGIS</span>
      <span className="property">{twin.propertyName}</span>
      <span
        className={`source-pill ${live ? "live" : "sample"}`}
        title={live ? "connected to bffd" : "no bff reachable — committed sample data"}
      >
        {live ? "LIVE" : "SAMPLE"}
      </span>
      <span className={`posture-pill ${twin.posture.toLowerCase()}`}>{twin.posture}</span>
      <span className={`governor-pill ${twin.governorState === "NOMINAL" ? "ok" : "bad"}`}>
        GOVERNOR · {twin.governorState}
      </span>
      <span className="northstar" title="signal → human decision, p50 seconds">
        sig→decision p50:{" "}
        {twin.signalToDecisionP50 === null ? "—" : `${twin.signalToDecisionP50.toFixed(1)}s`}
      </span>
      <span className="keys">j/k triage · 1-6 rungs · c cameras · t timeline · Esc abort</span>
    </header>
  );
}
