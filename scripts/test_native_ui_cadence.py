#!/usr/bin/env python3
"""Measure actual native UI cadence without polling the app's control bridge.

Development-only client. Opens copied configs, records the normal UI's optional
state output, and never changes the user's saved calibration or camera settings.
"""
import argparse
import json
from pathlib import Path
import time

from test_native_rigs import NativeBridge, field


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for n in range(1, args.windows + 1):
        text = (args.configs / f"VR{n}.txt").read_text()
        changes = [("save_raw", "n"), ("save_debug", "n")]
        if args.disable_outputs:
            changes += [("sock_port", "0"), ("zmq_port", "0"), ("com_port", "")]
        if args.video:
            changes += [("src_fn", str(args.video.resolve())), ("rust_camera_id", "")]
        for key, value in changes:
            text = field(text, key, value)
        (output / f"VR{n}.txt").write_text(text)
    command = [str(args.binary.resolve()), "--config", str(output / "VR1.txt"),
               "--window-size", "480x360", "--grid", "2x2", "--state-output",
               str(output / "state.json"), "--control-stdio"]
    if args.windows > 1:
        command.append("--array")
    samples = []
    with (output / "samples.jsonl").open("w") as transcript:
        bridge = NativeBridge(command, output)
        process = bridge.process
        origin = time.monotonic()
        previous = None
        try:
            while process.poll() is None and time.monotonic() - origin < args.seconds:
                time.sleep(.1)
                try:
                    state = json.loads((output / "state.json").read_text())
                except (FileNotFoundError, json.JSONDecodeError):
                    continue
                if state["elapsed_seconds"] == previous:
                    continue
                previous = state["elapsed_seconds"]
                item = {"host_elapsed_seconds": time.monotonic() - origin, **state}
                for instance in item["instances"]:
                    instance.pop("config_text", None)
                transcript.write(json.dumps(item) + "\n")
                transcript.flush()
                samples.append(item)
        finally:
            # No control requests were sent during measurement. Close the
            # copied sessions through their normal API only after it ends.
            bridge.finish(range(1, args.windows + 1))
    report = {"command": command, "outputs_disabled": args.disable_outputs,
              "config_directory": str(args.configs.resolve()), "sample_count": len(samples),
              "exit_code": process.returncode, "last": samples[-1] if samples else None,
              "maximum_state_gap_seconds": max((b["elapsed_seconds"] - a["elapsed_seconds"]
                  for a, b in zip(samples, samples[1:])), default=None)}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--configs", type=Path, default=Path("/home/pkaushi/.config/hejjetrac"))
    parser.add_argument("--binary", type=Path, default=Path("target/release/hejjetrac"))
    parser.add_argument("--seconds", type=float, default=40)
    parser.add_argument("--windows", type=int, choices=[1, 4], default=4)
    parser.add_argument("--disable-outputs", action="store_true")
    parser.add_argument("--video", type=Path)
    run(parser.parse_args())
