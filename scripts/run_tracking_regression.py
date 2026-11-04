#!/usr/bin/env python3
"""Serial release checks on the retained sample and private high-speed fixtures.

Linux development validation only. Uses the Rust CLI/decoder and retained analysis
scripts; original FicTrac is an explicit comparison tool, never an application
dependency. Inputs are read-only and the output directory must be new. Run
after builds and live camera tests finish so timing boundaries remain useful.
"""

import argparse
from collections import Counter
import json
import math
import os
from pathlib import Path
import platform
import resource
import subprocess
import sys
import time

from analyze_raw_tracking import dat_rows, sha256
from benchmark_takes import set_field


ROOT = Path(__file__).resolve().parent.parent


def multiply(left, right):
    w, x, y, z = left
    a, b, c, d = right
    return (w * a - x * b - y * c - z * d,
            w * b + x * a + y * d - z * c,
            w * c - x * d + y * a + z * b,
            w * d + x * c - y * b + z * a)


def quaternion(vector):
    angle = math.sqrt(sum(value * value for value in vector))
    scale = math.sin(angle / 2) / angle if angle else .5
    return (math.cos(angle / 2), *(value * scale for value in vector))


def scaled_axis(value):
    norm = math.sqrt(sum(component * component for component in value))
    value = tuple(component / norm for component in value)
    if value[0] < 0:
        value = tuple(-component for component in value)
    sine = math.sqrt(sum(component * component for component in value[1:]))
    scale = 2 * math.atan2(sine, value[0]) / sine if sine else 2
    return [component * scale for component in value[1:]]


def rms(vectors):
    return [math.sqrt(sum(vector[axis] ** 2 for vector in vectors) / len(vectors))
            for axis in range(3)] if vectors else None


