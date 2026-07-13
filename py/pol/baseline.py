"""Event-rate baselines per (zone, hour-of-week, object_class, identity_known).

A negative-binomial-flavoured rarity model in pure python: we accumulate
count mean/variance per bucket and score how surprising the current
observation is. The variance matters more than the mean (spec §9.5) — a
courtyard that sees 0-or-20 people is different from one that always sees 10.

Cold start: 21-day learning window with an archetype prior; during it pol
runs in shadow. That is represented here by `shadow=True` on every score —
M0 has no other value.
"""

from dataclasses import dataclass, field
from typing import Dict, Tuple

BaselineKey = Tuple[str, int, str, bool]  # (zone_id, hour_of_week, object_class, identity_known)


@dataclass(frozen=True)
class PolScore:
    score: float  # 0..1 rarity — 1 means "never seen here at this hour"
    note: str
    shadow: bool = True  # M0: always true. There is no live mode yet.


@dataclass
class Baseline:
    """Per-bucket count statistics, accumulated as Welford mean/variance."""

    counts: Dict[BaselineKey, Tuple[int, float, float]] = field(default_factory=dict)
    # (n_observations, mean, M2)

    def observe(self, key: BaselineKey, count: float) -> None:
        n, mean, m2 = self.counts.get(key, (0, 0.0, 0.0))
        n += 1
        delta = count - mean
        mean += delta / n
        m2 += delta * (count - mean)
        self.counts[key] = (n, mean, m2)

    def stats(self, key: BaselineKey) -> Tuple[int, float, float]:
        """(observations, mean, variance) for a bucket; variance uses the
        archetype prior floor of 1.0 so an empty bucket is 'rare', not
        'divide by zero'."""
        n, mean, m2 = self.counts.get(key, (0, 0.0, 0.0))
        variance = (m2 / (n - 1)) if n > 1 else 1.0
        return n, mean, max(variance, 1e-6)


def score_event(baseline: Baseline, key: BaselineKey, observed_count: float = 1.0) -> PolScore:
    """Rarity of seeing `observed_count` events in this bucket, as a bounded
    z-score squashed to (0, 1). Deterministic, explainable, and shadow-only."""
    n, mean, variance = baseline.stats(key)
    if n == 0:
        return PolScore(score=0.9, note="no baseline for this bucket yet (cold start prior)")
    z = (observed_count - mean) / (variance**0.5)
    # Squash: z<=0 → ~0 (less than normal is unremarkable), z=3 → ~0.78.
    score = max(0.0, min(1.0, z / 3.0)) if z > 0 else 0.0
    note = f"bucket n={n} mean={mean:.2f} var={variance:.2f} z={z:.2f}"
    return PolScore(score=score, note=note)
