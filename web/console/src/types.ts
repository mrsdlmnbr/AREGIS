// Console-side view types. These mirror the proto (the source of truth);
// M1 replaces the sample data with the bff's StreamTwin/StreamAlerts feeds
// and generated ts-proto types.

export type ZoneClass =
  | "PERIMETER"
  | "GROUNDS"
  | "THRESHOLD"
  | "INTERIOR"
  | "PRIVATE"
  | "SAFE_ROOM";

export type Posture = "NOMINAL" | "AWAY" | "NIGHT" | "ELEVATED" | "LOCKDOWN";

// The six rungs. THERE IS NO RUNG 7 — do not extend this union; the
// Governor has no branch for one and this UI renders no slot for one
// (spec §3.1, CLAUDE.md rule 3).
export type Rung = "OBSERVE" | "ILLUMINATE" | "ANNOUNCE" | "SHADOW" | "DENY" | "HANDOFF";

export const RUNGS: Rung[] = ["OBSERVE", "ILLUMINATE", "ANNOUNCE", "SHADOW", "DENY", "HANDOFF"];

/** ANNOUNCE and above require press-and-hold → operator signature (I1). */
export function requiresHold(rung: Rung): boolean {
  return rung !== "OBSERVE" && rung !== "ILLUMINATE";
}

export interface Zone {
  id: string;
  name: string;
  cls: ZoneClass;
  area: [number, number][];
}

export interface Device {
  id: string;
  kind: string;
  position: [number, number];
  calibrated: boolean;
}

export interface AssetView {
  id: string;
  kind: "AIR" | "GROUND";
  state: string;
  position: [number, number];
  battery: number;
}

export interface EntityView {
  id: string;
  identity: string;
  known: boolean;
  cls: string;
  position: [number, number];
  trail: [number, number][];
  confidence: number;
  expected: boolean;
  zoneId: string;
}

export interface ReceiptTerm {
  name: string;
  input: string;
  weight: number;
  contribution: number;
}

export interface AlertView {
  id: string;
  severity: number;
  score: number;
  entityId: string;
  zoneId: string;
  receipt: ReceiptTerm[];
  polNote: string;
  at: string;
}

export interface TwinView {
  propertyName: string;
  posture: Posture;
  governorState: "NOMINAL" | "DEGRADED" | "FAIL-CLOSED";
  boundary: [number, number][];
  geofence: [number, number][];
  zones: Zone[];
  devices: Device[];
  assets: AssetView[];
  entities: EntityView[];
  alerts: AlertView[];
  signalToDecisionP50: number | null;
}

export interface TapeEvent {
  at: string;
  text: string;
  kind: "log" | "action" | "governor" | "signature";
}
