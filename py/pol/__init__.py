"""AEGIS pattern-of-life tier (spec §9.5).

Learns what normal is so that abnormal is cheap to spot. SHADOW MODE IS THE
ONLY MODE IN M0: pol scores and logs; it never raises severity, and there is
deliberately no code path that could. The threat scorer consumes pol's number
as one explainable receipt term; when pol is unavailable it contributes 0 and
says so.
"""

from .baseline import Baseline, BaselineKey, PolScore, score_event

__all__ = ["Baseline", "BaselineKey", "PolScore", "score_event"]
