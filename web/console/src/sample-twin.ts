// Static sample twin for the M0 skeleton: the ridgeline estate mid-way
// through scenario S-001 (03:11, family away, one unknown on the grounds).
// Geometry matches sim/fixtures/ridgeline.yaml; receipt terms match
// docs/contracts/sim-cli.md §1 exactly.

import type { TwinView } from "./types";

export const SAMPLE_TWIN: TwinView = {
  propertyName: "Ridgeline Estate",
  posture: "AWAY",
  governorState: "NOMINAL",
  boundary: [
    [0, 0],
    [800, 0],
    [800, 400],
    [0, 400],
  ],
  geofence: [
    [15, 15],
    [785, 15],
    [785, 385],
    [15, 385],
  ],
  zones: [
    { id: "threshold_d03", name: "Rear door D-03", cls: "THRESHOLD", area: [[436, 244], [452, 244], [452, 252], [436, 252]] },
    { id: "rear_terrace", name: "Rear terrace", cls: "GROUNDS", area: [[400, 230], [520, 230], [520, 280], [400, 280]] },
    { id: "grounds_north", name: "North grounds", cls: "GROUNDS", area: [[200, 80], [520, 80], [520, 230], [200, 230]] },
    { id: "gate_drive", name: "Gate & driveway", cls: "THRESHOLD", area: [[600, 300], [680, 300], [680, 380], [600, 380]] },
    { id: "perimeter_east", name: "East perimeter", cls: "PERIMETER", area: [[700, 15], [785, 15], [785, 385], [700, 385]] },
    { id: "interior_main", name: "Main house", cls: "INTERIOR", area: [[100, 250], [400, 250], [400, 380], [100, 380]] },
  ],
  devices: [
    { id: "S-12", kind: "motion", position: [300, 110], calibrated: true },
    { id: "CAM-04", kind: "camera", position: [260, 60], calibrated: true },
    { id: "CAM-11", kind: "camera", position: [470, 300], calibrated: true },
    { id: "D-03", kind: "contact", position: [444, 248], calibrated: true },
    { id: "CAM-GATE", kind: "camera", position: [640, 340], calibrated: false },
  ],
  assets: [
    { id: "BEE-01", kind: "AIR", state: "ON_STATION", position: [438, 232], battery: 0.91 },
    { id: "ARGUS-1", kind: "GROUND", state: "DOCKED", position: [120, 100], battery: 1.0 },
  ],
  entities: [
    {
      id: "ent-000001",
      identity: "unknown:7F3A",
      known: false,
      cls: "PERSON",
      position: [440, 240],
      trail: [
        [330, 125],
        [352, 152],
        [398, 205],
        [440, 240],
      ],
      confidence: 0.9936,
      expected: false,
      zoneId: "rear_terrace",
    },
  ],
  alerts: [
    {
      id: "alert-1",
      severity: 4,
      score: 6.6,
      entityId: "ent-000001",
      zoneId: "grounds_north",
      polNote: "last unexpected perimeter entity: 41 days ago",
      at: "03:11:45",
      receipt: [
        { name: "base", input: "PERSON/unknown @ GROUNDS", weight: 1.0, contribution: 2.4 },
        { name: "posture_amplifier", input: "AWAY @ 03h (night)", weight: 2.0, contribution: 2.4 },
        { name: "expectation_discount", input: "no expectation matched", weight: 0.0, contribution: 0.0 },
        { name: "pol_anomaly", input: "last unexpected perimeter entity: 41 days ago", weight: 1.5, contribution: 1.2 },
        { name: "corroboration", input: "3 sensors, 0 mesh", weight: 1.0, contribution: 0.6 },
        { name: "dwell", input: "0.0 s", weight: 0.5, contribution: 0.0 },
        { name: "known_benign", input: "none", weight: 1.0, contribution: 0.0 },
      ],
    },
  ],
  signalToDecisionP50: 14.0,
};
