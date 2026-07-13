"""Time is an input, never an ambient (axiom A7).

Nothing in the Python tier may read the wall clock. Components
receive a Clock; the sim provides a deterministic one. There is deliberately
no wall-clock implementation in this tier at all — the appliance runtime
injects one at the process edge, and models never need it.
"""

from dataclasses import dataclass
from typing import Protocol


class Clock(Protocol):
    def now(self) -> float:
        """Seconds since the epoch of the run — an opaque monotonic value."""
        ...


@dataclass(frozen=True)
class FixedClock:
    """A clock frozen at one instant. For tests and pure replays."""

    at: float

    def now(self) -> float:
        return self.at


class ManualClock:
    """A clock the harness advances by hand."""

    def __init__(self, start: float = 0.0) -> None:
        self._now = start

    def now(self) -> float:
        return self._now

    def advance(self, seconds: float) -> None:
        if seconds < 0:
            raise ValueError("time does not run backwards in a replay")
        self._now += seconds
