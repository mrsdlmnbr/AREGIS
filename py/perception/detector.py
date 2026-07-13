"""Detection interfaces and the deterministic fake used by tests and the sim.

Real detectors (M2) implement Detector and run at the edge where the device
supports it (CAP_EDGE_EMBEDDER), else on the appliance GPU. Nothing in this
module loads weights or touches hardware.
"""

from dataclasses import dataclass
from typing import Optional, Protocol, Sequence

# The closed object-class set (proto ObjectClass). Detection classes outside
# this set are a contract violation, not a new feature.
OBJECT_CLASSES = ("PERSON", "VEHICLE", "ANIMAL", "PACKAGE", "DRONE", "UNKNOWN_OBJ")


@dataclass(frozen=True)
class BBox:
    x: float
    y: float
    w: float
    h: float

    def center(self) -> tuple:
        return (self.x + self.w / 2.0, self.y + self.h / 2.0)


@dataclass(frozen=True)
class Detection:
    object_class: str
    confidence: float
    bbox: BBox
    embedding: Optional[bytes] = None

    def __post_init__(self) -> None:
        if self.object_class not in OBJECT_CLASSES:
            raise ValueError(f"unknown object class {self.object_class!r} — the set is closed")
        if not 0.0 <= self.confidence <= 1.0:
            raise ValueError("confidence must be in [0, 1]")


class Detector(Protocol):
    def detect(self, frame: bytes) -> Sequence[Detection]: ...


class FakeDetector:
    """Deterministic detector for the sim: returns a scripted sequence of
    detection lists, one per frame, forever replayable."""

    def __init__(self, script: Sequence[Sequence[Detection]]) -> None:
        self._script = list(script)
        self._i = 0

    def detect(self, frame: bytes) -> Sequence[Detection]:
        if self._i >= len(self._script):
            return []
        out = self._script[self._i]
        self._i += 1
        return out
