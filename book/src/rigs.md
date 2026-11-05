# Several rigs

Each native window owns an independent source, calibration, tracker, output state and recording. The bundled `VR1.txt`–`VR4.txt` derive from the original BR rig configurations.

| Instance | Serial link | Historical numeric ID | UDP destination port | Local ZeroMQ PUB bind |
|---|---|---:|---:|---:|
| VR1 | M01 | 0 | 1111 | 9871 |
| VR2 | M02 | 2 | 1112 | 9872 |
| VR3 | M03 | 4 | 1113 | 9873 |
| VR4 | M04 | 6 | 1114 | 9874 |

These are examples for their original physical setup, not calibration for arbitrary cameras. Check every device and source geometry before using its data.

The serial link resolves the current device number. Our observed M01/M02/M03/M04
nodes were 6/2/4/0, illustrating why saved numeric order is unreliable. In
**Source & camera**, identify the serial, use the link/unlink icon and save the
rig. [The camera appendix](appendix-cameras.md) gives the optional one-time
Arducam serial procedure. Other suitable cameras work without programming.

## Creating windows

New desktop windows use existing saved VR/BR slots first. Malformed saved slots
stay visible as failed rigs; subsequent slots still start. When there is no saved
slot, New rig creates an editable copy that needs its own source and calibration.
Unnumbered documents retain the copy workflow, incrementing numeric camera ID,
enabled ports and instance name; video paths remain paths. Unsaved copies do not
inherit a save path or camera binding. The recorded-video headless controller
retains its existing sibling/copy workflow.

Closing one native tracking window, including the first one opened, leaves the other sessions running. Each remaining window retains its source, clock, outputs and recording. Explicit application shutdown/quit closes all sessions; normal per-window close still observes unsaved-state and recording finalization rules.

## Manage rigs from the app

[The graphical workspace guide](workspaces.md) covers package installation,
adding sixteen rigs without a terminal, compact arrangement, startup restoration,
saved view choices and optional reuse of the latest atlas.

In current source, open **More → Rig manager** to see every running session. Each row opens that rig's **Settings** or its read-only **Full config** text. Review source IDs, calibration and output ports here before an experiment. Save changes through the inspector for that rig.

Choose **More → Start saved rig array**, or **Start saved array** in the manager.
It opens all existing VR1–VR16 or BR1–BR16 sibling files, starts each independently
and arranges their windows. Failed entries remain labelled **Did not start** with
their individual error. Working rigs continue with their own ports. A missing
camera, duplicate binding, malformed file or busy device never substitutes a stock
video. A single-window sample fallback is stopped and its saved camera config is
restored before joining the array. Explicitly configured video files remain video
sources.

**Start all** resumes existing paused sessions and retries stopped sources.
**Arrange in a grid** tiles the open windows. Save one config per rig before
starting the array; review calibration, bindings and output ports for each rig.
The four provisioned OV9281s inspected on 2026-11-03 expose distinct M01–M04
serials. Earlier unprovisioned units shared `UC762`; identical serials require
an explicit USB socket binding or verified unique provisioning.
[Binding details and hardware limits](configuration.md#optional-camera-binding).

## A compact tiled grid

```sh
./hejjetrac --windows 16 --grid 4x4
./hejjetrac --config VR1.txt --array
./hejjetrac --config VR1.txt --window-size 360x260 --window-position=0,24
```

Size and position use logical pixels. Without a size override, grid placement derives cells from the monitor. New windows otherwise place themselves beside existing ones. The supported native session count is bounded at sixteen.

## Budget the actual workload

A sixteen-window sample test verifies independent native windows and numerical output. It is not a sustained sixteen-camera acquisition/recording guarantee. Source USB links, capture drivers, conversion, display, encoders and disk throughput all add work. Recording and diagnostic views can cost more than numerical tracking. Use correct source pacing, inspect skips, and test the entire rig count under the intended experiment load.

Use a distinct UDP destination and ZeroMQ bind per instance. PUB defaults to enabled on `0.0.0.0:9871`, serving localhost and IPv4 LAN interfaces. Copied rigs increment the effective port, including when the original omits it. Connect a remote receiver to the tracking computer's LAN IP and matching port; explicit local interface binds and `::` for IPv6 are also supported. `zmq_host` is not the remote receiver's IP. Only one publisher can own each bind port, including an existing Python republisher. Queue/drop counters describe application-level behavior; they cannot certify subscriber delivery. The exact wire contract and its loss semantics are in [outputs](output.md).
