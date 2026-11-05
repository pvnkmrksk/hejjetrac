#!/usr/bin/env python3
"""Reproduce multiwindow UI stalls with real X11 pointer input on copied rigs.

Development only. Moves the pointer over this test process's native windows.
Exercises covered windows, diagnostics, settings, four calibration magnifiers,
optional saved-rim dragging and a resize. Worker FPS is not a UI-latency oracle.
"""
import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import re
import subprocess
import threading
import time

from test_daily_ui_visual import capture_window
from test_native_rigs import NativeBridge, field


class Pointer:
    def __init__(self, bridge):
        self.bridge = bridge
        self.x = ctypes.CDLL("libX11.so.6")
        self.xt = ctypes.CDLL("libXtst.so.6")
        signatures = {
            "XOpenDisplay": ([ctypes.c_char_p], ctypes.c_void_p),
            "XWarpPointer": ([ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong,
                              ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint,
                              ctypes.c_int, ctypes.c_int], ctypes.c_int),
            "XRaiseWindow": ([ctypes.c_void_p, ctypes.c_ulong], ctypes.c_int),
            "XIconifyWindow": ([ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int], ctypes.c_int),
            "XMapRaised": ([ctypes.c_void_p, ctypes.c_ulong], ctypes.c_int),
            "XDefaultScreen": ([ctypes.c_void_p], ctypes.c_int),
            "XFlush": ([ctypes.c_void_p], ctypes.c_int),
            "XCloseDisplay": ([ctypes.c_void_p], ctypes.c_int),
            "XGetImage": ([ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
                           ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_int], ctypes.c_void_p),
            "XGetPixel": ([ctypes.c_void_p, ctypes.c_int, ctypes.c_int], ctypes.c_ulong),
            "XDestroyImage": ([ctypes.c_void_p], ctypes.c_int),
        }
        for name, (arguments, result) in signatures.items():
            operation = getattr(self.x, name)
            operation.argtypes, operation.restype = arguments, result
        self.xt.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        self.display = self.x.XOpenDisplay(None)
        if not self.display:
            raise RuntimeError("An accessible native X11 display is required")

    def window(self, identity):
        def title():
            state = self.bridge.request("status", identity)
            return state.get("host_window_title", state["window_title"])
        return self.named_window(title)

    def named_window(self, title):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            name = title() if callable(title) else title
            result = subprocess.run(["xwininfo", "-root", "-tree"], text=True, capture_output=True)
            matches = re.findall(r'(0x[0-9a-fA-F]+)\s+' + re.escape('"' + name + '"'), result.stdout)
            for match in matches:
                window = int(match, 16)
                owner = subprocess.check_output(["xprop", "-id", hex(window), "_NET_WM_PID"], text=True)
                process = int(owner.rsplit("=", 1)[1].strip())
                while process > 1:
                    if process == self.bridge.process.pid:
                        return window
                    stat = Path(f"/proc/{process}/stat").read_text().rsplit(")", 1)[1].split()
                    process = int(stat[1])
                # A live user run may have the same inspector title. Never
                # interact with it; continue searching for our owned window.
            time.sleep(.1)
        raise RuntimeError("Native test window did not appear")

    def move(self, window, x, y):
        self.x.XWarpPointer(self.display, 0, window, 0, 0, 0, 0, round(x), round(y))
        self.x.XFlush(self.display)

    def raise_window(self, window):
        self.x.XRaiseWindow(self.display, window)
        self.x.XFlush(self.display)

    def minimize(self, window):
        self.x.XIconifyWindow(self.display, window, self.x.XDefaultScreen(self.display))
        self.x.XFlush(self.display)

    def restore(self, window):
        self.x.XMapRaised(self.display, window)
        self.x.XFlush(self.display)

    def button(self, pressed):
        self.xt.XTestFakeButtonEvent(self.display, 1, int(pressed), 0)
        self.x.XFlush(self.display)

    def click(self, window, x, y):
        self.raise_window(window)
        self.move(window, x, y)
        time.sleep(.15)
        self.button(True)
        self.button(False)

    def pixel(self, window, x, y):
        image = self.x.XGetImage(self.display, window, x, y, 1, 1, ctypes.c_ulong(-1).value, 2)
        if not image:
            raise RuntimeError("Cannot read the native test button pixel")
        try:
            return self.x.XGetPixel(image, 0, 0)
        finally:
            self.x.XDestroyImage(image)

    def request_close(self, window):
        """Send WM_DELETE_WINDOW only to a window already verified as ours."""
        class Data(ctypes.Union):
            _fields_ = [('b',ctypes.c_char*20),('s',ctypes.c_short*10),('l',ctypes.c_long*5)]
        class Message(ctypes.Structure):
            _fields_ = [('type',ctypes.c_int),('serial',ctypes.c_ulong),('send_event',ctypes.c_int),
                        ('display',ctypes.c_void_p),('window',ctypes.c_ulong),('message_type',ctypes.c_ulong),
                        ('format',ctypes.c_int),('data',Data)]
        class Event(ctypes.Union):
            _fields_ = [('message',Message),('pad',ctypes.c_long*24)]
        self.x.XInternAtom.argtypes = [ctypes.c_void_p,ctypes.c_char_p,ctypes.c_int]
        self.x.XInternAtom.restype = ctypes.c_ulong
        self.x.XSendEvent.argtypes = [ctypes.c_void_p,ctypes.c_ulong,ctypes.c_int,ctypes.c_long,ctypes.POINTER(Event)]
        self.x.XSendEvent.restype = ctypes.c_int
        event = Event()
        event.message.type = 33
        event.message.send_event = 1
        event.message.display = self.display
        event.message.window = window
        event.message.message_type = self.x.XInternAtom(self.display,b'WM_PROTOCOLS',0)
        event.message.format = 32
        event.message.data.l[0] = self.x.XInternAtom(self.display,b'WM_DELETE_WINDOW',0)
        if not self.x.XSendEvent(self.display,window,0,0,ctypes.byref(event)):
            raise RuntimeError('Cannot send the owned native window close request')
        self.x.XFlush(self.display)

    def close(self):
        self.x.XCloseDisplay(self.display)


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    originals = {}
    for n in range(1, 5):
        source = args.configs / f"VR{n}.txt"
        originals[str(source.resolve())] = hashlib.sha256(source.read_bytes()).hexdigest()
        text = source.read_text()
        for key, value in [("save_raw", "n"), ("save_debug", "n"),
                           ("sock_port", "0"), ("zmq_port", "0"), ("com_port", "")]:
            text = field(text, key, value)
        if args.video:
            for key, value in [("src_fn", str(args.video.resolve())), ("rust_camera_id", ""),
                               ("sock_port", "0"), ("zmq_port", "0"), ("com_port", "")]:
                text = field(text, key, value)
        (output / source.name).write_text(text)
    command = [str(args.binary.resolve()), "--config", str(output / "VR1.txt"), "--array",
               "--control-stdio", "--grid", "2x2", "--window-size", "832x468",
               "--state-output", str(output / "state.json")]
    bridge = NativeBridge(command, output)
    pointer = Pointer(bridge)
    stop = threading.Event()
    phase = "startup"
    samples, latencies, drags = [], [], []
    report = {"passed": False, "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              "command": command, "original_config_sha256": originals, "captures": [],
              "scope": "Native X11 window/input timing on live copied rigs; no physical motion ground truth"}
    if args.video:
        report["scope"] = "Native X11 window/input timing on copied rigs with real recorded video; not a live-camera throughput test"
        report["video"] = {"path": str(args.video.resolve()), "sha256": hashlib.sha256(args.video.read_bytes()).hexdigest()}

    def monitor():
        previous = None
        with (output / "timing.jsonl").open("w") as transcript:
            while not stop.wait(.1):
                try:
                    state = json.loads((output / "state.json").read_text())
                except (FileNotFoundError, json.JSONDecodeError):
                    continue
                if state["elapsed_seconds"] == previous:
                    continue
                previous = state["elapsed_seconds"]
                state["phase"] = phase
                for instance in state["instances"]:
                    instance.pop("config_text", None)
                samples.append(state)
                transcript.write(json.dumps(state) + "\n")
                transcript.flush()

    thread = threading.Thread(target=monitor)
    thread.start()

    def input_probe():
        root = pointer.window(1)
        pointer.raise_window(root)
        pointer.move(root, 151, 20)
        time.sleep(.4)
        for _ in range(6):
            before = pointer.pixel(root, 140, 15)
            tick = time.monotonic()
            pointer.button(True)
            pointer.button(False)
            while time.monotonic() - tick < 10:
                if pointer.pixel(root, 140, 15) != before:
                    break
                time.sleep(.01)
            latencies.append(time.monotonic() - tick)
            time.sleep(.4)

    try:
        bridge.request("start", 1)
        bridge.wait(lambda states: len(states) == 4 and all(s["frames"] > 5 for s in states))
        bridge.request("view", 1, diagnostics=True)
        phase = "diagnostics-untouched"
        time.sleep(12)
        root = pointer.window(1)
        bridge.request("view", 1, rig_manager=True)
        phase = "rig-manager-and-diagnostics"
        time.sleep(8)
        subprocess.run(["xwininfo", "-name", "HejjéTrac · Rig manager"], check=True, stdout=subprocess.DEVNULL)
        manager = pointer.named_window("HejjéTrac · Rig manager")
        pointer.minimize(manager)
        phase = "minimized-rig-manager"
        time.sleep(8)
        assert "Iconic" in subprocess.check_output(["xprop", "-id", hex(manager), "WM_STATE"], text=True), "Rig manager did not minimize"
        if args.minimize_only:
            phase = "minimized-manager-native-input"
            input_probe()
            assert max(latencies) < args.max_input_ms / 1000, "Minimizing rig manager stalled native input"
            report["passed"] = True
            return
        pointer.restore(manager)
        phase = "restored-rig-manager"
        time.sleep(3)
        pointer.minimize(pointer.named_window("HejjéTrac · Rig manager"))
        bridge.request("view", 1, settings=True)
        phase = "settings"
        time.sleep(8)
        bridge.request("view", 1, settings=False)
        for n in range(1, 5):
            if args.video and bridge.request("status", n)["ended"]:
                bridge.request("rewind", n)
                bridge.request("start", n)
            before = bridge.request("config", n)["text"]
            bridge.request("view", n, calibration=True, calibration_step="ball",
                           calibration_action="edit", full_image=True)
            phase = f"calibration-{n}-before-hover"
            time.sleep(2)
            window = pointer.window(n)
            pointer.raise_window(window)
            pointer.move(window, 300, 250)
            phase = f"calibration-{n}-hover"
            time.sleep(8)
            if args.drag:
                state = json.loads((output / "state.json").read_text())
                ui = next(s["ui"] for s in state["instances"] if s["id"] == n)
                left, top, width, height = ui["source_rect"]
                source_width, source_height = ui["source_dimensions"]
                fields = dict((line.split(":", 1)[0].strip(), line.split(":", 1)[1].strip())
                              for line in before.splitlines() if ":" in line and not line.lstrip().startswith("#"))
                marks = json.loads(fields["roi_circ"].replace("{", "[").replace("}", "]"))
                x, y = left + marks[0] * width / source_width, top + marks[1] * height / source_height
                pointer.move(window, x, y)
                time.sleep(.15)
                pointer.button(True)
                time.sleep(.15)
                pointer.move(window, x + 25, y + 9)
                time.sleep(.3)
                pointer.button(False)
                time.sleep(.15)
                after = bridge.request("config", n)["text"]
                moved = dict((line.split(":", 1)[0].strip(), line.split(":", 1)[1].strip())
                             for line in after.splitlines() if ":" in line and not line.lstrip().startswith("#"))
                actual = json.loads(moved["roi_circ"].replace("{", "[").replace("}", "]"))
                expected = [marks[0] + 25 * source_width / width, marks[1] + 9 * source_height / height]
                drags.append({"session": n, "expected_first_mark": expected, "actual_first_mark": actual[:2]})
                assert all(abs(a - b) < .8 for a, b in zip(actual[:2], expected)), "Native rim drag did not follow the pointer"
                assert moved["vfov"] == fields["vfov"], "Rim drag changed FOV"
                bridge.request("replace_config", n, text=before)
            title = bridge.request("status", n)["window_title"]
            report["captures"].append(capture_window(output, f"calibration-{n}", title))
            bridge.request("view", n, calibration_action="finish")
            bridge.request("view", n, calibration=False, settings=False)
            assert bridge.request("config", n)["text"] == before
        # All camera windows cover the rig manager, reproducing the old 1 Hz stall.
        for n in range(1, 5):
            pointer.raise_window(pointer.window(n))
        phase = "after-calibrations-covered-manager"
        time.sleep(12)
        phase = "native-input-probe"
        input_probe()
        bridge.request("view", 4, window_size=[1200, 850], diagnostics=True)
        phase = "large-resized-window"
        time.sleep(8)
        report["captures"].append(capture_window(output, "resized", bridge.request("status", 4)["window_title"]))
        if args.close_root:
            bridge.request("close", 1, discard=True)
            phase = "hidden-root-with-three-live-windows"
            time.sleep(8)
        report["native_input_seconds"] = latencies
        report["native_drags"] = drags
        report["maximum_ui_interval_ms"] = max(s["ui"]["maximum_interval_ms"] for r in samples for s in r["instances"])
        report["maximum_native_input_seconds"] = max(latencies)
        assert max(latencies) < args.max_input_ms / 1000, "Pointer-to-painted-button latency exceeds the UI budget"
        assert report["maximum_ui_interval_ms"] < 500, "Native windows stalled for at least half a second"
        report["passed"] = True
    except Exception as error:
        report["failure"] = str(error)
        raise
    finally:
        pointer.close()
        stop.set()
        thread.join()
        bridge.finish(range(1, 5))
        report["native_input_seconds"] = latencies
        report["maximum_native_input_seconds"] = max(latencies, default=None)
        report["maximum_ui_interval_ms"] = max((s["ui"]["maximum_interval_ms"] for r in samples for s in r["instances"]), default=None)
        report["original_configs_unchanged"] = all(hashlib.sha256(Path(p).read_bytes()).hexdigest() == digest for p, digest in originals.items())
        phases = sorted({r["phase"] for r in samples})
        report["phases"] = {name: {"samples": sum(r["phase"] == name for r in samples),
            "maximum_root_update_ms": max(r["root_update_ms"] for r in samples if r["phase"] == name)} for name in phases}
        report["samples"] = len(samples)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "report": str(output / "report.json"),
                      "maximum_native_input_seconds": max(latencies)}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--configs", type=Path, default=Path("."))
    parser.add_argument("--binary", type=Path, default=Path("target/release/hejjetrac"))
    parser.add_argument("--max-input-ms", type=float, default=250)
    parser.add_argument("--drag", action="store_true")
    parser.add_argument("--close-root", action="store_true")
    parser.add_argument("--minimize-only", action="store_true")
    parser.add_argument("--video", type=Path)
    run(parser.parse_args())
