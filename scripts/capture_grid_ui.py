#!/usr/bin/env python3
"""Development-only native grid capture; FFmpeg is never an app dependency.

Copies saved replay fixtures, disables experiment outputs and records only the
window owned by this test process. This demonstration is separate from input
latency and complete-video numerical measurements.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from test_daily_ui_visual import capture_window
from test_native_rigs import NativeBridge, field
from test_native_ui_interaction import Pointer


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    configs = output / 'configs'
    configs.mkdir()
    os.environ['HEJJETRAC_STATE_DIR'] = str(output / 'app-state')
    os.environ['XDG_CONFIG_HOME'] = str(output / 'config-home')
    for n in range(1, 17):
        text = (args.fixture / f'VR{n}.txt').read_text()
        for key, value in [('sock_port', '0'), ('zmq_port', '0'), ('com_port', ''),
                           ('save_raw', 'n'), ('save_debug', 'n')]:
            text = field(text, key, value)
        (configs / f'VR{n}.txt').write_text(text)
    command = ['nice', '-n', '10', str(args.binary.resolve()), '--config',
               str(configs / 'VR1.txt'), '--array', '--control-stdio',
               '--state-output', str(output / 'state.json')]
    bridge = NativeBridge(command, output)
    pointer = None
    try:
        bridge.request('start', 1)
        bridge.wait(lambda s: len(s) == 16 and all(x['frames'] > 10 for x in s), timeout=60)
        for n in range(1, 17):
            bridge.request('view', n, rig_manager=False, full_image=n % 2 == 0,
                           diagnostics=n % 4 in [0, 1], overlays=n % 3 == 0)
        bridge.request('arrange_grid')
        time.sleep(1)
        pointer = Pointer(bridge)
        window = pointer.window(1)
        pointer.raise_window(window)
        title = bridge.request('status', 1)['host_window_title']
        still = capture_window(output, 'sixteen-native-grid', title)
        before = bridge.request('sessions')
        video = output / 'sixteen-native-grid.mp4'
        with (output / 'capture.log').open('w') as log:
            subprocess.run(['ffmpeg', '-nostdin', '-y', '-f', 'x11grab', '-framerate', '15',
                            '-window_id', str(window), '-i', os.environ.get('DISPLAY', ':0'),
                            '-t', '4', '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '24',
                            '-vf', 'pad=ceil(iw/2)*2:ceil(ih/2)*2', '-pix_fmt', 'yuv420p',
                            str(video)], stdout=log, stderr=subprocess.STDOUT, check=True, timeout=30)
        after = bridge.request('sessions')
        state = json.loads((output / 'state.json').read_text())
        if state['native_viewport_count'] != 1 or any(s['native_window'] for s in after):
            raise RuntimeError('Capture unexpectedly opened multiple native surfaces')
        deltas = [b['frames']-a['frames'] for a, b in zip(before, after)]
        if min(deltas) < 1:
            raise RuntimeError('A rig did not advance during native capture')
        report = {'scope': __doc__, 'passed': True, 'native_viewport_count': 1,
                  'window_id': hex(window), 'source_frame_deltas_during_capture': deltas,
                  'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                  'video_sha256': hashlib.sha256(video.read_bytes()).hexdigest(), 'still': still}
        draft = (configs / 'VR1.txt').read_text()+'\n# retained native-close test note\n'
        bridge.request('replace_config',1,text=draft)
        if not bridge.request('status',1)['dirty']:
            raise RuntimeError('Unsaved-close fixture is not dirty')
        pointer.request_close(window)
        time.sleep(.5)
        remaining = bridge.request('sessions')
        if len(remaining)!=16 or not remaining[0]['dirty']:
            raise RuntimeError('Native whole-grid close discarded unsaved text or rigs')
        report['unsaved_native_close'] = {'rigs_retained':len(remaining),'draft_retained':True,
            'capture':capture_window(output,'unsaved-grid-close',title)}
        (output / 'report.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps(report))
    finally:
        if pointer:
            pointer.close()
        bridge.finish(range(1, 17))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/fictrac-desktop'))
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
