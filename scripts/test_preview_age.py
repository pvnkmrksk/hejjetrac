"""Regression checks for the published timing distribution, not tracker tolerances."""
import json
from pathlib import Path
import tempfile
import unittest
from analyze_preview_age import distribution, summarize


class PreviewAgeTests(unittest.TestCase):
    def test_even_median_and_nearest_rank_percentile(self):
        result = distribution([40, 10, 30, 20])
        self.assertEqual(result["median_ms"], 25)
        self.assertEqual(result["p95_ms"], 40)
        self.assertEqual(result["maximum_ms"], 40)

    def test_invalid_timings_are_not_silently_filtered(self):
        for values in [[], [float("nan")], [float("inf")], [-1]]:
            with self.assertRaises(ValueError):
                distribution(values)

    def test_late_maximum_is_attributed_and_holds_are_excluded(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for phase in ["before-calibration", "after-calibration"]:
                records = [{"elapsed_seconds": t, "sessions": [{"session": 1, "preview_age_ms": age,
                    "camera_lag_ms": 2, "preview_fps": 25, "processing_ms": 4, "tracking_ms": 3}]}
                    for t, age in [(1, 10), (83, 49), (180, 20)]]
                (root / f"{phase}-samples.jsonl").write_text("\n".join(map(json.dumps, records)))
            result = summarize(root)
            self.assertEqual(result["maximum_observation"]["elapsed_seconds"], 83)
            self.assertEqual(result["aggregate"]["median_ms"], 20)
            self.assertEqual(result["per_phase"]["before-calibration"]["last_20_seconds"]["median_ms"], 20)
            path = root / "after-calibration-samples.jsonl"
            records[0]["sessions"][0]["paused"] = True
            path.write_text("\n".join(map(json.dumps, records)))
            with self.assertRaises(ValueError):
                summarize(root)


if __name__ == "__main__":
    unittest.main()