def fast_comparison(path, reference, clock, omit_period):
    records = [json.loads(line) for line in path.read_text().splitlines()]
    expected = [index for index in range(4579)
                if not omit_period or index % omit_period < omit_period - 3]
    if [entry["source_frame_index"] for entry in records] != expected:
        raise RuntimeError(f"source image count/order differs in {path}")
    statuses = Counter(entry["row"]["status"] for entry in records)
    differences, extended = [], []
    timestamp_error = 0.
    for position, entry in enumerate(records):
        index, row = entry["source_frame_index"], entry["row"]
        timestamp_error = max(timestamp_error, abs(row["timestamp_ms"] - clock[index][21]))
        if not row["accepted"] or row["status"] != ("tracked" if position else "initializing"):
            raise RuntimeError(f"clear fast fixture has an unmeasured image at {index}: {row['status']}")
        if not all(math.isfinite(value) for value in row["delta_camera"]):
            raise RuntimeError(f"nonfinite fast fixture rotation at {index}")
        if not position:
            continue
        previous = records[position - 1]["source_frame_index"]
        composed = (1., 0., 0., 0.)
        for source_index in range(previous + 1, index + 1):
            composed = multiply(quaternion(reference[source_index][1:4]), composed)
        target = scaled_axis(composed)
        difference = [row["delta_camera"][axis] - target[axis] for axis in range(3)]
        differences.append(difference)
        if index != previous + 1:
            extended.append(difference)
    component_rms = rms(differences)
    rolling = [rms(differences[index:index + 100])
               for index in range(len(differences) - 99)]
    worst = [max(window[axis] for window in rolling) for axis in range(3)]
    gates = {"every_clear_image_measured": statuses["tracked"] == len(records) - 1,
             "source_clock_preserved": timestamp_error < 1e-6,
             "all_decision_component_rms_below_020": max(component_rms) < .020,
             "every_100_interval_component_rms_below_020": max(worst) < .020,
             "extended_interval_count": len(extended) == (57 if omit_period else 0)}
    return {"processed_images": len(records), "statuses": dict(statuses),
            "all_decision_component_rms_rad": component_rms,
            "extended_intervals": len(extended),
            "extended_interval_component_rms_rad": rms(extended),
            "worst_100_interval_component_rms_rad": worst,
            "maximum_source_clock_error_ms": timestamp_error,
            "evidence_sha256": sha256(path), "gates": gates,
            "passes": all(gates.values())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/fictrac")
    parser.add_argument("--evidence-binary", type=Path,
                        default=ROOT / "target/release/examples/replay_evidence")
    parser.add_argument("--original-fictrac", type=Path,
                        default=Path.home() / "src/fictrac/bin/fictrac")
    parser.add_argument("--vr3-directory", type=Path,
                        default=ROOT.parent / "Hejjetrac-data-20261029/performance/source-fixture")
    parser.add_argument("--vr3-replay-dat", type=Path,
                        help="Existing same-AVI original FicTrac replay; omitted means create a fresh replay")
    parser.add_argument("--raw47-take", type=Path,
                        default=Path.home() / "Videos/hejjetrak/VR1/take-1793134575426458881")
    parser.add_argument("--fast47-directory", type=Path,
                        default=ROOT / "runs/live47-fast-20261028")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report", type=Path,
                        help="Optional new compact JSON report outside the run directory")
    args = parser.parse_args()
    output = args.output.resolve()
    binary = args.binary.resolve(strict=True)
    evidence = args.evidence_binary.resolve(strict=True)
    original = args.original_fictrac.resolve(strict=True)
    vr3_directory = args.vr3_directory.resolve(strict=True)
    vr3_replay = args.vr3_replay_dat.resolve(strict=True) if args.vr3_replay_dat else None
    fast_directory = args.fast47_directory.resolve(strict=True)
    fast_video = fast_directory / "recording/epoch-0000-media/raw.avi"
    fast_clock = fast_directory / "recording/epoch-0000-all-frames.dat"
    fast_reference = fast_directory / "rim-check/fictrac/baseline-20261028_120425.dat"
    fast_config = fast_directory / "rim-check/config.txt"
    # Resolve every retained input before starting expensive tracking. The live
    # upstream rig directory is mutable and its old data were archived; use the
    # preserved fixture/config instead of whichever calibration is live today.
    fixture_hashes = [
        (vr3_directory / "config.txt", "34cc8788f8f5d01369b472acaab72f08c48daa17f3f3c79ad7d73c1f9b58982f"),
        (vr3_directory / "fictrac-20261026_212053.dat", "a8bfa7306a2144c5a0233739f9e796bff35fd1f3a7dc371392a18c8f8a852475"),
        (vr3_directory / "fictrac-raw-20261026_212053.avi", "0cdb83d8468fce955999a54bd04cfde48e652db8c36de9fc45b15d359c521e4b"),
        (vr3_directory / "fictrac-vidLogFrames-20261026_212053.txt", "5ad33ec6eebc6cef3444705cbe668da18038e5b1ff6fafce8c671d29a1f3b899"),
        (fast_video, "20e1d79f84d85c9b638acb34bdcdb54813f6fcd603a96075796c0fdd11172c45"),
        (fast_clock, "15ae6ed354f9db37697399712069508dd8760e60d17f03471972724c7aacd4cd"),
        (fast_reference, "09417db7706c3029c6b6b082c2e5a201fc37d10418c6eeb74f01587ae40af638"),
        (fast_config, "18e54b759dafbc5037ae77db235700254652497120b903f7a201f9e13281e1a5")]
    for path, expected_hash in fixture_hashes:
        if sha256(path) != expected_hash:
            parser.error(f"retained input/oracle changed: {path}")
    for name in ["completion.json", "epoch-0000-config.txt", "epoch-0000-quality.csv"]:
        (args.raw47_take / name).resolve(strict=True)
    for name in ["clock-oracle-final-complete.jsonl", "clock-oracle-final-omit80.jsonl"]:
        (fast_directory / name).resolve(strict=True)
    if args.report and args.report.exists():
        parser.error("--report must be a new file")
    output.mkdir(parents=True, exist_ok=False)
    report = {"status": "running", "date": time.strftime("%Y-%m-%d"),
              "platform": platform.platform(), "logical_cpus": os.cpu_count(),
              "cpu_model": next((line.split(":", 1)[1].strip()
                                  for line in Path("/proc/cpuinfo").read_text().splitlines()
                                  if line.startswith("model name")), "unavailable"),
              "build_profile": "Workspace release, thin LTO, codegen-units=1; binaries built before this suite",
              "rustflags": os.environ.get("RUSTFLAGS"),
              "binary": str(binary), "binary_sha256": sha256(binary),
              "evidence_binary_sha256": sha256(evidence),
              "original_fictrac_sha256": sha256(original),
              "retained_fixture_sha256": {str(path): digest for path, digest in fixture_hashes},
              "timing_boundary": "Each command runs serially. External wall includes process startup, input/config/decode, numeric outputs and finalization. VR3 and raw47 wrapper timing additionally includes independent reference/control and Python analysis; candidate summary retains tracker/pipeline boundaries. No machine-wide quietness is inferred by this script.",
              "accuracy_scope": "Fixed fixtures and original FicTrac comparisons, not physical ground truth. Existing tolerances and annotations unchanged; all numerical/quality rows retained. Gap reference composes original delta quaternions across explicitly omitted source images without fitting lag, scale or axes.",
              "cases": {}, "commands": []}
    report["source_commit"] = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    report["source_dirty"] = subprocess.check_output(
        ["git", "status", "--short"], cwd=ROOT, text=True).splitlines()
    report["core_source_sha256"] = {str(path.relative_to(ROOT)): sha256(path)
                                    for path in sorted((ROOT / "crates/fictrac-core/src").glob("*.rs"))}
    report["rustc_verbose"] = subprocess.check_output(["rustc", "-vV"], text=True).strip()

    def save():
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    def execute(name, command, directory=ROOT):
        command = [str(value) for value in command]
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        started = time.monotonic()
        with (output / f"{name}.log").open("x") as stream:
            result = subprocess.run(command, cwd=directory, stdout=stream,
                                    stderr=subprocess.STDOUT, timeout=900)
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        report["commands"].append({"name": name, "command": command,
                                   "wall_seconds": time.monotonic() - started,
                                   "child_user_seconds": after.ru_utime - before.ru_utime,
                                   "child_system_seconds": after.ru_stime - before.ru_stime,
                                   "exit_code": result.returncode, "cwd": str(directory)})
        save()
        if result.returncode:
            raise RuntimeError(f"{name} failed; inspect {output / (name + '.log')}")

    def read(path):
        return json.loads(path.read_text())

    try:
        execute("demo120", [binary, "demo", "--frames", 120, "--output", output / "demo120"])
        demo = read(output / "demo120/oracle.json")
        report["cases"]["demo120"] = {"oracle": demo,
            "summary": read(output / "demo120/tracking/summary.json")}
        if not demo["passed"] or report["cases"]["demo120"]["summary"]["frames"] != 120:
            raise RuntimeError("known-motion demo did not pass its unchanged gate")

        sample = ROOT / "examples/sample.mp4"
        if sha256(sample) != "afdb193b1555574f9c9eba660eac8eb2531fa9db62e6876afb87fd4fe36753ad":
            raise RuntimeError("sample fixture hash changed")
        text = (ROOT / "examples/benchmark/config.txt").read_text()
        for key, value in [("save_raw", "n"), ("save_debug", "n"), ("sock_port", "0"),
                           ("zmq_port", "0"), ("com_port", "")]:
            text = set_field(text, key, value)
        sample_config = output / "sample-config.txt"
        sample_config.write_text(text)
        execute("sample300", [binary, "track", "--config", sample_config,
                               "--input", sample, "--output", output / "sample300"])
        execute("sample-compare", [binary, "compare", "--reference", ROOT / "examples/benchmark/upstream.dat",
                                   "--candidate", output / "sample300/all-frames.dat",
                                   "--report", output / "sample-comparison.json"])
        report["cases"]["sample300"] = {"input_sha256": sha256(sample),
            "summary": read(output / "sample300/summary.json"),
            "comparison": read(output / "sample-comparison.json")}
        sample_summary = report["cases"]["sample300"]["summary"]
        if sample_summary["frames"] != 300 or sample_summary["rejected"] != 0:
            raise RuntimeError("sample image count/acceptance changed")

        if vr3_replay is None:
            upstream_directory = output / "upstream-vr3"
            upstream_directory.mkdir()
            (upstream_directory / "input.avi").symlink_to(
                vr3_directory / "fictrac-raw-20261026_212053.avi")
            text = (vr3_directory / "config.txt").read_text()
            for key, value in [("src_fn", "input.avi"), ("output_fn", "baseline"),
                               ("do_display", "n"), ("save_raw", "n"), ("save_debug", "n"),
                               ("sock_port", "-1"), ("zmq_port", "0"), ("com_port", "")]:
                text = set_field(text, key, value)
            (upstream_directory / "config.txt").write_text(text)
            execute("upstream-vr3", [original, "config.txt"], upstream_directory)
            references = list(upstream_directory.glob("*.dat"))
            if len(references) != 1:
                raise RuntimeError("expected exactly one fresh original FicTrac VR3 replay")
            vr3_replay = references[0]
        report["vr3_same_avi_reference"] = {"path": str(vr3_replay), "sha256": sha256(vr3_replay),
            "scope": "Original FicTrac replay of the exact compressed AVI, separate from preserved live source rows; not physical ground truth."}

        execute("vr3", [sys.executable, ROOT / "scripts/benchmark_vr3.py", "--binary", binary,
                         "--reference-dir", vr3_directory,
                         "--candidate-dir", output / "vr3", "--output", output / "vr3-comparison.json",
                         "--suffix-control-dir", output / "vr3-control", "--make-suffix-control",
                         "--upstream-replay-dat", vr3_replay, "--require-pass"])
        vr3 = read(output / "vr3-comparison.json")
        report["cases"]["vr3"] = {key: vr3[key] for key in
            ["input_sha256", "video_frames", "healthy_pre_hand", "fast_section", "post_hand",
             "fresh_suffix_control", "gates", "passes", "candidate_summary", "process_wall_seconds"]}
        if vr3["video_frames"] != 5157:
            raise RuntimeError("VR3 image count changed")

        execute("raw47", [sys.executable, ROOT / "scripts/benchmark_takes.py",
                           "--binary", binary, "--original-fictrac", original,
                           "--take", args.raw47_take.resolve(),
                           "--annotations", ROOT / "examples/benchmark/takes-20261027.json",
                           "--output", output / "raw47", "--require-pass"])
        raw47 = read(output / "raw47/report.json")["takes"][0]
        report["cases"]["raw47"] = {key: raw47[key] for key in
            ["video_sha256", "video_frames", "source_config_sha256", "source_config_unchanged",
             "all_frames_without_exclusions", "occlusion_intervals", "annotation_provenance",
             "clear", "fast", "worst_clear_100_image_component_rms", "gates", "passes",
             "all_frames_sha256", "quality_sha256", "reference_sha256"]}
        report["cases"]["raw47"]["summary"] = read(output / "raw47/take-0/rust/summary.json")
        if raw47["video_frames"] != 6075:
            raise RuntimeError("raw47 image count changed")

        reference, capture_rows = dat_rows(fast_reference), dat_rows(fast_clock)
        first_capture = next(iter(capture_rows))
        if (list(reference) != list(range(4579))
                or list(capture_rows) != list(range(first_capture, first_capture + 4579))):
            raise RuntimeError("fast47 source clock/reference image count changed")
        # Recording begins at live capture 19. Video image zero uses the first
        # retained clock row, matching replay_evidence's SOURCE_DAT_CLOCK input.
        clock = list(capture_rows.values())
        for name, period in [("fast47-complete", None), ("fast47-gap80", 80)]:
            path = output / f"{name}.jsonl"
            command = [evidence, fast_video, fast_config, 4579, path, fast_clock]
            if period:
                command.append(period)
            execute(name, command)
            comparison = fast_comparison(path, reference, clock, period)
            comparison["input_sha256"] = sha256(fast_video)
            retained = fast_directory / ("clock-oracle-final-omit80.jsonl" if period
                                         else "clock-oracle-final-complete.jsonl")
            comparison["byte_identical_to_retained_before_binding"] = sha256(path) == sha256(retained)
            report["cases"][name] = comparison
            save()
            if not comparison["passes"]:
                raise RuntimeError(f"{name} failed unchanged increment/continuity gates")
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
        raise
    finally:
        save()
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            with args.report.open("x") as stream:
                json.dump(report, stream, indent=2)
                stream.write("\n")
    print(json.dumps({"status": report["status"], "report": str(output / "report.json")}))


if __name__ == "__main__":
    main()
