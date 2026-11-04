#!/usr/bin/env python3
"""Stress four real native cameras bound to burned serials M01--M04.

Development client only. Copies VR1--VR4 without changing their calibration,
uses deliberately stale numeric camera sources, disables experiment outputs,
and opens only its own native process. Retains timing transcripts, snapshots,
and short raw-video takes with complete numeric/quality evidence. No camera
firmware, original configuration, or existing process is changed.

Default observation is 60 seconds before and after sequential saved-calibration
reviews, followed by pause/resume, actual stream reopen, and three-second takes.
Live jitter is observed; there is no physical motion ground truth in this test.
"""
import argparse
from collections import Counter
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import time

from benchmark_vr3 import dat_rows
from test_native_rigs import NativeBridge, field


SERIALS = ["M01", "M02", "M03", "M04"]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def serial_inventory():
    """Read USB descriptors without opening a stream or guessing enumeration order."""
    inventory = []
    for video in sorted(Path("/sys/class/video4linux").glob("video*")):
        resolved = video.resolve()
        if (resolved / "index").read_text().strip() != "0":
            continue
        usb = next((p for p in resolved.parents if (p / "idVendor").exists()), None)
        if usb is None or not (usb / "serial").is_file():
            continue
        serial = (usb / "serial").read_text().strip()
        if serial not in SERIALS:
            continue
        vendor = (usb / "idVendor").read_text().strip().lower()
        product = (usb / "idProduct").read_text().strip().lower()
        inventory.append({"id": int(video.name.removeprefix("video")),
                          "path": f"/dev/{video.name}", "serial": serial,
                          "hardware_id": f"usb:{vendor}:{product}:{serial}:video:0",
                          "sysfs_path": str(resolved)})
    for serial in SERIALS:
        found = [camera for camera in inventory if camera["serial"] == serial]
        if len(found) != 1:
            raise RuntimeError(f"{serial} must identify exactly one capture node; got {found}")
    return {camera["serial"]: camera for camera in inventory}


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, math.ceil(len(ordered) * fraction) - 1)] if ordered else None


class TimedBridge(NativeBridge):
    def __init__(self, command, output, budget):
        self.timings = []
        self.budget = budget
        self.enforce_latency = False
        self.timing_file = (output / "command-latency.jsonl").open("w")
        super().__init__(command, output)

    def request(self, command, session=None, **args):
        started = time.monotonic()
        result = super().request(command, session, **args)
        elapsed = time.monotonic() - started
        item = {"command": command, "session": session, "seconds": elapsed,
                "steady_state_budget_enforced": self.enforce_latency}
        self.timings.append(item)
        self.timing_file.write(json.dumps(item) + "\n")
        self.timing_file.flush()
        if self.enforce_latency and elapsed > self.budget:
            raise RuntimeError(f"native {command} for session {session} took {elapsed:.3f}s "
                               f"(limit {self.budget:.3f}s)")
        return result

    def finish(self, identities=()):
        try:
            super().finish(identities)
        finally:
            self.timing_file.close()


def healthy(states):
    if len(states) != 4 or {s["session"] for s in states} != {1, 2, 3, 4}:
        raise RuntimeError(f"expected four native sessions, got {states}")
    for state in states:
        if (state["ended"] or state["error"] or state["command_error"]
                or not state["worker_active"] or state["sample_fallback_active"]):
            raise RuntimeError(f"live native session failed: {state}")


