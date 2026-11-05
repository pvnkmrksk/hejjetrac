#!/usr/bin/env python3
"""Native desktop MCP regression: no --control-stdio, real video and saved outputs.

Runs isolated configs/state, authenticates over loopback, drives real egui input,
checks a 4x4 grid, shared views/recording, calibration completion and restoration.
Requires a native display. This is a development client, not an app dependency.
"""
import argparse
import base64
import csv
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.error
import urllib.request

from test_native_rigs import field


class Client:
    def __init__(self, binary, output, args):
        self.state = output / 'state'
        self.state.mkdir(exist_ok=True)
        env = dict(os.environ, HEJJETRAC_STATE_DIR=str(self.state),
                   XDG_CONFIG_HOME=str(output / 'config-home'))
        self.log = (output / f'process-{len(list(output.glob("process-*.log")))}.log').open('w')
        self.process = subprocess.Popen([str(binary), *args], env=env, stdin=subprocess.DEVNULL,
                                        stdout=self.log, stderr=self.log, cwd=output)
        self.counter = 0
        self.file = None
        self.connection = None
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            for path in (self.state / 'mcp').glob('*.json'):
                try:
                    connection = json.loads(path.read_text())['connection']
                    self.connection = connection
                    self.rpc('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                                           'clientInfo': {'name': 'desktop-regression', 'version': '1'}})
                    self.file = path
                    return
                except (OSError, ValueError, urllib.error.URLError):
                    pass
            if self.process.poll() is not None:
                raise RuntimeError(f'desktop exited {self.process.returncode}; see {self.log.name}')
            time.sleep(.1)
        raise RuntimeError('auto-started MCP endpoint was not discoverable')

    def rpc(self, method, params=None, headers=None):
        self.counter += 1
        body = json.dumps({'jsonrpc': '2.0', 'id': self.counter, 'method': method,
                           'params': params or {}}).encode()
        request = urllib.request.Request(self.connection['url'], body,
            {'Content-Type': 'application/json', 'Accept': 'application/json, text/event-stream',
             'MCP-Protocol-Version': '2025-11-25',
             **self.connection['headers'], **(headers or {})})
        with urllib.request.urlopen(request, timeout=15) as response:
            result = json.load(response)
        if 'error' in result:
            raise RuntimeError(result['error'])
        return result['result']

    def tool(self, name, **args):
        value = self.rpc('tools/call', {'name': name, 'arguments': args})
        if value.get('isError'):
            raise RuntimeError(value)
        return value

    def command(self, command, session=None, **args):
        request = {'command': command, 'args': args}
        if session is not None:
            request['session'] = session
        response = self.tool('hejjetrac_execute', **request)['structuredContent']
        if not response['ok']:
            raise RuntimeError(response['error'])
        return response['result']

    def wait(self, predicate, timeout=25):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            state = self.command('sessions')
            if predicate(state):
                return state
            time.sleep(.05)
        raise RuntimeError('completion timeout: ' + repr([{k:s.get(k) for k in ('session','frames','paused','tile_rect','error','command_error','view')} for s in state]))

    def screenshot(self, path, surface='root'):
        result = self.tool('hejjetrac_screenshot', surface=surface)
        image = next(item for item in result['content'] if item['type'] == 'image')
        path.write_bytes(base64.b64decode(image['data'], validate=True))
        assert path.read_bytes().startswith(b'\x89PNG\r\n\x1a\n')

    def finish(self):
        try:
            for s in self.command('sessions'):
                self.command('close', s['session'], discard=True)
            self.process.wait(timeout=20)
            assert self.process.returncode == 0
            assert not self.file.exists(), 'clean shutdown left a credential discovery file'
        finally:
            if self.process.poll() is None:
                self.process.terminate()
                self.process.wait(timeout=20)
            self.log.close()


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    root = Path(__file__).resolve().parents[1]
    text = (root / 'examples/upstream-config.txt').read_text()
    for key, value in [('src_fn', str(root / 'examples/sample.mp4')), ('sock_port', '0'),
                       ('zmq_port', '0'), ('save_raw', 'n'), ('save_debug', 'n'),
                       ('com_port', ''), ('vid_codec', 'raw')]:
        text = field(text, key, value)
    configs = output / 'configs'
    configs.mkdir()
    for number in range(1, 17):
        (configs / f'VR{number}.txt').write_text(text)
    client = Client(binary, output, ['--config', str(configs / 'VR1.txt'), '--array'])
    report = {'passed': False, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'video_sha256': hashlib.sha256((root / 'examples/sample.mp4').read_bytes()).hexdigest(),
              'platform': os.uname().sysname, 'launch': 'native --config --array without control-stdio'}
    try:
        tools = client.rpc('tools/list')['tools']
        assert {t['name'] for t in tools} == {'hejjetrac_execute', 'hejjetrac_surfaces',
                                             'hejjetrac_screenshot', 'hejjetrac_ui'}
        if os.name == 'posix':
            assert client.file.stat().st_mode & 0o077 == 0
        for headers, code in [({'Authorization': 'Bearer incorrect'}, 401),
                              ({'Origin': 'https://example.com'}, 403),
                              ({'MCP-Protocol-Version': 'unsupported'}, 400)]:
            try:
                client.rpc('ping', headers=headers)
                raise AssertionError('invalid transport request was accepted')
            except urllib.error.HTTPError as error:
                assert error.code == code
        sessions = client.wait(lambda s: len(s) == 16 and all(x['frames'] >= 3 for x in s), 60)
        for session in sessions:
            client.command('pause', session['session'])
        sessions = client.wait(lambda s: all(x['paused'] for x in s))
        for session in sessions:
            client.command('view', session['session'], rig_manager=False)
        time.sleep(.2)
        rects = [s['tile_rect'] for s in sessions]
        assert all(not s['native_window'] for s in sessions)
        assert len({round(r[0], 1) for r in rects}) == 4
        assert len({round(r[1], 1) for r in rects}) == 4
        client.screenshot(output / 'grid-4x4.png')
        report['grid'] = rects
        client.command('array_view', full_image=True, diagnostics=True, overlays=True)
        shared = client.command('sessions')
        assert all(s['view']['full_image'] and s['view']['diagnostics'] and s['view']['overlays'] for s in shared)
        client.command('array_view', full_image=False, diagnostics=False, overlays=False)
        # Exercise actual pointer events on the second rig, rather than rig_focus.
        rect = client.command('status', 2)['tile_rect']
        x, y = rect[0] + rect[2] / 2, rect[1] + rect[3] / 2
        for _ in range(2):
            client.tool('hejjetrac_ui', action='click', x=x, y=y)
            time.sleep(.07)
        expanded = client.wait(lambda s: s[1]['tile_rect'] and s[1]['tile_rect'][2] > rect[2] * 2)
        client.command('view', 2, full_image=True, diagnostics=True)
        client.screenshot(output / 'individual-debug.png')
        back = client.command('status', 2)['tile_open_rect']
        client.tool('hejjetrac_ui', action='click', x=back[0]+back[2]/2, y=back[1]+back[3]/2)
        grid = client.wait(lambda s: all(x['tile_rect'] and x['tile_rect'][2] < expanded[1]['tile_rect'][2] / 2 for x in s))
        assert grid[1]['view']['full_image'] and grid[1]['view']['diagnostics']
        assert not grid[0]['view']['diagnostics']
        # A single recording command must create verified data/media for every rig.
        client.command('array_record', start=True, directory=str(output / 'record-all'))
        client.wait(lambda s: all(x['recording'] for x in s))
        before = client.command('sessions')
        for session in before:
            client.command('step', session['session'], frames=3)
        client.wait(lambda s: all(x['advance_remaining'] == 0 and x['frames'] >= b['frames']+3 for x,b in zip(s,before)))
        client.command('array_record', start=False)
        client.wait(lambda s: all(not x['recording'] and all(t['state'] not in ('queued','finalizing') for t in x['recording_finalizations']) for x in s))
        rows = []
        for folder in sorted((output / 'record-all').glob('rig-*')):
            numeric = list(csv.reader(next(folder.glob('*-all-frames.dat')).open()))
            quality = list(csv.DictReader(next(folder.glob('*-quality.csv')).open()))
            assert len(numeric) == len(quality) == 3
            assert [int(r[0]) for r in numeric] == [int(q['frame']) for q in quality]
            media = next(folder.glob('*-media/raw.avi'))
            replay = output / f'replay-{folder.name}'
            subprocess.run([str(binary.with_name('hejjetrac-cli')), 'track', '--config',
                            str(configs / 'VR1.txt'), '--input', str(media), '--output', str(replay)],
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=30)
            assert len(list(csv.reader((replay / 'all-frames.dat').open()))) == 3
            rows.append({'rig': folder.name, 'rows': 3, 'media_bytes': media.stat().st_size})
        assert len(rows) == 16
        report['recordings'] = rows
        # Point-valued calibration start must coexist with array_record's boolean start.
        client.command('calibrate', 1, kind='forward', start=[100., 100.], end=[150., 120.])
        client.command('undo', 1)
        # Finish writes the existing rig once and closes its calibration inspector.
        client.command('rig_focus', 1)
        client.command('view', 1, calibration=True, calibration_step='ball', calibration_action='inspect')
        client.command('set', 1, key='thr_ratio', value='1.26')
        client.command('view', 1, calibration_action='keep')
        client.command('view', 1, calibration_action='keep')
        client.command('view', 1, calibration_action='keep')
        done = client.command('status', 1)
        assert not done['view']['calibration'] and not done['view']['settings'] and not done['dirty']
        assert '1.26' in (configs / 'VR1.txt').read_text()
        client.command('rig_grid')
        client.command('arrange_grid', columns=4, rows=4)
        client.command('workspace_save', restore_on_launch=True)
        client.screenshot(output / 'grid-custom-layout.png')
        # A second process must get a distinct port and independent credentials.
        second_out = output / 'second'
        second_out.mkdir()
        second = Client(binary, second_out, ['--config', str(configs / 'VR1.txt')])
        try:
            assert second.connection['url'] != client.connection['url']
            assert second.connection['headers'] != client.connection['headers']
            second.command('view', 1, window_size=[432, 321])
            second.wait(lambda s: all(abs(a-b) <= 1 for a,b in zip(s[0]['window_inner_size'], [432,321])))
            second.command('save_config', 1, path=str(output / 'saved-size.txt'))
        finally:
            second.finish()
        saved = Client(binary, second_out, ['--config', str(output / 'saved-size.txt')])
        try:
            saved.wait(lambda s: all(abs(a-b) <= 1 for a,b in zip(s[0]['window_inner_size'], [432,321])))
            report['restored_single_size'] = saved.command('status', 1)['window_inner_size']
        finally:
            saved.finish()
        client.command('view', 1, rig_manager=True)
        time.sleep(.4)
        client.screenshot(output / 'rig-manager.png')
        report['passed'] = True
    finally:
        client.finish()
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    restored = Client(binary, output, [])
    try:
        saved_workspace = restored.wait(lambda s: len(s) == 16 and all(x['tile_rect'] for x in s), 60)
        report['workspace_restored_rigs'] = len(saved_workspace)
        report['saved_layout'] = json.loads((output / 'state/workspace.json').read_text())
        assert report['saved_layout']['columns'] == 4 and report['saved_layout']['rows'] == 4
        restored.screenshot(output / 'restored-workspace.png')
    finally:
        restored.finish()
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'passed': True, 'rigs': 16, 'recordings_verified': 16,
                      'restored_single_size': report['restored_single_size']}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/hejjetrac'))
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
