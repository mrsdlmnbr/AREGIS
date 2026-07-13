// Live-twin probe: if a bffd is reachable, the console overlays live state
// (posture, open alerts, entities) and marks itself LIVE; otherwise it runs
// on the committed sample and says so. Honesty in the header: an operator
// must never mistake a demo for the estate (spec §9.4 — never serve
// silently-stale data; the same rule applies to serving sample data).

import type { Posture } from "./types";

export interface LiveStatus {
  posture: Posture | null;
  openAlerts: number;
  entities: number;
}

const BFF_URL = (import.meta as { env?: Record<string, string> }).env?.VITE_BFF_URL ?? "http://127.0.0.1:7400";

export async function fetchLive(): Promise<LiveStatus | null> {
  try {
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), 1500);
    const resp = await fetch(`${BFF_URL}/v1/twin`, {
      headers: { "X-Aegis-Persona": "OPERATOR" },
      signal: ctrl.signal,
    });
    clearTimeout(timer);
    if (!resp.ok) return null;
    const twin = (await resp.json()) as {
      posture?: string;
      openAlerts?: unknown[];
      entities?: unknown[];
    };
    return {
      posture: (twin.posture as Posture) ?? null,
      openAlerts: twin.openAlerts?.length ?? 0,
      entities: twin.entities?.length ?? 0,
    };
  } catch {
    return null; // no bff — sample mode, clearly labelled
  }
}