def wait(bridge, predicate, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        states = bridge.request("sessions")
        healthy(states)
        if predicate(states):
            return states
        time.sleep(.05)
    raise RuntimeError(f"native state did not converge in {timeout}s: {states}")


def verify_binding(states, inventory):
    healthy(states)
    for state in states:
        serial = SERIALS[state["session"] - 1]
        camera = inventory[serial]
        if (state["running_camera_binding"] != camera["hardware_id"]
                or state["camera_binding"] != camera["hardware_id"]
                or state["camera"] is None
                or not re.search(re.escape(camera["path"]) + r"(?:\D|$)", state["source_label"])):
            raise RuntimeError(f"{serial} opened a different physical camera: {state}")
        if state["source"] != "99999":
            raise RuntimeError(f"stale numeric source was rewritten instead of bound: {state}")


def timestamp(state):
    row = state["last_row"]
    if row is None or not math.isfinite(row["timestamp_ms"]):
        raise RuntimeError(f"missing finite live numeric row: {state}")
    return row["timestamp_ms"]


def compact(state):
    keys = ["session", "frames", "last_row", "source_label", "actual_fps", "source_fps",
            "processing_ms", "tracking_ms", "preview_fps", "dropped", "resources",
            "camera_lag_ms", "capture_age_ms", "preview_age_ms", "frame_age_ms", "path_points"]
    return {key: state[key] for key in keys if key in state}


def process_resources(pid):
    status = Path(f"/proc/{pid}/status")
    fields = dict(line.split(":", 1) for line in status.read_text().splitlines() if ":" in line)
    return {"rss_kib": int(fields["VmRSS"].split()[0]),
            "threads": int(fields["Threads"]),
            "open_fds": sum(1 for _ in Path(f"/proc/{pid}/fd").iterdir())}


def observe(bridge, inventory, seconds, label, output, clock_anchor=None):
    before = bridge.request("sessions")
    verify_binding(before, inventory)
    if any(s["paused"] for s in before):
        raise RuntimeError(f"observation {label} started paused")
    origin = time.monotonic()
    previous, previous_time = before, origin
    initial = {state["session"]: state for state in before}
    if clock_anchor is None:
        clock_anchor = {"wall": origin,
                        "sources": {state["session"]: timestamp(state) for state in before}}
    samples = []
    last_relative_measurement = {state["session"]: origin for state in before}
    with (output / f"{label}-samples.jsonl").open("w") as stream:
        while time.monotonic() - origin < seconds:
            time.sleep(min(.5, max(.01, seconds - (time.monotonic() - origin))))
            states = bridge.request("sessions")
            now = time.monotonic()
            verify_binding(states, inventory)
            old = {state["session"]: state for state in previous}
            elapsed = now - previous_time
            clock_lags = []
            for state in states:
                baseline = old[state["session"]]
                if state["last_row"]["accepted"] and state["last_row"]["status"] == "tracked":
                    last_relative_measurement[state["session"]] = now
                elif now - last_relative_measurement[state["session"]] > 1:
                    raise RuntimeError(f"{label}: camera advances but relative tracking is stalled: {state}")
                clock_delta = (timestamp(state) - timestamp(baseline)) / 1000
                if state["frames"] <= baseline["frames"] or clock_delta <= 0:
                    raise RuntimeError(f"{label}: live source/processing stalled: {state}")
                if abs(clock_delta - elapsed) > 1:
                    raise RuntimeError(f"{label}: source clock advanced {clock_delta:.3f}s "
                                       f"during {elapsed:.3f}s wall time: {state}")
                source_advance = (timestamp(state) - clock_anchor["sources"][state["session"]]) / 1000
                lag_growth = (now - clock_anchor["wall"]) - source_advance
                clock_lags.append({"session": state["session"], "seconds": lag_growth})
                if lag_growth > 1:
                    raise RuntimeError(f"{label}: source delivery accumulated {lag_growth:.3f}s "
                                       f"of lag since the source-clock anchor: {state}")
                # Exposed age measures are independent of the advertised FPS.
                for key in ["camera_lag_ms", "capture_age_ms", "preview_age_ms", "frame_age_ms"]:
                    value = state.get(key)
                    if value is not None and (not math.isfinite(value) or value > 1000):
                        raise RuntimeError(f"{label}: {key}={value} exceeds 1000ms: {state}")
            sample = {"elapsed_seconds": now - origin, "source_lag_growth": clock_lags,
                      "process": process_resources(bridge.process.pid),
                      "sessions": [compact(s) for s in states]}
            stream.write(json.dumps(sample) + "\n")
            stream.flush()
            samples.append(sample)
            previous, previous_time = states, now
    elapsed = previous_time - origin
    progress = []
    for state in previous:
        baseline = initial[state["session"]]
        source_seconds = (timestamp(state) - timestamp(baseline)) / 1000
        ratio = source_seconds / elapsed
        if not .9 <= ratio <= 1.1:
            raise RuntimeError(f"{label}: source clock/wall ratio {ratio:.3f} for session {state['session']}")
        progress.append({"session": state["session"], "serial": SERIALS[state["session"] - 1],
                         "frames": state["frames"] - baseline["frames"],
                         "processed_fps": (state["frames"] - baseline["frames"]) / elapsed,
                         "source_seconds": source_seconds, "source_to_wall_ratio": ratio})
    return {"elapsed_seconds": elapsed, "progress": progress,
            "process_initial": samples[0]["process"], "process_final": samples[-1]["process"],
            "peak_rss_kib": max(sample["process"]["rss_kib"] for sample in samples),
            "sample_count": len(samples), "initial": before, "final": previous}


def snapshot_all(bridge, output, label):
    result = []
    for identity in range(1, 5):
        directory = output / f"{label}-VR{identity}"
        saved = bridge.request("snapshot", identity, directory=str(directory))
        required = ["input.png", "roi.png", "instant-map.png", "accumulated-map.png", "debug.png"]
        for name in required:
            if not (directory / name).is_file():
                raise RuntimeError(f"native snapshot missing {directory / name}")
        result.append({"session": identity, "directory": str(directory),
                       "files_sha256": {name: sha256(directory / name) for name in required},
                       "status": saved["status"]})
    return result


def audit_take(directory, cli):
    rows = dat_rows(directory / "epoch-0000-all-frames.dat")
    with (directory / "epoch-0000-quality.csv").open(newline="") as stream:
        quality = list(csv.DictReader(stream))
    accepted = dat_rows(directory / "epoch-0000.dat")
    if not rows or len(rows) != len(quality):
        raise RuntimeError(f"empty/mismatched all-frame and quality evidence in {directory}")
    for row, decision in zip(rows, quality):
        if int(row[0]) != int(decision["frame"]) or abs(row[21] - float(decision["timestamp_ms"])) > 1e-5:
            raise RuntimeError(f"numeric and quality rows disagree in {directory}")
    if len(accepted) != sum(q["accepted"] == "true" for q in quality):
        raise RuntimeError(f"accepted-only output lacks matching quality decisions in {directory}")
    if any(b[0] != a[0] + 1 or b[21] <= a[21] for a, b in zip(rows, rows[1:])):
        raise RuntimeError(f"numeric evidence has nonconsecutive frames/nonmonotonic clocks in {directory}")
    tracked = [(row, q) for row, q in zip(rows, quality) if q["accepted"] == "true" and q["status"] == "tracked"]
    if not tracked:
        raise RuntimeError(f"live take contains no accepted relative tracking estimates: {directory}")
    video = directory / "epoch-0000-media" / "raw.avi"
    inspected = subprocess.run([str(cli), "inspect", str(video), "--decode"],
                               capture_output=True, text=True, check=True, timeout=120)
    match = re.search(r"decoded_frames=(\d+)(?:\s|$)", inspected.stdout)
    if match is None or int(match[1]) != len(rows):
        raise RuntimeError(f"raw-video decode does not match numeric evidence in {directory}: {inspected.stdout}")
    deltas = [math.sqrt(sum(value * value for value in row[1:4])) for row, _ in tracked]
    return {"directory": str(directory), "rows": len(rows), "accepted_rows": len(accepted),
            "quality_statuses": dict(Counter(q["status"] for q in quality)),
            "first_timestamp_ms": rows[0][21], "last_timestamp_ms": rows[-1][21],
            "observed_nonzero_rotation_rows": sum(delta > 1e-9 for delta in deltas),
            "rotation_delta_rad_p50": percentile(deltas, .5),
            "rotation_delta_rad_p95": percentile(deltas, .95),
            "rotation_delta_rad_max": max(deltas), "raw_video_sha256": sha256(video),
            "rust_decoder": inspected.stdout.strip()}


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    inventory = serial_inventory()
    report = {"passed": False, "inventory": inventory, "binary_sha256": sha256(args.binary),
              "platform": platform.platform(), "display": {key: os.environ.get(key) for key in ["DISPLAY", "WAYLAND_DISPLAY"]},
              "scope": "Live four-camera responsiveness, exact serial assignment, source-clock progress and numeric/media consistency. Airflow jitter has no physical ground-truth rotation oracle. Advertised FPS alone is not a latency measurement.",
              "observations": {}, "calibration_reviews": [], "captures": [], "reopens": [],
              "original_config_sha256": {}, "copied_config_sha256": {}}
    for identity, serial in enumerate(SERIALS, 1):
        source = args.rig_directory / f"VR{identity}.txt"
        report["original_config_sha256"][str(source.resolve())] = sha256(source)
        text = source.read_text()
        for key, value in [("src_fn", "99999"), ("rust_camera_id", inventory[serial]["hardware_id"]),
                           ("rust_instance_name", f"VR{identity}"), ("save_raw", "n"), ("save_debug", "n"),
                           ("sock_port", "0"), ("zmq_port", "0"), ("com_port", ""),
                           ("rust_camera_width", "320"), ("rust_camera_height", "240"), ("src_fps", "100")]:
            text = field(text, key, value)
        copied = output / f"VR{identity}.txt"
        copied.write_text(text)
        report["copied_config_sha256"][str(copied)] = sha256(copied)
    command = [str(args.binary.resolve()), "--config", str(output / "VR1.txt"), "--array",
               "--control-stdio", "--grid", "2x2", "--window-size", "480x360",
               "--screenshot", str(output / "native-root.png"), "--screenshot-after", "3"]
    report["command"] = command
    bridge = TimedBridge(command, output, args.max_command_seconds)
    try:
        discovery = bridge.request("discover")
        report["discovery"] = discovery
        needed = {"camera_list", "sessions", "start", "pause", "rewind", "view", "snapshot", "record_start", "record_stop"}
        if not needed.issubset(discovery["commands"]):
            raise RuntimeError(f"native discovery lacks {needed - set(discovery['commands'])}")
        cameras = bridge.request("camera_list")
        for serial, expected in inventory.items():
            matching = [c for c in cameras if c["hardware_id"] == expected["hardware_id"]]
            if len(matching) != 1 or matching[0]["id"] != expected["id"]:
                raise RuntimeError(f"native camera inventory disagrees with USB serial {serial}: {cameras}")
        report["native_inventory"] = cameras
        for identity in range(1, 5):
            bridge.request("start", identity)
        startup = wait(bridge, lambda states: all(s["frames"] >= 10 and s["last_row"] is not None for s in states))
        clock_anchor = {"wall": time.monotonic(),
                        "sources": {state["session"]: timestamp(state) for state in startup}}
        verify_binding(startup, inventory)
        report["startup"] = startup
        bridge.enforce_latency = True
        canonical = {identity: bridge.request("config", identity)["text"] for identity in range(1, 5)}
        report["binding_roundtrips"] = []
        for identity, serial in enumerate(SERIALS, 1):
            before = bridge.request("status", identity)
            expected = inventory[serial]
            bridge.request("set", identity, fields={"rust_camera_id": "", "src_fn": str(expected["id"])})
            unlinked = wait(bridge, lambda states: states[identity - 1]["revision"] > before["revision"]
                            and states[identity - 1]["frames"] > before["frames"])[identity - 1]
            bridge.request("set", identity, fields={"rust_camera_id": expected["hardware_id"], "src_fn": "99999"})
            relinked = wait(bridge, lambda states: states[identity - 1]["revision"] > unlinked["revision"]
                            and states[identity - 1]["frames"] > unlinked["frames"])[identity - 1]
            for state in [unlinked, relinked]:
                if (state["source_label"] != before["source_label"]
                        or state.get("actual_camera_id") != expected["id"]
                        or state["tracking_samples"] <= before["tracking_samples"]
                        or state["last_row"]["sequence"] <= before["last_row"]["sequence"]):
                    raise RuntimeError(f"link/unlink restarted or reassigned {serial}: {state}")
            if bridge.request("config", identity)["text"] != canonical[identity]:
                raise RuntimeError(f"link/unlink changed unrelated configuration for {serial}")
            report["binding_roundtrips"].append({"session": identity, "before": before,
                                                  "unlinked": unlinked, "relinked": relinked})
        report["observations"]["before_calibration"] = observe(bridge, inventory, args.seconds, "before-calibration", output, clock_anchor)
        report["snapshots_before"] = snapshot_all(bridge, output, "before")
        for identity in range(1, 5):
            before = bridge.request("status", identity)
            review_wall = time.monotonic()
            bridge.request("view", identity, calibration=True, calibration_step="ball", calibration_action="redraw")
            paused_review = wait(bridge, lambda states: states[identity - 1]["paused"])[identity - 1]
            if not paused_review["view"]["full_image"]:
                raise RuntimeError(f"VR{identity} calibration did not show the complete frame")
            time.sleep(args.pause_seconds)
            if not args.no_x11_captures and all(shutil.which(tool) for tool in ["xwininfo", "xwd"]):
                from test_daily_ui_visual import capture_window
                report["captures"].append(capture_window(output, f"calibration-VR{identity}", f"HejjéTrac · VR{identity} calibration"))
                main_title = bridge.request("status", identity)["window_title"]
                report["captures"].append(capture_window(output, f"calibration-frame-VR{identity}", main_title))
            for next_step in ["exclude", "lab", "view"]:
                step = bridge.request("view", identity, calibration_action="keep")
                if step["view"]["calibration_step"] != next_step:
                    raise RuntimeError(f"VR{identity} calibration keep did not advance to {next_step}: {step}")
            bridge.request("view", identity, calibration=False, settings=False)
            after = wait(bridge, lambda states: not states[identity - 1]["paused"]
                         and states[identity - 1]["frames"] > paused_review["frames"])[identity - 1]
            if (time.monotonic() - review_wall) - (timestamp(after) - timestamp(before)) / 1000 > 1:
                raise RuntimeError(f"VR{identity} calibration resumed an old camera image")
            if bridge.request("config", identity)["text"] != canonical[identity] or after["revision"] != before["revision"]:
                raise RuntimeError(f"saved-calibration review mutated VR{identity}'s configuration/tracker epoch")
            report["calibration_reviews"].append({"session": identity, "before": before, "after": after})
        report["observations"]["after_calibration"] = observe(bridge, inventory, args.seconds, "after-calibration", output, clock_anchor)
        report["snapshots_after"] = snapshot_all(bridge, output, "after")
        # Pause one source while the other three continue; resume must return to
        # the live capture clock rather than spend seconds draining old images.
        pause_take = output / "pause-boundary-take"
        bridge.request("record_start", 1, directory=str(pause_take),
                       record_options={"raw": True, "debug": False}, recording_tuning={"encoding": "raw"})
        started_take = wait(bridge, lambda states: states[0]["recording"])
        wait(bridge, lambda states: states[0]["frames"] >= started_take[0]["frames"] + 10)
        bridge.request("pause", 1)
        paused = wait(bridge, lambda states: states[0]["paused"])
        paused_at = time.monotonic()
        time.sleep(args.pause_seconds)
        during = bridge.request("sessions")
        if during[0]["frames"] != paused[0]["frames"] or any(s["frames"] <= p["frames"] for s, p in zip(during[1:], paused[1:])):
            raise RuntimeError("pausing one camera blocked another worker or failed to pause")
        bridge.request("start", 1)
        resumed = wait(bridge, lambda states: not states[0]["paused"] and states[0]["frames"] > paused[0]["frames"])
        resumed_clock = (timestamp(resumed[0]) - timestamp(paused[0])) / 1000
        resume_elapsed = time.monotonic() - paused_at
        if resumed_clock < resume_elapsed - 1:
            raise RuntimeError(f"paused camera resumed stale frames: source advanced {resumed_clock:.3f}s during {resume_elapsed:.3f}s")
        report["pause_resume"] = {"paused": paused, "during": during, "resumed": resumed,
                                  "source_advance_seconds": resumed_clock, "wall_seconds": resume_elapsed}
        retracked = wait(bridge, lambda states: states[0]["last_row"]["accepted"]
                         and states[0]["last_row"]["status"] == "tracked", timeout=5)
        report["pause_resume"]["retracked"] = retracked
        wait(bridge, lambda states: states[0]["frames"] >= retracked[0]["frames"] + 10)
        bridge.request("record_stop", 1)
        def pause_finalized(states):
            entries = [entry for entry in states[0]["recording_finalizations"] if entry["directory"] == str(pause_take)]
            if not entries or entries[-1]["state"] in ["queued", "finalizing"]:
                return False
            entry = entries[-1]
            if entry["state"] != "complete" or entry["numeric_error"] or entry["media_warnings"] or entry["completion_error"]:
                raise RuntimeError(f"pause-boundary take failed: {entry}")
            return True
        wait(bridge, pause_finalized)
        for identity in range(1, 5):
            bridge.request("rewind", identity)
            wait(bridge, lambda states: next(s for s in states if s["session"] == identity)["paused"])
            bridge.request("start", identity)
            states = wait(bridge, lambda states: all(not s["paused"] and s["frames"] >= 5 for s in states))
            verify_binding(states, inventory)
            report["reopens"].append({"session": identity, "states": states})
        takes = {identity: output / f"take-VR{identity}" for identity in range(1, 5)}
        for identity, directory in takes.items():
            bridge.request("record_start", identity, directory=str(directory),
                           record_options={"raw": True, "debug": False}, recording_tuning={"encoding": "raw"})
        wait(bridge, lambda states: all(s["recording"] for s in states))
        report["observations"]["recording"] = observe(bridge, inventory, args.record_seconds, "recording", output)
        for identity in range(1, 5):
            bridge.request("record_stop", identity)
        def finalized(states):
            for state in states:
                if state["recording"]:
                    return False
                entries = [entry for entry in state["recording_finalizations"] if entry["directory"] == str(takes[state["session"]])]
                if not entries or entries[-1]["state"] in ["queued", "finalizing"]:
                    return False
                entry = entries[-1]
                if (entry["state"] != "complete" or entry["completion_error"] or entry["numeric_error"] or entry["media_warnings"]):
                    raise RuntimeError(f"short raw take did not finalize cleanly: {entry}")
            return True
        report["final"] = wait(bridge, finalized, timeout=30)
        report["final_process_resources"] = process_resources(bridge.process.pid)
        report["snapshots_final"] = snapshot_all(bridge, output, "final")
        if not args.no_x11_captures and all(shutil.which(tool) for tool in ["xwininfo", "xwd"]):
            from test_daily_ui_visual import capture_window
            bridge.request("view", 1, settings=True)
            report["captures"].append(capture_window(output, "camera-link-controls", "HejjéTrac · VR1 configuration"))
            bridge.request("view", 1, settings=False)
        if not (output / "native-root.png").is_file():
            raise RuntimeError("native GPU framebuffer screenshot was not retained")
        # Stop this test's cameras before independent serial decoding, so the
        # media oracle cannot distort the measured live performance cases.
        for identity in reversed(range(1, 5)):
            bridge.request("close", identity, discard=True)
        bridge.process.wait(timeout=15)
        if bridge.process.returncode != 0:
            raise RuntimeError(f"native test process exited {bridge.process.returncode}")
        report["takes"] = [audit_take(directory, args.cli.resolve()) for directory in takes.values()]
        report["pause_boundary_take"] = audit_take(pause_take, args.cli.resolve())
        rows = dat_rows(pause_take / "epoch-0000-all-frames.dat")
        with (pause_take / "epoch-0000-quality.csv").open(newline="") as stream:
            decisions = list(csv.DictReader(stream))
        gaps = [i for i in range(1, len(rows)) if rows[i][21] - rows[i - 1][21] > args.pause_seconds * 500]
        if len(gaps) != 1:
            raise RuntimeError(f"pause take did not retain exactly one source-clock gap: {gaps}")
        gap = gaps[0]
        anchor = rows[gap - 1]
        resume_rows = []
        for row, decision in zip(rows[gap:], decisions[gap:]):
            if decision["status"] == "tracked":
                break
            if (any(value != 0 for value in row[1:4] + row[5:8])
                    or row[14:17] != anchor[14:17] or row[19:21] != anchor[19:21]):
                raise RuntimeError("resume invented motion across the intentionally paused interval")
            resume_rows.append(decision["status"])
        if not resume_rows or "relocalized" not in resume_rows:
            raise RuntimeError(f"pause take did not retain explicit zero-motion reanchoring: {resume_rows}")
        report["pause_boundary_take"]["resume_statuses"] = resume_rows
        report["pause_boundary_take"]["source_gap_ms"] = rows[gap][21] - anchor[21]
        report["passed"] = True
    except Exception as error:
        report["failure"] = str(error)
        raise
    finally:
        timing_values = [item["seconds"] for item in bridge.timings if item["steady_state_budget_enforced"]]
        report["command_latency_seconds"] = {"count": len(timing_values), "p50": percentile(timing_values, .5),
                                             "p95": percentile(timing_values, .95), "maximum": max(timing_values, default=None),
                                             "limit": args.max_command_seconds}
        bridge.finish(range(1, 5))
        report["exit_code"] = bridge.process.returncode
        changed = [path for path, digest in report["original_config_sha256"].items() if sha256(Path(path)) != digest]
        report["original_configs_unchanged"] = not changed
        if changed:
            report["passed"] = False
            report["original_config_changes"] = changed
        (output / "report.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(json.dumps({"passed": report["passed"], "report": str(output / "report.json"),
                      "command_latency_seconds": report["command_latency_seconds"]}, indent=2))
    if not report["passed"]:
        raise RuntimeError(f"live four-camera audit failed; see {output / 'report.json'}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac-desktop"))
    parser.add_argument("--cli", type=Path, default=Path("target/release/fictrac"))
    parser.add_argument("--rig-directory", type=Path, default=Path("."))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seconds", type=float, default=60, help="Observation seconds both before and after calibration reviews")
    parser.add_argument("--record-seconds", type=float, default=3)
    parser.add_argument("--pause-seconds", type=float, default=3)
    parser.add_argument("--max-command-seconds", type=float, default=1)
    parser.add_argument("--no-x11-captures", action="store_true", help="Retain built-in framebuffer and computed diagnostics only")
    options = parser.parse_args()
    for name in ["seconds", "record_seconds", "pause_seconds", "max_command_seconds"]:
        if not math.isfinite(getattr(options, name)) or not 0 < getattr(options, name) <= 3600:
            parser.error(f"--{name.replace('_', '-')} must be finite within (0, 3600]")
    run(options)
