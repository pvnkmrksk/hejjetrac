#!/usr/bin/env python3
"""Native array failure isolation, binding and default-video recording checks.

Development only; opens its own native windows, uses copied configs/isolated
outputs, and closes only its own process. --hardware opens two connected Linux
USB cameras using explicit socket bindings; it never writes camera firmware.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

from test_native_rigs import NativeBridge, field
from test_daily_ui_visual import capture_window


def socket_ids():
    result = []
    for video in sorted(Path('/sys/class/video4linux').glob('*')):
        if (video / 'index').read_text().strip() != '0':
            continue
        resolved = video.resolve()
        usb = next((p for p in resolved.parents if (p / 'idVendor').exists()), None)
        if usb is None:
            continue
        controller = next(p.parent.name for p in usb.parents if p.name.startswith('usb') and p.name[3:].isdigit())
        ports = usb.name.split('-', 1)[1]
        result.append(f'usb-port:{controller}:{ports}:video:0')
    if len(result) != 2 or len(set(result)) != 2:
        raise RuntimeError(f'expected two distinguishable physical USB sockets, got {result}')
    return result


def run(args):
    output = args.output.resolve()
    if output.exists():
        output = output / f'rerun-{time.time_ns()}'
    output.mkdir(parents=True, exist_ok=False)
    template = args.config.read_text()
    sockets = socket_ids() if args.hardware else []
    for slot in [1, 2, 4]:
        text = template
        settings = [('rust_instance_name', f'VR{slot}'), ('save_raw', 'n'),
                    ('save_debug', 'n'), ('vid_codec', 'raw'), ('sock_port', '0'),
                    ('zmq_port', '0'), ('com_port', '')]
        if slot == 1:
            settings += [('src_fn', '99999'), ('rust_camera_id', 'usb:ffff:ffff:missing:video:0')]
        elif args.hardware:
            settings += [('src_fn', str(99990 + slot)), ('rust_camera_id', sockets[0 if slot == 2 else 1]),
                         ('rust_camera_width', '320'), ('rust_camera_height', '240'), ('src_fps', '100')]
        else:
            settings += [('src_fn', str(args.video.resolve())), ('rust_camera_id', ''), ('src_fps', '30')]
        for key, value in settings:
            text = field(text, key, value)
        (output / f'VR{slot}.txt').write_text(text)
    (output / 'VR3.txt').write_text('malformed manual configuration\n')
    bridge = NativeBridge([str(args.binary.resolve()), '--config', str(output / 'VR1.txt'),
                           '--array', '--control-stdio', '--window-size', '480x360'], output)
    report = {'passed': False, 'hardware': args.hardware,
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'captures': []}

    def wait(predicate, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            states = bridge.request('sessions')
            healthy = [s for s in states if s['session'] in [2, 4]]
            if any(s['ended'] or s['error'] or s['command_error'] for s in healthy):
                raise RuntimeError(f'healthy rig failed: {healthy}')
            if predicate(states):
                return states
            time.sleep(.1)
        raise RuntimeError(f'array state did not converge: {states}')

    try:
        discovery = bridge.request('discover')
        if 'start_array' not in discovery['commands'] or 'camera_list' not in discovery['commands']:
            raise RuntimeError('array/camera operations are missing from discovery')
        states = wait(lambda ss: len(ss) == 4 and all(ss[i]['frames'] >= 5 for i in [1, 3]))
        for i in [0, 2]:
            if states[i]['launch_state'] != 'Did not start' or states[i]['worker_active'] or not states[i]['error']:
                raise RuntimeError(f'failed slot was not clearly reported: {states[i]}')
        if any(s['sample_fallback_active'] for s in states):
            raise RuntimeError('array substituted a stock video')
        report['initial'] = states
        report['inventory'] = bridge.request('camera_list')
        if args.hardware:
            for i, binding in [(1, sockets[0]), (3, sockets[1])]:
                if states[i]['running_camera_binding'] != binding or '/dev/video' not in states[i]['source_label']:
                    raise RuntimeError('stale numeric source did not resolve through the saved USB binding')
        report['captures'].append(capture_window(output, 'array-results', 'HejjéTrac · Rig manager'))
        bridge.request('view', 2, settings=True)
        report['captures'].append(capture_window(output, 'settings', 'HejjéTrac · VR2 configuration'))
        invalid = output / 'invalid-empty-selection'
        report['invalid_record'] = bridge.request('record_start', 2, expect_error=True,
            directory=str(invalid), record_options={'raw': False, 'debug': False})
        if invalid.exists():
            raise RuntimeError('invalid recording created output')
        before = bridge.request('config', 2)['text']
        report['invalid_edit'] = bridge.request('set', 2, expect_error=True, key='src_fps', value='NaN')
        if bridge.request('config', 2)['text'] != before:
            raise RuntimeError('invalid manual edit changed the configuration')
        bridge.request('pause', 2)
        wait(lambda ss: ss[1]['paused'])
        take = output / 'default-raw-take'
        bridge.request('record_start', 2, directory=str(take))
        states = wait(lambda ss: ss[1]['recording'])
        target = states[1]['frames'] + 5
        bridge.request('run', 2, frames=5)
        wait(lambda ss: ss[1]['frames'] == target and ss[1]['advance_remaining'] == 0)
        bridge.request('record_stop', 2)
        def finalized_media(states):
            for entry in states[1]['recording_finalizations']:
                if entry['directory'] == str(take) and entry['state'] == 'complete':
                    if entry['completion_error'] or entry['numeric_error'] or entry['media_warnings']:
                        raise RuntimeError(f'background finalization failed: {entry}')
                    return entry['media_recording']
            media = states[1]['media_recording']
            return media if media and media['raw']['state'] == 'complete' else None
        states = wait(lambda ss: not ss[1]['recording'] and finalized_media(ss) is not None)
        media = finalized_media(states)
        if not media['raw']['requested'] or media['debug']['requested'] or media['raw']['encoded_frames'] != 5:
            raise RuntimeError(f'default recording did not retain five raw frames: {media}')
        inspected = subprocess.run([str(args.cli.resolve()), 'inspect', str(take / 'epoch-0000-media/raw.avi'), '--decode'],
                                   capture_output=True, text=True, check=True, timeout=30)
        if 'decoded_frames=5 ' not in inspected.stdout:
            raise RuntimeError('Rust decoder did not verify all five recorded images')
        report['recorded_video_decoder'] = inspected.stdout
        report['finalized_media'] = media
        report['final'] = states
        report['passed'] = True
    finally:
        bridge.finish([1, 2, 3, 4])
        report['exit_code'] = bridge.process.returncode
        (output / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({'passed': report['passed'], 'report': str(output / 'report.json')}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/debug/hejjetrac'))
    parser.add_argument('--cli', type=Path, default=Path('target/debug/hejjetrac-cli'))
    parser.add_argument('--config', type=Path, default=Path('examples/benchmark/config.txt'))
    parser.add_argument('--video', type=Path, default=Path('examples/sample.mp4'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--hardware', action='store_true')
    run(parser.parse_args())
