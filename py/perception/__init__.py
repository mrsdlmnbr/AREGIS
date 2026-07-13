"""AEGIS perception tier — turns pixels into Sightings (spec §9.2).

M0 scope: interfaces, a deterministic fake detector for the sim harness, and
the sighting builder. Real model weights arrive in M2; nothing here loads a
model, opens a socket, or reads a clock. Models only, never authority.
"""

from .clock import Clock, FixedClock, ManualClock
from .detector import (
    OBJECT_CLASSES,
    BBox,
    Detection,
    Detector,
    FakeDetector,
)
from .homography import CameraCalibration, Homography
from .sighting import (
    REASON_DEGENERATE,
    REASON_UNCALIBRATED,
    Sighting,
    WorldPosition,
    build_sighting,
)

__all__ = [
    "Clock",
    "FixedClock",
    "ManualClock",
    "OBJECT_CLASSES",
    "BBox",
    "Detection",
    "Detector",
    "FakeDetector",
    "CameraCalibration",
    "Homography",
    "REASON_DEGENERATE",
    "REASON_UNCALIBRATED",
    "Sighting",
    "WorldPosition",
    "build_sighting",
]
