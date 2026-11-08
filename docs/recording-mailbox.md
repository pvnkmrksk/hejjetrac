# Experiment recording mailbox

Launch one native tracker with its own existing calibration file and a fresh private absolute control directory.

```sh
hejjetrac --config /absolute/path/VR1.txt --windows 1 \
  --control-dir /absolute/path/to/control \
  --control-instance 79f20b742a1c4d738e75b1e79a1d2199
```

The instance token has 32 hexadecimal characters. The host writes an atomic
`endpoint.json` with `protocolVersion: 1`, its backend and process ID, and the
capabilities `native-gui-control`, `per-take-finalization` and
`owned-take-stop-v1`. The launcher retains the native process ID in mailbox mode.
Tracking and the window stay alive across takes.

Write a request atomically as `<id>.request.json`. Use a fresh 32-hex ID,
the descriptor's `instance`, a future Unix time in `expiresUtcMilliseconds`,
`command` and an `args` object. The expiry may be at most two minutes ahead.
Read `<id>.response.json` and match its ID and instance. The result or error is
paired with `ok`. Duplicate IDs return the previous reply. The window accepts
`status`, `start`, `pause`, `record_start` and `record_stop`.

For `record_start`, provide a fresh absolute `directory` and optional
`record_options` with raw and debug booleans. The capture worker refuses to
replace a take that is already active. Wait for status to report that directory
and the first submitted video frame before reporting that recording began.

For `record_stop`, provide the exact `expected_directory` and a unique 32-hex
`stop_request_id`. The capture worker only stops that active take. Wait for
that directory's `completion.json` to report the matching `control_stop_id`,
successful drain and zero pending frames. The receipt also records
`numeric_frames` and `capture_pause_count`. A queued response or an idle
status alone does not establish that the owned take finished.
