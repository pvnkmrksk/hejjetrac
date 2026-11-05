#!/usr/bin/env python3
"""Review the four saved 47-degree fits in native windows on retained camera video.

Development-only bridge client. Never substitutes this replay for live-camera
validation; does not change the source configs or publish experiment outputs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from test_native_rigs import NativeBridge, field
from test_daily_ui_visual import capture_window


def run(args):
    output = args.output.resolve()
    if output.exists():
        output = output / f"rerun-{time.time_ns()}"
    output.mkdir(parents=True, exist_ok=False)
    for n in range(1, 5):
        text = Path(f"VR{n}.txt").read_text()
        video = args.recordings / f"take-VR{n}/epoch-0000-media/raw.avi"
        for key, value in [("src_fn", str(video.resolve())), ("rust_camera_id", ""),
                           ("save_raw", "n"), ("save_debug", "n"),
                           ("sock_port", "0"), ("zmq_port", "0"), ("com_port", "")]:
            text = field(text, key, value)
        (output / f"VR{n}.txt").write_text(text)
    bridge = NativeBridge([str(args.binary.resolve()), "--config", str(output / "VR1.txt"),
                           "--array", "--control-stdio", "--grid", "2x2",
                           "--window-size", "480x360"], output)
    report = {"passed": False, "scope": "Native replay of retained real camera video; not a live camera test",
              "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(), "captures": [], "rigs": []}
    try:
        bridge.wait(lambda ss: len(ss) == 4 and all(s["frames"] >= 5 for s in ss))
        for n in range(1, 5):
            bridge.request("pause", n)
        for n in range(1, 5):
            original = bridge.request("config", n)["text"]
            state = bridge.request("view", n, calibration=True, calibration_step="ball", calibration_action="edit",
                                   full_image=True, overlays=True)
            assert state["view"]["edit_marks"]
            deadline = time.monotonic() + 5
            while True:
                state = bridge.request("status", n)
                size = state["window_inner_size"]
                if size and size[0] >= 639 and size[1] >= 479:
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"Calibration window did not expand: {size}")
                time.sleep(.1)
            report["captures"].append(capture_window(output, f"ball-VR{n}", state["window_title"]))
            report["captures"].append(capture_window(output, f"controls-VR{n}", f"HejjéTrac · VR{n} calibration"))
            bridge.request("view", n, calibration_action="finish")
            deadline = time.monotonic() + 5
            while True:
                size = bridge.request("status", n)["window_inner_size"]
                if size and abs(size[0] - 480) <= 1 and abs(size[1] - 360) <= 1:
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"Calibration window did not restore: {size}")
                time.sleep(.1)
            assert bridge.request("config", n)["text"] == original, "Review changed saved configuration"
            report["rigs"].append(bridge.request("snapshot", n, directory=str(output / f"snapshot-VR{n}")))
            bridge.request("view", n, calibration=False, settings=False)
        bridge.request("rewind", 1)
        bridge.request("start", 1)
        capture = report["captures"][0]
        subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-f", "x11grab", "-framerate", "25",
                        "-window_id", capture["window_id"], "-i", os.environ.get("DISPLAY", ":0"),
                        "-t", "2", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-c:v", "libx264",
                        "-pix_fmt", "yuv420p", str(output / "native-replay.mp4")], check=True, timeout=20)
        report["screen_capture"] = str(output / "native-replay.mp4")
        report["passed"] = True
    finally:
        bridge.finish(range(1, 5))
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path("target/release/hejjetrac"))
    parser.add_argument("--recordings", type=Path, default=Path("runs/live-four-final-stress-20261103"))
    run(parser.parse_args())
