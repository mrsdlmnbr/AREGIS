"""Ground-plane projection (spec §9.2).

Each camera is calibrated with a homography at install. Every sighting
carries a site-coordinate world position with a covariance — WITHOUT THIS,
CROSS-SENSOR FUSION IS IMPOSSIBLE. The install flow captures it; the console
must surface cameras that lack it.
"""

from dataclasses import dataclass, field
from typing import Optional, Tuple


@dataclass(frozen=True)
class Homography:
    """Row-major 3×3 image→ground homography."""

    m: Tuple[float, float, float, float, float, float, float, float, float]

    def project(self, u: float, v: float) -> Optional[Tuple[float, float]]:
        """Project an image point to the ground plane. None if the point is
        at the horizon (w ≈ 0) — a degenerate projection is refused, never
        guessed (fail closed)."""
        a, b, c, d, e, f, g, h, i = self.m
        w = g * u + h * v + i
        if abs(w) < 1e-9:
            return None
        return ((a * u + b * v + c) / w, (d * u + e * v + f) / w)


@dataclass(frozen=True)
class CameraCalibration:
    device_id: str
    calibrated: bool
    homography: Optional[Homography] = None
    # Position uncertainty (metres, 1σ) attached to every projection.
    sigma_m: float = field(default=1.5)
