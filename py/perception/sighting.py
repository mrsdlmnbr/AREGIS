"""Sighting builder: Detection + CameraCalibration + Clock → Sighting.

The rule that matters (spec §9.2 failure mode): an uncalibrated camera still
produces sightings — but WITHOUT a world position and WITH a degraded flag.
Never silently degrade, never guess a position.
"""

from dataclasses import dataclass
from typing import Optional

from .clock import Clock
from .detector import Detection
from .homography import CameraCalibration

REASON_UNCALIBRATED = "uncalibrated-camera"
REASON_DEGENERATE = "degenerate-projection"


@dataclass(frozen=True)
class WorldPosition:
    x: float
    y: float
    sigma_m: float


@dataclass(frozen=True)
class Sighting:
    device_id: str
    at: float
    object_class: str
    confidence: float
    embedding: Optional[bytes]
    world_position: Optional[WorldPosition]
    degraded: bool
    degraded_reason: Optional[str]


def build_sighting(
    detection: Detection,
    calibration: CameraCalibration,
    clock: Clock,
) -> Sighting:
    """Build the Sighting that goes on perception.sighting. Time comes from
    the injected clock (A7); position comes from the homography or is
    honestly absent."""
    at = clock.now()
    if not calibration.calibrated or calibration.homography is None:
        return Sighting(
            device_id=calibration.device_id,
            at=at,
            object_class=detection.object_class,
            confidence=detection.confidence,
            embedding=detection.embedding,
            world_position=None,
            degraded=True,
            degraded_reason=REASON_UNCALIBRATED,
        )
    u, v = detection.bbox.center()
    projected = calibration.homography.project(u, v)
    if projected is None:
        return Sighting(
            device_id=calibration.device_id,
            at=at,
            object_class=detection.object_class,
            confidence=detection.confidence,
            embedding=detection.embedding,
            world_position=None,
            degraded=True,
            degraded_reason=REASON_DEGENERATE,
        )
    x, y = projected
    return Sighting(
        device_id=calibration.device_id,
        at=at,
        object_class=detection.object_class,
        confidence=detection.confidence,
        embedding=detection.embedding,
        world_position=WorldPosition(x=x, y=y, sigma_m=calibration.sigma_m),
        degraded=False,
        degraded_reason=None,
    )
