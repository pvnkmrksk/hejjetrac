#!/usr/bin/env python3
"""Summarize sampled live preview age without mixing intentional calibration holds.

Development analysis only. Age is relative to the fastest observed source
delivery, not an absolute exposure-to-screen measurement.
"""
import argparse
import json
import math
from pathlib import Path
import statistics


def distribution(values):
    if not values or any(not math.isfinite(v) or v < 0 for v in values):
        raise ValueError("timing samples must be nonempty, finite and nonnegative")
    ordered = sorted(values)
    return {"samples": len(values), "median_ms": statistics.median(values),
            "p95_ms": ordered[math.ceil(.95 * len(ordered)) - 1],
            "maximum_ms": ordered[-1]}


def summarize(directory):
    samples = []
    phases = {}
    for phase in ["before-calibration", "after-calibration"]:
        records = [json.loads(line) for line in (directory / f"{phase}-samples.jsonl").read_text().splitlines()]
        phase_samples = []
        for record in records:
            for state in record["sessions"]:
                if state.get("paused") or state.get("ended"):
                    raise ValueError("observation contains a deliberate hold or ended source")
                item = {"phase": phase, "elapsed_seconds": record["elapsed_seconds"],
                        "session": state["session"], "age_ms": state["preview_age_ms"],
                        "backlog_ms": state["camera_lag_ms"],
                        "preview_fps": state["preview_fps"],
                        "worker_ms": state["processing_ms"], "tracking_ms": state["tracking_ms"]}
                phase_samples.append(item)
        phases[phase] = distribution([s["age_ms"] for s in phase_samples])
        phases[phase]["first_20_seconds"] = distribution([s["age_ms"] for s in phase_samples if s["elapsed_seconds"] <= 20])
        end = max(s["elapsed_seconds"] for s in phase_samples)
        phases[phase]["last_20_seconds"] = distribution([s["age_ms"] for s in phase_samples if s["elapsed_seconds"] >= end - 20])
        samples.extend(phase_samples)
    maximum = max(samples, key=lambda s: s["age_ms"])
    return {"definition": "Monotonic age of the latest published source image relative to fastest observed delivery; not absolute sensor/display latency.",
            "selection": "Both 180-second steady-state phases; intentional calibration holds and short recording phase excluded.",
            "percentile": "p95 uses nearest rank; median averages the two middle samples when count is even",
            "aggregate": distribution([s["age_ms"] for s in samples]),
            "per_camera": {str(i): distribution([s["age_ms"] for s in samples if s["session"] == i]) for i in sorted({s["session"] for s in samples})},
            "per_phase": phases, "maximum_observation": maximum,
            "maximum_relative_capture_backlog_ms": max(s["backlog_ms"] for s in samples),
            "median_observed_preview_fps": statistics.median(s["preview_fps"] for s in samples),
            "configured_display_limit_fps": 30,
            "explanation": "At approximately 100 input FPS, the 33.33 ms publication minimum is tested on discrete source arrivals; previews commonly publish every fourth arrival (about 40 ms, roughly 25 FPS). Source delivery/conversion, fitting and the time until the next image publication/read contribute to preview age. The maximum is in steady state, not only startup. Flat early/late distributions and bounded capture backlog argue against an accumulating queue on this fixture."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = summarize(args.directory)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["aggregate"], indent=2))
