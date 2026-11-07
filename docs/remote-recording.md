# Recording with tracking and VR on separate computers

The native desktop accepts authenticated start/stop commands over the LAN.
Launch it on the **tracking computer** with an explicit listener:

```sh
hejjetrac --control-bind 0.0.0.0:17871
hejjetrac --config /path/to/VR1.txt --array --control-bind 0.0.0.0:17871
```

The first command uses the normal saved workspace, or one unbound camera on a
fresh installation. The second opens the saved rig array. Without
`--control-bind`, control listens on loopback only. An exact local IP can replace
`0.0.0.0`; IPv6 uses `[address]:port`. Permit the chosen TCP port in the tracking
computer's firewall for the VR computer. Motion output ports are independent.

In Rig Manager, **Copy connection JSON** gives the URL and per-launch bearer
token. The same object is in `mcp/<PID>.json` beside `workspace.json`; the
descriptor also reports `listen_address` and `remote_control`. With a wildcard
listener, its URL uses localhost for local clients. Copy the connection object
to your experiment client and replace the URL host with the tracking computer's
numeric LAN IP, for example `http://192.168.1.20:17871/mcp`. Tokens change on
restart. This HTTP endpoint provides control of the app: use a trusted private
LAN or a VPN/SSH tunnel. It does not supply TLS itself.

## Commands from the VR computer

Send HTTP POST requests to `/mcp` with `Content-Type: application/json`,
`MCP-Protocol-Version: 2025-11-25` and `Authorization: Bearer <token>`.
First initialize the client:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"vr-experiment","version":"1"}}}
```

Then discover running sessions:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"hejjetrac_execute","arguments":{"command":"sessions"}}}
```

Start a take on session 1. The destination is a **new directory on the tracking
computer**, even though the command originates on the VR computer:

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hejjetrac_execute","arguments":{"command":"record_start","session":1,"args":{"directory":"/data/experiment-42/trial-007","record_options":{"raw":true,"debug":false},"recording_tuning":{"encoding":"mjpeg"},"recording_metadata":{"experiment_id":"experiment-42","trial_id":"trial-007","trigger_id":"start-007","trigger_unix_ns":"1793970000123456789","tags":{"animal":"fly-12","scene":"corridor"}}}}}}
```

`trigger_unix_ns` is the VR computer's UTC Unix time at the trigger, as a decimal
string. Store the same experiment/trial IDs and clock in the VR log. Up to 32
string tags are allowed. These are data; they never execute commands.

Stop that take:

```json
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"hejjetrac_execute","arguments":{"command":"record_stop","session":1}}}
```

For all active rigs, replace the arguments with:

```json
{"command":"array_record","args":{"start":true,"directory":"/data/experiment-42/trial-008","recording_metadata":{"experiment_id":"experiment-42","trial_id":"trial-008","tags":{"scene":"corridor"}}}}
```

```json
{"command":"array_record","args":{"start":false}}
```

Each rig writes a separate `rig-<session>` subfolder with the shared trial tags.
The array uses each rig's video/encoding selections. Starts are queued to
independent workers; they are not simultaneous hardware shutter triggers.

Inspect `result.structuredContent.ok` and its error, not only HTTP status. A
`queued:true` reply is an acknowledgement, not completion. Poll `sessions` or
session `status` until the desired `recording` state is observed. After Stop,
also wait for the matching `recording_finalizations` entry to be `complete`
without `numeric_error` or `completion_error`; inspect media warnings separately.
`completion.json` is the durable finalization report. If a connection fails
after sending Start, poll the recording directory/state before retrying.
Stopping an already stopped rig is safe. Every new take needs a new directory.

Browser-origin requests are rejected. Experiment software, curl and standard
HTTP clients can call the endpoint directly; no model service is required.
The same typed commands are available locally through desktop JSONL/MCP and
the shared headless recorded-video controller.

## Manual synchronization and clock limits

Every new desktop take, headless recording and CLI tracking run includes:

| File | Evidence |
|---|---|
| `recording.json` | App version, platform, source/binding, rig name, optional host name, trial metadata and UTC/monotonic clock anchor |
| `frame-times.csv` | One row per all-frame numeric row, including rejected/recovering observations; recording row, config epoch, tracker frame, source sequence when available, original source and emitted timestamps, delivery UTC estimate, capture UTC estimate, observed host UTC and monotonic elapsed time |
| `recording-events.jsonl` | Start, source identity, configuration epochs and Stop; desktop source events retain the actual opened camera number, hardware ID and USB socket when available |

UTC values use **Unix nanoseconds**; JSON represents them as decimal strings to
avoid floating-point integer loss. CSV timestamps ending `_ms` are milliseconds.
Desktop `epoch` selects `epoch-NNNN-all-frames.dat` and its quality/config/media
files. `recording_row` is a zero-based index across the entire take. Full all-frame
numeric/quality data retain rejected frames; the legacy accepted-only `.dat`
and live wire formats are unchanged. Media status records how many video frames
were actually encoded if optional recording stops under load.

`delivery_unix_ns` and `capture_estimate_unix_ns` map monotonic instants through
the fixed clock anchor. `observed_unix_ns` samples the host UTC clock per row,
so operating-system clock adjustments are visible. The capture estimate uses
the camera's relative timestamp normalized to host delivery; fixed sensor/USB
latency is not independently measured. Capture UTC is empty for replayed files:
their original exposure wall time cannot be reconstructed from video PTS alone.
Source and emitted clocks remain separate when `src_fps` changes replay timing.
`source_sequence` is the camera adapter delivery counter or the zero-based decoded
file index; a delivery counter alone cannot prove that a sensor lost no frames.

Synchronize both computers with NTP or PTP, save the VR host's UTC event/frame
log and keep common trial tags. Join on trial ID, then align the time series by
UTC while accounting for network delay and measured clock offset. MCP `ping`
also returns `server_unix_ns` for a client to bracket with its send/receive times.
The monotonic columns preserve interval evidence across host-clock corrections.
These records support manual alignment; they do not establish submillisecond
exposure alignment. Use a shared hardware clock/trigger when that accuracy is
required. Pauses, rejected frames and acquisition gaps remain missing motion.

## Validation scope

See the current [release validation](validation.md) for actual LAN-interface,
real-video, recording and camera-binding stress results. Automated device
inventory permutations do not establish physical USB hotplug behavior or
native macOS/Windows camera execution.
