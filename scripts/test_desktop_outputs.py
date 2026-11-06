#!/usr/bin/env python3
"""Native real-video UDP/ZeroMQ regression; pyzmq is a test-only dependency."""
import argparse
import hashlib
import json
from pathlib import Path
import socket
import time

import zmq
from zmq.utils.monitor import recv_monitor_message

from test_desktop_mcp import Client
from test_native_rigs import field


def collect(udp, sub):
    u, z = [], []
    deadline = time.monotonic() + .3
    while time.monotonic() < deadline:
        if udp is not None:
            try:
                u.append(udp.recv(16384).strip())
            except BlockingIOError:
                pass
        if sub is not None:
            try:
                z.append(sub.recv(zmq.NOBLOCK).strip())
            except zmq.Again:
                pass
        time.sleep(.001)
    return u, z


def run(args):
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.bind(('127.0.0.1', 0))
    udp.setblocking(False)
    reserve = socket.socket()
    reserve.bind(('127.0.0.1', 0))
    pub_port = reserve.getsockname()[1]
    reserve.close()
    udp_port = udp.getsockname()[1]
    context = zmq.Context()

    def subscribe():
        sub = context.socket(zmq.SUB)
        sub.setsockopt(zmq.LINGER, 0)
        sub.setsockopt(zmq.SUBSCRIBE, b'')
        monitor = sub.get_monitor_socket(events=zmq.EVENT_HANDSHAKE_SUCCEEDED)
        sub.connect(f'tcp://127.0.0.1:{pub_port}')
        return sub, monitor

    def handshake(monitor):
        if not monitor.poll(10000):
            raise RuntimeError('native publisher did not finish its real ZMTP handshake')
        assert recv_monitor_message(monitor)['event'] == zmq.EVENT_HANDSHAKE_SUCCEEDED
        time.sleep(.1)

    text = (root / 'examples/upstream-config.txt').read_text()
    for key, value in [('src_fn', str(root / 'examples/sample.mp4')), ('src_fps', '30'),
                       ('sock_host', '127.0.0.1'), ('sock_port', str(udp_port)),
                       ('zmq_host', '127.0.0.1'), ('zmq_port', str(pub_port)),
                       ('save_raw', 'n'), ('save_debug', 'n'), ('com_port', '')]:
        text = field(text, key, value)
    config = output / 'VR1.txt'
    config.write_text(text)
    sub, monitor = subscribe()
    client = None
    report = {'passed': False, 'video_sha256': hashlib.sha256((root / 'examples/sample.mp4').read_bytes()).hexdigest(),
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'phases': []}
    try:
        client = Client(args.binary.resolve(), output, ['--config', str(config)])
        handshake(monitor)
        client.command('pause', 1)
        client.wait(lambda s: s[0]['paused'])
        for mode, z_on, u_on in [('both', True, True), ('zmq', True, False),
                                 ('udp', False, True), ('both_again', True, True)]:
            if mode == 'both_again':
                sub.close()
                monitor.close()
                sub, monitor = subscribe()
            before = client.command('status', 1)
            client.command('set', 1, fields={'zmq_port': str(pub_port) if z_on else '-1',
                                           'sock_port': str(udp_port) if u_on else '-1'})
            state = client.wait(lambda s: s[0]['revision'] == s[0]['requested_revision'] and
                                bool(s[0]['outputs']['zmq']) == z_on and
                                s[0]['outputs']['udp_enabled'] == u_on)[0]
            assert state['frames'] == before['frames'], 'transport edit advanced a paused source'
            assert not state['error'] and not state['command_error']
            if mode == 'both_again':
                handshake(monitor)
            collect(udp, sub)
            target = state['frames'] + 15
            client.command('step', 1, frames=15)
            done = client.wait(lambda s: s[0]['frames'] >= target and not s[0]['advance_remaining'])[0]
            u, z = collect(udp, sub)
            assert bool(u) == u_on and bool(z) == z_on, (mode, len(u), len(z))
            for row in u + z:
                assert row.startswith(b'FT, ') and len(row.split(b',')) == 26
            if z_on and u_on:
                assert u == z, 'simultaneous transports differed in numeric values/order'
            assert done['frames'] >= before['frames'] + 15
            report['phases'].append({'mode': mode, 'udp_rows': len(u), 'zmq_rows': len(z),
                                     'source_frames_before': before['frames'], 'source_frames_after': done['frames'],
                                     'rows_sha256': hashlib.sha256(b'\n'.join(u or z)).hexdigest()})
        client.command('view', 1, settings=True, settings_section='outputs')
        time.sleep(.2)
        surfaces = client.tool('hejjetrac_surfaces')['structuredContent']
        report['surfaces'] = surfaces
        client.screenshot(output / 'tracking.png')
        report['passed'] = True
    finally:
        if client is not None:
            client.finish()
        sub.close()
        monitor.close()
        context.term()
        udp.close()
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'passed': True, 'phases': report['phases']}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/hejjetrac'))
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
