import unittest

from pol import Baseline, PolScore, score_event

KEY = ("grounds_north", 27, "PERSON", False)  # 03:00 Saturday, unknown person


class TestBaseline(unittest.TestCase):
    def test_cold_start_is_rare_and_shadow(self):
        s = score_event(Baseline(), KEY)
        self.assertGreaterEqual(s.score, 0.8)
        self.assertTrue(s.shadow, "M0 pol is shadow-only; there is no live mode")

    def test_routine_bucket_scores_low(self):
        b = Baseline()
        for _ in range(50):
            b.observe(KEY, 1.0)  # one person every week at this hour: routine
        s = score_event(b, KEY, observed_count=1.0)
        self.assertLess(s.score, 0.2)

    def test_burst_in_quiet_bucket_scores_high(self):
        b = Baseline()
        for _ in range(50):
            b.observe(KEY, 0.0)  # nobody, ever, at 03:00
        s = score_event(b, KEY, observed_count=3.0)
        self.assertGreater(s.score, 0.7)

    def test_deterministic(self):
        def build():
            b = Baseline()
            for i in range(20):
                b.observe(KEY, float(i % 3))
            return score_event(b, KEY, 2.0)

        self.assertEqual(build(), build())

    def test_every_score_is_shadow(self):
        # There is deliberately no constructor argument, flag, or code path
        # that yields shadow=False in M0 (spec §9.5 cold start).
        self.assertTrue(PolScore(0.5, "x").shadow)


if __name__ == "__main__":
    unittest.main()
