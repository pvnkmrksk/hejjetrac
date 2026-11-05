#!/usr/bin/env python3
"""Verify opt-in atlas reuse/refresh on four physical serial-bound cameras.

Development harness; copies configurations and disables experiment outputs.
Airflow jitter supplies observed motion, not physical motion ground truth.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import time

from test_daily_ui_visual import capture_window
from test_live_four import healthy, process_resources, serial_inventory, verify_binding
from test_native_rigs import NativeBridge, field


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def wait(bridge, predicate, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        states = bridge.request('sessions')
        healthy(states)
        if predicate(states):
            return states
        time.sleep(.03)
    raise RuntimeError('Live cache state did not converge')


def observe(bridge, seconds, output):
    first = wait(bridge, lambda states: all(s['last_row'] and s['last_row']['status'] == 'tracked' for s in states))
    start = time.monotonic()
    statuses, resources, samples = Counter(), [], []
    with output.open('w') as stream:
        while time.monotonic() - start < seconds:
            states = bridge.request('sessions')
            healthy(states)
            resource = process_resources(bridge.process.pid)
            resources.append(resource)
            for state in states:
                statuses[state['last_row']['status']] += 1
            sample = {'elapsed_seconds': time.monotonic() - start, 'process': resource,
                      'sessions': [{k: s.get(k) for k in ['session', 'frames', 'last_row', 'actual_fps',
                                   'preview_age_ms', 'tracking_ms', 'atlas_cache', 'view']} for s in states]}
            stream.write(json.dumps(sample) + '\n')
            samples.append(sample)
            time.sleep(.2)
    elapsed = time.monotonic() - start
    last = bridge.request('sessions')
    rates = [(b['frames'] - a['frames']) / elapsed for a, b in zip(first, last)]
    if min(rates) < 80:
        raise RuntimeError(f'Live cache observation cannot sustain requested rate: {rates}')
    return {'seconds': elapsed, 'processed_fps': rates, 'sampled_statuses': dict(statuses),
            'initial_resources': resources[0], 'final_resources': resources[-1],
            'samples': len(samples)}


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    configs = output / 'configs'
    configs.mkdir()
    originals = {}
    for n in range(1, 5):
        path = args.configs / f'VR{n}.txt'
        originals[str(path.resolve())] = sha(path)
        text = path.read_text()
        for key, value in [('src_fn', '99999'), ('sock_port', '0'), ('zmq_port', '0'),
                           ('com_port', ''), ('save_raw', 'n'), ('save_debug', 'n')]:
            text = field(text, key, value)
        (configs / path.name).write_text(text)
    copied = {str(path): sha(path) for path in configs.glob('VR*.txt')}
    report = {'passed': False, 'binary_sha256': sha(args.binary), 'scope': __doc__,
              'inventory': serial_inventory(), 'original_config_sha256': originals, 'phases': {}, 'captures': []}
    expected_views = {n: {'full_image': n % 2 == 0, 'diagnostics': n % 2 == 1,
                          'overlays': True, 'reuse_atlas': True} for n in range(1, 5)}
    bridge = None
    try:
        for phase, seconds in [('cold', args.cold_seconds), ('warm', args.warm_seconds)]:
            directory = output / phase
            directory.mkdir()
            command = [str(args.binary.resolve()), '--config', str(configs / 'VR1.txt'),
                       '--array', '--grid', '2x2', '--control-stdio']
            started = time.monotonic()
            bridge = NativeBridge(command, directory)
            bridge.request('start', 1)
            states = wait(bridge, lambda states: all(s['frames'] > 20 for s in states))
            verify_binding(states, report['inventory'])
            if phase == 'cold':
                if any(s['atlas_cache']['loaded'] for s in states):
                    raise RuntimeError('Cold fixture unexpectedly imported an atlas')
                for n, view in expected_views.items():
                    bridge.request('view', n, **view)
            else:
                states = wait(bridge, lambda states: all(s['atlas_cache']['loaded'] and s['last_row']['status'] == 'tracked' for s in states))
                for state in states:
                    if any(state['view'][key] != value for key, value in expected_views[state['session']].items()):
                        raise RuntimeError('A saved full/debug/cache view was not restored')
                report['warm_all_confirmed_after_process_start_seconds'] = time.monotonic() - started
            report['phases'][phase] = observe(bridge, seconds, directory / 'samples.jsonl')
            states = bridge.request('sessions')
            if phase == 'cold' and any(s['atlas_cache']['saved_frame'] is None for s in states):
                raise RuntimeError('Enabled automatic atlas checkpoints never completed')
            if phase == 'warm':
                bridge.request('view', 3, settings=True, settings_section='atlas')
                report['captures'].append(capture_window(directory, 'atlas-startup-settings', 'HejjéTrac · VR3 configuration'))
                bridge.request('view', 3, settings=False)
                cache = Path(states[2]['atlas_cache']['path'])
                prior = json.loads(cache.read_text())
                bridge.request('pause', 3)
                paused = wait(bridge, lambda states: states[2]['paused'])
                refresh_after = paused[2]['last_row']['frame']
                bridge.request('map_refresh', 3)
                cleared = wait(bridge, lambda states: states[2]['last_row'] is None and 'Atlas cleared' in states[2]['notice'])
                if cleared[2]['atlas_cache']['loaded'] or cleared[2]['atlas_cache']['saved_frame'] is not None:
                    raise RuntimeError('Refresh retained imported atlas state or an old checkpoint')
                if cache.exists():
                    raise RuntimeError('Refresh retained the previous saved atlas')
                bridge.request('realtime', 3, enabled=True)
                bridge.request('start', 3)
                # start resumes an existing paused source, preserving its source clock.
                wait(bridge, lambda states: not states[2]['paused'] and states[2]['last_row'] and states[2]['last_row']['status'] == 'tracked')
                time.sleep(.3)
                bridge.request('view', 3, reuse_atlas=True)
                wait(bridge, lambda states: states[2]['atlas_cache']['saved_frame'] is not None)
                replacement = json.loads(cache.read_text())
                # Source frame numbers restart on each process launch. Compare
                # against this run's paused frame, never the prior process.
                if replacement['source_frame'] <= refresh_after:
                    raise RuntimeError('Save latest atlas retained the old checkpoint')
                report['refresh'] = {'previous_process_checkpoint_frame': prior['source_frame'], 'refresh_after_current_run_source_frame': refresh_after, 'replacement_source_frame': replacement['source_frame'],
                                     'old_cache_removed': True, 'new_checkpoint_saved': True,
                                     'replacement_known_cells': sum(v != 128 for v in replacement['pixels'])}
                bridge.request('snapshot', 3, directory=str(directory / 'after-refresh'))
            bridge.finish(range(1, 5))
            bridge = None
        report['cached_atlases'] = [{
            'path': str(path), 'sha256': sha(path),
            **{key: value for key, value in json.loads(path.read_text()).items() if key in ['version', 'identity', 'width', 'height', 'source_frame']}
        } for path in sorted((configs / '.hejjetrac').glob('*.atlas.json'))]
        report['original_configs_unchanged'] = all(sha(Path(path)) == value for path, value in originals.items())
        report['copied_configs_unchanged'] = all(sha(Path(path)) == value for path, value in copied.items())
        if not report['original_configs_unchanged'] or not report['copied_configs_unchanged']:
            raise RuntimeError('Atlas/view controls changed numerical configuration text')
        report['passed'] = True
    finally:
        if bridge:
            bridge.finish(range(1, 5))
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['phases'], indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/fictrac-desktop'))
    parser.add_argument('--configs', type=Path, default=Path('.'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cold-seconds', type=float, default=35)
    parser.add_argument('--warm-seconds', type=float, default=15)
    run(parser.parse_args())
