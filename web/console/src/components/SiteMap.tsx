// The live site map IS the home screen — not a camera grid (spec §14.1).
// Entities move on it; cameras are one keystroke ('c') from any entity.

import type { TwinView } from "../types";

const ZONE_TINT: Record<string, string> = {
  PERIMETER: "rgba(180, 120, 40, 0.12)",
  GROUNDS: "rgba(70, 140, 90, 0.10)",
  THRESHOLD: "rgba(200, 160, 60, 0.18)",
  INTERIOR: "rgba(90, 110, 180, 0.12)",
  PRIVATE: "rgba(160, 60, 60, 0.10)",
  SAFE_ROOM: "rgba(60, 160, 160, 0.14)",
};

function pts(poly: [number, number][]): string {
  return poly.map(([x, y]) => `${x},${y}`).join(" ");
}

export function SiteMap({
  twin,
  selectedEntity,
  onSelectEntity,
}: {
  twin: TwinView;
  selectedEntity: string | null;
  onSelectEntity: (id: string) => void;
}) {
  return (
    <svg viewBox="-20 -20 840 440" className="sitemap" aria-label="live site map">
      {/* legal property line */}
      <polygon points={pts(twin.boundary)} className="boundary" />
      {/* geofence: where assets may operate — dashed, inset */}
      <polygon points={pts(twin.geofence)} className="geofence" />
      {twin.zones.map((z) => (
        <polygon key={z.id} points={pts(z.area)} fill={ZONE_TINT[z.cls]} stroke="rgba(255,255,255,0.08)">
          <title>
            {z.name} ({z.cls})
          </title>
        </polygon>
      ))}
      {twin.devices.map((d) => (
        <g key={d.id} transform={`translate(${d.position[0]}, ${d.position[1]})`}>
          <rect x={-3} y={-3} width={6} height={6} className={d.calibrated ? "device" : "device uncalibrated"}>
            <title>
              {d.id} ({d.kind}){d.calibrated ? "" : " — UNCALIBRATED: no cross-sensor fusion"}
            </title>
          </rect>
        </g>
      ))}
      {twin.assets.map((a) => (
        <g key={a.id} transform={`translate(${a.position[0]}, ${a.position[1]})`}>
          <polygon points="0,-7 6,5 -6,5" className={`asset ${a.kind.toLowerCase()}`}>
            <title>
              {a.id} · {a.state} · battery {(a.battery * 100).toFixed(0)}%
            </title>
          </polygon>
        </g>
      ))}
      {twin.entities.map((e) => (
        <g key={e.id} onClick={() => onSelectEntity(e.id)} style={{ cursor: "pointer" }}>
          <polyline points={pts(e.trail)} className="trail" />
          <circle
            cx={e.position[0]}
            cy={e.position[1]}
            r={selectedEntity === e.id ? 8 : 6}
            className={e.known ? "entity known" : "entity unknown"}
          >
            <title>
              {e.identity} · conf {e.confidence.toFixed(2)} · {e.expected ? "expected" : "NOT expected"}
            </title>
          </circle>
          <text x={e.position[0] + 10} y={e.position[1] - 8} className="entity-label">
            {e.identity}
          </text>
        </g>
      ))}
    </svg>
  );
}
