#!/usr/bin/env python3
"""Compare a complete recorded-clock replay without dropping rejected images.

Development evidence only. The original FicTrac reference measures agreement,
not physical ground truth. The take and its calibration remain read-only.
"""
import argparse
from collections import Counter
import csv
import hashlib
import json
import math
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rms(records, reference, start, end):
    return [math.sqrt(sum((item['row']['delta_camera'][axis] - reference[index][axis + 1]) ** 2
                         for index, item in enumerate(records[start:end], start)) / (end - start))
            for axis in range(3)]


def describe(records, timestamps, reference):
    missing, intervals = None, []
    for index, item in enumerate(records):
        tracked = item['row']['status'] == 'tracked'
        if index and not tracked and missing is None:
            missing = index
        if tracked and missing is not None:
            intervals.append({'first_image': missing, 'next_tracked_image': index,
                              'duration_ms': timestamps[index] - timestamps[missing]})
            missing = None
    if missing is not None:
        intervals.append({'first_image': missing, 'next_tracked_image': None,
                          'duration_ms': timestamps[-1] - timestamps[missing]})
    windows = [(171, 300), (1600, 1700), (3700, 4050), (7800, 8200)]
    return {'frames': len(records), 'statuses': dict(Counter(x['row']['status'] for x in records)),
            'all_decision_component_rms_rad': rms(records, reference, 1, len(records)),
            'maximum_unmeasured_interval_ms': max((x['duration_ms'] for x in intervals), default=0),
            'unmeasured_intervals': intervals,
            'diagnostic_windows': [
                {'start_inclusive': start, 'end_exclusive': end,
                 'statuses': dict(Counter(x['row']['status'] for x in records[start:end])),
                 'all_decision_component_rms_rad': rms(records, reference, start, end)}
                for start, end in windows if end <= len(records)]}


def run(args):
    take = args.take
    quality = list(csv.DictReader((take / 'epoch-0000-quality.csv').open()))
    timestamps = [float(row['timestamp_ms']) for row in quality]
    reference = [[float(v.strip()) for v in line.split(',')] for line in args.reference.read_text().splitlines()]
    records = [json.loads(line) for line in args.candidate.read_text().splitlines()]
    baseline = [json.loads(line) for line in args.baseline.read_text().splitlines()]
    if not (len(quality) == len(reference) == len(records) == len(baseline)):
        raise RuntimeError('Capture, reference and both replays must contain every source image')
    for replay in [records, baseline]:
        for index, item in enumerate(replay):
            row = item['row']
            if item['source_frame_index'] != index or row['timestamp_ms'] != timestamps[index]:
                raise RuntimeError('Replay changed source image order or recorded timestamp')
            if not all(math.isfinite(v) for v in row['delta_camera']):
                raise RuntimeError('Replay emitted nonfinite motion')
            if row['status'] != 'tracked' and row['delta_camera'] != [0., 0., 0.]:
                raise RuntimeError('Replay fabricated relative motion during initialization/recovery')
    intervals = sorted(b - a for a, b in zip(timestamps, timestamps[1:]))
    candidate = describe(records, timestamps, reference)
    report = {'date': '2026-11-05', 'scope': __doc__,
              'recording': str(take),
              'source_sha256': sha(take / 'epoch-0000-media/raw.avi'),
              'recorded_config_sha256': sha(take / 'epoch-0000-config.txt'),
              'replay_config_sha256': sha(args.config),
              'reference_sha256': sha(args.reference), 'candidate_sha256': sha(args.candidate),
              'baseline_sha256': sha(args.baseline), 'evidence_binary_sha256': sha(args.binary),
              'initial_state_limit': 'Recording begins with an already learned live atlas; both replays start empty. The post-run cache is not its initial state.',
              'calibration': 'Recorded 47-degree field of view and saved rim/exclusions/axes retained; output sinks disabled for replay.',
              'timestamp_interval_ms': {'median': intervals[len(intervals) // 2],
                                        'p95': intervals[int(len(intervals) * .95)], 'maximum': max(intervals)},
              'live_quality_statuses': dict(Counter(x['status'] for x in quality)),
              'baseline': describe(baseline, timestamps, reference), 'candidate': candidate,
              'integrity': {'every_source_image': True, 'exact_recorded_clock': True,
                            'missing_relative_motion_zero': True, 'no_accuracy_tolerance_changed': True}}
    if args.run_metadata:
        run = json.loads(args.run_metadata.read_text())
        if run['binary_sha256'] != report['evidence_binary_sha256'] or run['config_sha256'] != report['replay_config_sha256']:
            raise RuntimeError('Run metadata does not identify the tested binary/configuration')
        report['run'] = run
        report['timing_boundary'] = 'One serial release run: Rust source open/decode, tracking and numerical diagnostics, all JSONL evidence writes and close. Excludes original-FicTrac comparison and graphical presentation. Not a repeated steady-state performance distribution.'
    if args.build_report:
        build = json.loads(args.build_report.read_text())
        report['build'] = {k: build[k] for k in ['platform', 'cpu_model', 'logical_cpus', 'build_profile',
                          'rustflags', 'rustc_verbose', 'core_source_sha256', 'source_commit', 'source_dirty']}
    report['accuracy_limit'] = 'Full-clip component RMS is reported with every decision. The 171..300 diagnostic window has a Z RMS above the existing 0.020 rad target; full-clip agreement does not erase that local error. No physical motion ground truth is available.'
    report['baseline_scope'] = 'Retained exact-clock output before the fixes, using the same recorded geometry, quality six and 50-evaluation limit. The baseline executable was not retained for a matched performance comparison.'
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: candidate[k] for k in ['frames', 'statuses', 'all_decision_component_rms_rad', 'maximum_unmeasured_interval_ms']}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ['take', 'candidate', 'baseline', 'reference', 'config', 'binary', 'output']:
        parser.add_argument('--' + flag, type=Path, required=True)
    parser.add_argument('--run-metadata', type=Path)
    parser.add_argument('--build-report', type=Path)
    run(parser.parse_args())
