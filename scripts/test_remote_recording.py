#!/usr/bin/env python3
"""Native four-rig remote recording stress using a non-loopback interface.

Uses real sample video and independently decodes every saved movie with Rust.
No physical-camera/hotplug or inter-computer clock accuracy claim is made.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import socket
import subprocess
import time
import urllib.error
import urllib.request

from test_desktop_mcp import Client
from test_native_rigs import field


def run(args):
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    configs = output / 'rigs'
    configs.mkdir()
    text = (root / 'examples/upstream-config.txt').read_text()
    for key, value in [('src_fn', str(root / 'examples/sample.mp4')),
                       ('save_raw', 'n'), ('save_debug', 'n'), ('sock_port', '0'),
                       ('zmq_port', '0'), ('com_port', ''), ('src_fps', '30'),
                       ('vid_codec', 'raw'), ('rust_camera_id', '')]:
        text = field(text, key, value)
    for rig in range(1, 5):
        (configs / f'VR{rig}.txt').write_text(field(text, 'rust_instance_name', f'VR{rig}'))
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as route:
        # Route selection only: no datagram is sent to this documentation address.
        route.connect(('192.0.2.1', 9))
        address = route.getsockname()[0]
    if address.startswith('127.'):
        raise RuntimeError('No non-loopback IPv4 interface is available')
    # Ignore development-machine HTTP proxies for this local LAN-interface test.
    urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
    client = Client(args.binary.resolve(), output,
                    ['--config', str(configs / 'VR1.txt'), '--array', '--control-bind', '0.0.0.0:0'])
    report = {'passed': False, 'scope': 'one Linux computer, non-loopback interface, real-video native four-rig recording',
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'physical_cameras': False, 'cycles': args.cycles, 'takes': [], 'authentication_checks': []}
    try:
        descriptor = json.loads(client.file.read_text())
        assert descriptor['remote_control'] is True
        port = descriptor['listen_address'].rsplit(':', 1)[1]
        client.connection['url'] = f'http://{address}:{port}/mcp'
        report['listen_address'] = descriptor['listen_address']
        report['tested_interface'] = address
        server_time = int(client.rpc('ping')['server_unix_ns'])
        assert abs(server_time - time.time_ns()) < 5_000_000_000
        for headers, expected in [({'Authorization': 'Bearer incorrect'}, 401),
                                  ({'Origin': 'https://example.com'}, 403),
                                  ({'Host': f'example.com:{port}'}, 403)]:
            try:
                client.rpc('ping', headers=headers)
                raise AssertionError('Invalid credentials/origin/host was accepted')
            except urllib.error.HTTPError as error:
                assert error.code == expected
                report['authentication_checks'].append(expected)
        client.wait(lambda ss: len(ss) == 4 and all(s['frames'] >= 3 for s in ss))
        for rig in range(1, 5):
            client.command('pause', rig)
        client.wait(lambda ss: all(s['paused'] for s in ss))
        client.command('arrange_grid', columns=2, rows=2)
        for cycle in range(args.cycles):
            before = client.command('sessions')
            for s in before:
                config = client.command('config', s['session'])['text']
                try:
                    client.command('set', s['session'], fields={'src_fn': '999999',
                        'rust_camera_id': 'usb:ffff:ffff:missing:video:0'})
                    raise AssertionError('Missing binding was accepted')
                except RuntimeError as error:
                    assert 'missing' in str(error)
                assert client.command('config', s['session'])['text'] == config
            assert [s['frames'] for s in client.command('sessions')] == [s['frames'] for s in before]
            take = output / f'trial-{cycle:03d}'
            metadata = {'experiment_id': 'remote-recording-stress', 'trial_id': f'trial-{cycle:03d}',
                        'trigger_id': f'start-{cycle:03d}', 'trigger_unix_ns': str(time.time_ns()),
                        'tags': {'scene': 'corridor', 'client': 'non-loopback'}}
            client.command('array_record', start=True, directory=str(take), recording_metadata=metadata)
            started = client.wait(lambda ss: all(s['recording'] for s in ss))
            try:
                client.command('array_record', start=True, directory=str(take))
                raise AssertionError('Duplicate Start was accepted')
            except RuntimeError as error:
                assert 'already recording' in str(error)
            targets = {s['session']: s['frames'] + 3 for s in started}
            for s in started:
                client.command('run', s['session'], frames=3)
            client.wait(lambda ss: all(s['frames'] == targets[s['session']] and s['advance_remaining'] == 0 for s in ss))
            client.command('array_record', start=False)
            def complete(ss):
                return all(not s['recording'] and any(
                    f['directory'] == str(take / f"rig-{s['session']}") and f['state'] == 'complete'
                    and not f['numeric_error'] and not f['completion_error'] and not f['media_warnings']
                    for f in s['recording_finalizations']) for s in ss)
            client.wait(complete)
            client.command('array_record', start=False)  # Idempotent Stop.
            for rig in range(1, 5):
                directory = take / f'rig-{rig}'
                manifest = json.loads((directory / 'recording.json').read_text())
                assert manifest['recording_metadata'] == metadata
                assert manifest['source_kind'] == 'recorded_video'
                with (directory / 'frame-times.csv').open() as stream:
                    rows = list(csv.DictReader(stream))
                numeric = (directory / 'epoch-0000-all-frames.dat').read_text().splitlines()
                quality = list(csv.DictReader((directory / 'epoch-0000-quality.csv').open()))
                assert len(rows) == len(numeric) == len(quality) == 3
                assert [int(r['recording_row']) for r in rows] == [0, 1, 2]
                assert [int(r['frame']) for r in rows] == [int(r['frame']) for r in quality]
                assert [r['accepted'] for r in rows] == [r['accepted'] for r in quality]
                assert all(not r['capture_estimate_unix_ns'] for r in rows)
                assert all(int(r['delivery_unix_ns']) > 1_000_000_000_000_000_000 for r in rows)
                assert all(int(rows[i]['monotonic_elapsed_ns']) <= int(rows[i + 1]['monotonic_elapsed_ns']) for i in range(2))
                decoded = subprocess.run([str(args.cli.resolve()), 'inspect',
                    str(directory / 'epoch-0000-media/raw.avi'), '--decode'],
                    text=True, capture_output=True, check=True, timeout=30)
                assert 'decoded_frames=3 ' in decoded.stdout
                events = [json.loads(s) for s in (directory / 'recording-events.jsonl').read_text().splitlines()]
                assert [e['event'] for e in events] == ['start', 'source', 'stop']
                assert events[-1]['recording_rows'] == 3
                report['takes'].append({'trial': cycle, 'rig': rig, 'rows': len(rows), 'decoded_frames': 3})
        client.screenshot(output / 'remote-rig-grid.png')
        client.command('view', 1, settings=True, settings_section='source')
        time.sleep(.2)
        client.screenshot(output / 'remote-rig-source.png')
        report['final'] = [{k:s[k] for k in ('session','frames','paused','recording','error','command_error')}
                           for s in client.command('sessions')]
        assert all(not s['error'] and not s['command_error'] for s in report['final'])
        report['passed'] = True
    finally:
        client.finish()
        report['exit_code'] = client.process.returncode
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'passed': True, 'takes': len(report['takes']), 'report': str(output / 'report.json')}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/hejjetrac'))
    parser.add_argument('--cli', type=Path, default=Path('target/release/hejjetrac-cli'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cycles', type=int, default=20)
    run(parser.parse_args())
