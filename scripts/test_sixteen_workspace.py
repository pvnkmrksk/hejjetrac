#!/usr/bin/env python3
"""Native shared-surface rig-grid stress, numeric replay and graphical restore.

Development harness only. Uses independent decoders/trackers of a real
video, not physical cameras. Large arrays must use one hardware GPU surface. Loopback accepted numeric rows must match
the retained complete-video baseline byte for byte. No original rig is edited.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import threading
import time

from test_daily_ui_visual import capture_window
from test_live_four import process_resources
from test_native_rigs import NativeBridge, field
from test_native_ui_interaction import Pointer


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compact(states):
    keys = ['session', 'frames', 'window_rect', 'window_inner_size', 'view',
            'tile_rect', 'tile_open_rect', 'native_window', 'preview_canvas_rect', 'preview_image_rect', 'actual_fps', 'tracking_ms',
            'preview_fps', 'error', 'notice', 'source_label', 'last_row']
    return [{key: s.get(key) for key in keys} for s in states]


def geometry(states, monitor):
    boxes, fills = [], []
    for state in states:
        x, y, w, h = state['tile_rect']
        if min(x, y) < -1 or x + w > monitor[0] + 1 or y + h > monitor[1] + 1:
            raise RuntimeError(f"Window is outside the desktop: {state['session']} {[x, y, w, h]}")
        for a, b, c, d in boxes:
            if min(x + w, a + c) - max(x, a) > 1 and min(y + h, b + d) - max(y, b) > 1:
                raise RuntimeError('Tracking windows overlap after grid arrangement')
        boxes.append([x, y, w, h])
        canvas, image = state['preview_canvas_rect'], state['preview_image_rect']
        fills.append(image[2] * image[3] / (canvas[2] * canvas[3]))
    if min(fills) < .97:
        raise RuntimeError(f"Grid leaves image pillarboxing: {fills}")
    return {'visible_tiles': len(boxes), 'minimum_image_fill_fraction': min(fills),
            'outer_rectangles': boxes}


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    os.environ['HEJJETRAC_STATE_DIR'] = str(output / 'app-state')
    os.environ['XDG_CONFIG_HOME'] = str(output / 'config-home')
    original = args.config.read_bytes()
    sockets, streams = [], []
    stop = threading.Event()
    failures = []
    for n in range(1, args.sessions + 1):
        receiver = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        receiver.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4 * 1024 * 1024)
        receiver.bind(('127.0.0.1', 0))
        receiver.setblocking(False)
        sockets.append(receiver)
        streams.append((output / f'VR{n}-accepted.dat').open('wb'))
        text = original.decode()
        for key, value in [('src_fn', str(args.video.resolve())), ('src_fps', '-1'),
                           ('rust_camera_id', ''), ('rust_resilient', 'y'),
                           ('rust_instance_name', f'Grid16_{n:02d}'), ('save_raw', 'n'),
                           ('save_debug', 'n'), ('zmq_port', '0'), ('com_port', ''),
                           ('sock_host', '127.0.0.1'), ('sock_port', str(receiver.getsockname()[1]))]:
            text = field(text, key, value)
        (output / f'VR{n}.txt').write_text(text)

    def receive():
        try:
            while not stop.is_set():
                ready, _, _ = select.select(sockets, [], [], .05)
                for receiver in ready:
                    while True:
                        try:
                            packet = receiver.recv(8192)
                        except BlockingIOError:
                            break
                        if not packet.startswith(b'FT, '):
                            raise RuntimeError('Unexpected native numeric packet')
                        streams[sockets.index(receiver)].write(packet[4:])
        except Exception as error:
            failures.append(str(error))

    thread = threading.Thread(target=receive)
    thread.start()
    command = ['nice', '-n', '10', str(args.binary.resolve()), '--config', str(output / 'VR1.txt'),
               '--array', '--control-stdio',
               '--state-output', str(output / 'state.json')]
    bridge = NativeBridge(command, output)
    pointer = None
    report = {'passed': False, 'binary_sha256': sha(args.binary),
              'input_sha256': sha(args.video), 'reference_sha256': sha(args.reference),
              'command': command, 'scope': __doc__, 'captures': [], 'samples': []}
    try:
        # The development bridge starts its first worker paused; ordinary
        # graphical launches do not. Resume without resetting its source epoch.
        bridge.request('start', 1)
        states = bridge.wait(lambda s: len(s) == args.sessions and all(x['frames'] > 10 for x in s), timeout=60)
        bridge.request('view', 1, rig_manager=False)
        pointer = Pointer(bridge)
        # Mixed saved view modes exercise both crop aspect and diagnostic aspect.
        expected_views = {}
        for n in range(1, args.sessions + 1):
            view = {'full_image': n % 2 == 0, 'diagnostics': n % 4 in [0, 1],
                    'overlays': n % 3 == 0, 'display_fps': 30}
            bridge.request('view', n, rig_manager=False, **view)
            expected_views[n] = view
        bridge.request('arrange_grid', columns=4)
        time.sleep(2)
        states = bridge.request('sessions')
        def settled():
            time.sleep(.7)
            states = bridge.request('sessions')
            if any(s['native_window'] or not s['tile_rect'] for s in states):
                raise RuntimeError('Large arrays must render inside one shared native surface')
            state = json.loads((output / 'state.json').read_text())
            if state['native_viewport_count'] != 1:
                raise RuntimeError(f"Grid opened {state['native_viewport_count']} native GPU surfaces")
            return states

        import re
        root_geometry = subprocess.check_output(['xwininfo','-root'],text=True)
        monitor = [int(re.search(r'Width:\s+(\d+)',root_geometry)[1]),int(re.search(r'Height:\s+(\d+)',root_geometry)[1])]
        states = settled()
        report['forced_4x4'] = geometry(states, monitor)
        bridge.request('arrange_grid')
        states = settled()
        report['automatic_grid'] = geometry(states, monitor)
        report['grid_states'] = compact(states)
        report['native_viewport_count'] = 1
        host_title = states[0]['host_window_title']
        window = pointer.window(1)
        pointer.raise_window(window)
        report['captures'].append(capture_window(output, 'shared-rig-grid', host_title))
        # Real native input expands and collapses a tile while every worker
        # continues. Read painted pixels; background processing FPS alone cannot
        # establish responsiveness.
        latencies = []
        for _ in range(6):
            state = bridge.request('status',1)
            x,y,w,h = state['tile_open_rect']
            pointer.move(window,x+w*.5,y+h*.5)
            time.sleep(.1)
            px,py = round(x+3),round(y+h*.5)
            before = pointer.pixel(window,px,py)
            tick = time.monotonic()
            pointer.button(True); pointer.button(False)
            while pointer.pixel(window,px,py) == before and time.monotonic()-tick < 2:
                time.sleep(.005)
            latencies.append(time.monotonic()-tick)
            time.sleep(.15)
        report['native_input_seconds'] = latencies
        if max(latencies) > .25:
            raise RuntimeError(f'Grid native input exceeded 250ms: {latencies}')
        states = settled()
        # Settings are embedded too: creating inspectors cannot recreate a
        # large collection of native presentation surfaces.
        bridge.request('view',1,settings=True,settings_section='recording')
        settled()
        report['captures'].append(capture_window(output,'grid-recording-settings',host_title))
        bridge.request('view',1,settings_section='advanced')
        settled()
        report['captures'].append(capture_window(output,'grid-advanced-settings',host_title))
        bridge.request('view',1,settings=False)
        settled()
        started = time.monotonic()
        with (output / 'observations.jsonl').open('w') as log:
            while True:
                states = bridge.request('sessions')
                sample = {'elapsed_seconds': time.monotonic() - started,
                          'process': process_resources(bridge.process.pid),
                          'sessions': compact(states)}
                available = int(next(line.split()[1] for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:')))
                if sample['process']['rss_kib'] > 4*1024*1024 or available < 2*1024*1024:
                    raise RuntimeError('Safety stop: app exceeds 4GiB or less than 2GiB system memory remains')
                log.write(json.dumps(sample) + '\n')
                log.flush()
                report['samples'].append(sample)
                if any(s['error'] or s['command_error'] for s in states):
                    raise RuntimeError('A native session reported an error')
                if args.smoke_seconds and time.monotonic()-started >= args.smoke_seconds:
                    break
                if all(s['ended'] for s in states):
                    break
                if time.monotonic() - started > 180:
                    raise RuntimeError('Sixteen real-video sessions did not finish within 180 seconds')
                time.sleep(.5)
        report['final'] = compact(states)
        if not args.smoke_seconds and any(s['frames'] != 5157 for s in states):
            raise RuntimeError('A native worker omitted real-video source images')
        workspace = bridge.request('workspace_save', restore_on_launch=True)
        report['workspace'] = workspace
        # Freeze the complete accepted streams before testing a new playback
        # epoch. Closing the first rig must release its files/output socket,
        # while the shared root surface and other rigs remain usable.
        stop.set()
        thread.join(timeout=2)
        if not args.smoke_seconds:
            resources_before = process_resources(bridge.process.pid)
            bridge.request('close',1,discard=True)
            time.sleep(.7)
            remaining = bridge.request('sessions')
            if len(remaining) != args.sessions-1 or any(s['session']==1 for s in remaining):
                raise RuntimeError('Closing the first grid rig closed or retained the wrong sessions')
            resources_after = process_resources(bridge.process.pid)
            if resources_after['open_fds'] >= resources_before['open_fds']:
                raise RuntimeError('Closed first rig retained its source/output file descriptors')
            bridge.request('start',2)
            deadline = time.monotonic()+15
            while True:
                peer = bridge.request('status',2)
                if peer['frames'] > 10 and not peer['ended']:
                    break
                if time.monotonic()>deadline:
                    raise RuntimeError('Another rig did not restart after the first was closed')
                time.sleep(.05)
            before = peer['frames']
            time.sleep(.5)
            peer = bridge.request('status',2)
            if peer['frames'] <= before:
                raise RuntimeError('Other grid source stopped advancing after closing the first rig')
            report['closed_first_rig'] = {'rigs_remaining':len(remaining),
                'resources_before':resources_before,'resources_after':resources_after,
                'continuing_rig':2,'continuing_rig_frame_delta':peer['frames']-before}
            report['captures'].append(capture_window(output,'grid-after-first-rig-closed',peer['host_window_title']))
        bridge.finish(range(1, args.sessions + 1))
        pointer.close()
        pointer = None
        time.sleep(.2)
        stop.set()
        thread.join(timeout=2)
        for stream in streams:
            stream.flush()
        if failures:
            raise RuntimeError(failures[0])
        expected = args.reference.read_bytes()
        if not args.smoke_seconds:
            report['numeric_streams'] = []
            for n in range(1, args.sessions + 1):
                actual = (output / f'VR{n}-accepted.dat').read_bytes()
                if actual != expected:
                    raise RuntimeError(f'VR{n} accepted numeric data differs from the retained VR3 baseline')
                report['numeric_streams'].append({'session': n, 'rows': len(actual.splitlines()),
                                                   'sha256': hashlib.sha256(actual).hexdigest()})
            # Exercise the same no-config graphical entry point as the app launcher.
            restore_command = ['nice', '-n', '10', str(args.binary.resolve()), '--exit-after', '10',
                               '--state-output', str(output / 'restored-state.json'),
                               '--screenshot', str(output / 'restored-root.png'), '--screenshot-after', '4']
            with (output / 'restore.stdout').open('w') as stdout, (output / 'restore.stderr').open('w') as stderr:
                subprocess.run(restore_command, cwd=output, stdout=stdout, stderr=stderr, check=True, timeout=45)
            restored = json.loads((output / 'restored-state.json').read_text())
            if len(restored['instances']) != args.sessions:
                raise RuntimeError('Graphical launch did not restore all sixteen saved rigs')
            for state in restored['instances']:
                if state['error'] or state['frames'] < 20 or state['view'] != {**expected_views[state['id']], 'reuse_atlas': False}:
                    raise RuntimeError(f'Graphical launch did not restore a view/source: {state}')
            if restored['native_viewport_count'] != 1:
                raise RuntimeError('Graphical workspace restore recreated multiple GPU surfaces')
            restored_boxes = []
            for state in restored['instances']:
                x, y, w, h = state['tile_rect']
                if min(x, y) < 0 or x + w > monitor[0] + 1 or y + h > monitor[1] + 1:
                    raise RuntimeError('Graphical workspace restoration put a rig outside the desktop')
                if any(min(x + w, a + c) - max(x, a) > 1 and min(y + h, b + d) - max(y, b) > 1
                       for a, b, c, d in restored_boxes):
                    raise RuntimeError('Graphical workspace restoration overlapped tracking windows')
                restored_boxes.append([x, y, w, h])
            report['graphical_restore'] = {'command': restore_command, 'sessions': args.sessions,
                'minimum_frames': min(s['frames'] for s in restored['instances']), 'all_views_match': True,
                'all_windows_visible_without_overlap': True, 'outer_rectangles': restored_boxes}
        if args.config.read_bytes() != original:
            raise RuntimeError('Original fixture configuration was changed')
        report['passed'] = True
    finally:
        stop.set()
        thread.join(timeout=2)
        if pointer:
            pointer.close()
        bridge.finish(range(1, args.sessions + 1))
        for stream in streams:
            stream.close()
        for receiver in sockets:
            receiver.close()
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'passed': report['passed'], 'maximum_native_input_ms': max(report['native_input_seconds']) * 1000,
                      'visible_tiles': report['automatic_grid']['visible_tiles'], 'graphical_restore': report.get('graphical_restore')}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/fictrac-desktop'))
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--video', type=Path, required=True)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--sessions',type=int,choices=range(5,17),default=16)
    parser.add_argument('--smoke-seconds',type=float,default=0,help='Bounded UI smoke only; skips numeric parity and restart checks')
    run(parser.parse_args())
