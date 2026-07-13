import unittest

from perception import (
    BBox,
    CameraCalibration,
    Detection,
    FakeDetector,
    FixedClock,
    Homography,
    ManualClock,
    REASON_DEGENERATE,
    REASON_UNCALIBRATED,
    build_sighting,
)

# Identity-ish homography: image (u, v) maps to ground (u, v) directly.
IDENTITY = Homography(m=(1, 0, 0, 0, 1, 0, 0, 0, 1))


class TestClock(unittest.TestCase):
    def test_manual_clock_never_runs_backwards(self):
        c = ManualClock(10.0)
        c.advance(5.0)
        self.assertEqual(c.now(), 15.0)
        with self.assertRaises(ValueError):
            c.advance(-1.0)


class TestDetection(unittest.TestCase):
    def test_closed_class_set(self):
        with self.assertRaises(ValueError):
            Detection("BURGLAR", 0.9, BBox(0, 0, 10, 10))

    def test_confidence_bounds(self):
        with self.assertRaises(ValueError):
            Detection("PERSON", 1.2, BBox(0, 0, 10, 10))

    def test_fake_detector_is_deterministic(self):
        script = [[Detection("PERSON", 0.9, BBox(0, 0, 4, 8))], []]
        a = FakeDetector(script)
        b = FakeDetector(script)
        self.assertEqual(a.detect(b""), b.detect(b""))
        self.assertEqual(a.detect(b""), [])
        self.assertEqual(a.detect(b""), [])  # exhausted → empty forever


class TestSightingBuilder(unittest.TestCase):
    def test_calibrated_camera_projects_world_position(self):
        cal = CameraCalibration("CAM-04", calibrated=True, homography=IDENTITY)
        det = Detection("PERSON", 0.79, BBox(326, 121, 8, 8))
        s = build_sighting(det, cal, FixedClock(3.6))
        self.assertFalse(s.degraded)
        self.assertAlmostEqual(s.world_position.x, 330.0)
        self.assertAlmostEqual(s.world_position.y, 125.0)
        self.assertEqual(s.at, 3.6)

    def test_uncalibrated_camera_degrades_never_guesses(self):
        cal = CameraCalibration("CAM-NEW", calibrated=False)
        det = Detection("PERSON", 0.9, BBox(0, 0, 10, 10))
        s = build_sighting(det, cal, FixedClock(1.0))
        self.assertTrue(s.degraded)
        self.assertEqual(s.degraded_reason, REASON_UNCALIBRATED)
        self.assertIsNone(s.world_position)

    def test_degenerate_projection_is_refused(self):
        # Bottom row zeroes w for every point: the horizon case.
        h = Homography(m=(1, 0, 0, 0, 1, 0, 0, 0, 0))
        cal = CameraCalibration("CAM-04", calibrated=True, homography=h)
        det = Detection("PERSON", 0.9, BBox(10, 10, 4, 4))
        s = build_sighting(det, cal, FixedClock(1.0))
        self.assertTrue(s.degraded)
        self.assertEqual(s.degraded_reason, REASON_DEGENERATE)
        self.assertIsNone(s.world_position)


if __name__ == "__main__":
    unittest.main()
