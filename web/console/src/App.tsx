// AEGIS operator console — M0 skeleton over a static sample twin.
// Dense, keyboard-first, map-primary (spec §14.1). M1 swaps SAMPLE_TWIN
// for the bff's StreamTwin/StreamAlerts and the mock signature for the
// operator's hardware token.

import { useCallback, useEffect, useMemo, useState } from "react";
import { AbortWindow } from "./components/AbortWindow";
import { EscalationLadder } from "./components/EscalationLadder";
import { Header } from "./components/Header";
import { SiteMap } from "./components/SiteMap";
import { TriageQueue } from "./components/TriageQueue";
import { WatchTape } from "./components/WatchTape";
import { SAMPLE_TWIN } from "./sample-twin";
import { requiresHold, Rung, RUNGS, TapeEvent } from "./types";

interface PendingAction {
  rung: Rung;
  windowSeconds: number;
}

export default function App() {
  const twin = SAMPLE_TWIN;
  const [selectedAlert, setSelectedAlert] = useState<string | null>(twin.alerts[0]?.id ?? null);
  const [selectedEntity, setSelectedEntity] = useState<string | null>(null);
  const [focusedRung, setFocusedRung] = useState<Rung | null>(null);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [cameraPanel, setCameraPanel] = useState(false);
  const [tape, setTape] = useState<TapeEvent[]>([
    { at: "03:11:43", kind: "log", text: "S-12 motion, north treeline" },
    { at: "03:11:44", kind: "log", text: "CAM-04 PERSON 0.79 → 0.84, fused" },
    { at: "03:11:45", kind: "log", text: "CAM-11 PERSON 0.81 — entity unknown:7F3A, conf 0.99" },
    { at: "03:11:45", kind: "governor", text: "alert-1 SEV 4 · playbook perimeter_breach_night v4" },
    { at: "03:11:45", kind: "governor", text: "GOVERNOR ALLOW OBSERVE (supervised) grant-000001 · audit aud-000002" },
  ]);

  const append = useCallback((kind: TapeEvent["kind"], text: string) => {
    setTape((t) => [...t, { at: "03:12:0" + (t.length % 10), kind, text }]);
  }, []);

  const executeRung = useCallback(
    (rung: Rung, signed: boolean) => {
      if (requiresHold(rung) && !signed) {
        append("governor", `${rung} refused: operator signature required (I1)`);
        return;
      }
      if (rung === "OBSERVE" || rung === "ILLUMINATE") {
        setPending({ rung, windowSeconds: 8 });
        append("action", `${rung} authorized · COUNTING_DOWN, 8 s abort window`);
      } else {
        append("action", `${rung} authorized with two-key grant · EXECUTING`);
      }
    },
    [append],
  );

  const signature = useCallback(
    (rung: Rung) => append("signature", `operator signature produced for ${rung} (hold-to-authorize)`),
    [append],
  );

  const alertIds = useMemo(() => twin.alerts.map((a) => a.id), [twin.alerts]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && pending) {
        append("action", `${pending.rung} ABORTED by operator (Esc)`);
        setPending(null);
      } else if (e.key === "j" || e.key === "k") {
        if (alertIds.length > 0) {
          const i = selectedAlert ? alertIds.indexOf(selectedAlert) : -1;
          const next = e.key === "j" ? Math.min(i + 1, alertIds.length - 1) : Math.max(i - 1, 0);
          setSelectedAlert(alertIds[next]);
        }
      } else if (e.key >= "1" && e.key <= "6") {
        setFocusedRung(RUNGS[Number(e.key) - 1]);
      } else if (e.key === "c" && selectedEntity) {
        setCameraPanel((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pending, selectedAlert, alertIds, selectedEntity, append]);

  return (
    <div className="console">
      <Header twin={twin} />
      <main className="main">
        <section className="map-pane">
          <SiteMap twin={twin} selectedEntity={selectedEntity} onSelectEntity={setSelectedEntity} />
          {cameraPanel && selectedEntity && (
            <div className="camera-panel">
              camera pivot for {selectedEntity} — CAM-04 · CAM-11 (M1: WebRTC via bff)
            </div>
          )}
          {pending && (
            <AbortWindow
              seconds={pending.windowSeconds}
              label={`${pending.rung} · BEE-01 · supervised launch`}
              onAbort={() => {
                append("action", `${pending.rung} ABORTED by operator`);
                setPending(null);
              }}
              onElapsed={() => {
                append("action", `${pending.rung} window elapsed · EXECUTING`);
                setPending(null);
              }}
            />
          )}
        </section>
        <aside className="side-pane">
          <TriageQueue alerts={twin.alerts} selected={selectedAlert} onSelect={setSelectedAlert} />
          <EscalationLadder focusedRung={focusedRung} onExecute={executeRung} onSignature={signature} />
        </aside>
      </main>
      <WatchTape events={tape} />
    </div>
  );
}
