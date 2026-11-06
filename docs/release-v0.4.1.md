# v0.4.1 — desktop MCP and shared rig controls

This patch adds automatic local agent access to the running desktop and improves
the daily controls for several rigs. Tracking algorithms, numeric acceptance
limits and missing-motion semantics are unchanged from v0.4.0.

- Every desktop launch starts an authenticated loopback MCP endpoint with typed
  session commands and PNG screenshots. Separate app processes receive separate
  endpoints and credentials. [Connection and tool contracts](agent-control.md#automatic-desktop-mcp).
- Grid controls apply footage/debug, zoom/full-frame, calibration overlays and
  recording to all rigs. **Reset tracking · all rigs** is available in both the
  grid and Rig Manager. Click a tile or its settings button to configure a rig;
  double-click to expand it and use Back to return to the grid.
- Rig Manager shows each VR number, actual camera serial and binding state,
  video device, USB hardware port and output health. Output settings offer
  ZeroMQ or UDP socket; new rigs enable ZeroMQ by default. Selecting a transport
  disables the other without rewriting existing legacy configurations on load.
- **Save workspace copy…** exports configurations and view preferences into a
  portable folder. **Load workspace…** accepts its JSON path in another build.
  Save the loaded workspace with Restore on launch to make it the device default.
- Saved workspaces restore the grid layout, window dimensions and individual
  views. Finishing calibration saves the existing rig and closes its inspector.
  [Workspace guide](../book/src/workspaces.md).
- Linux installer checks require the default Vulkan path and separately exercise
  explicit OpenGL. Startup fixtures use isolated user state and verify that a
  missing bound camera reports an error without substituting the sample.
- macOS bundle and citation metadata now match the 0.4.1 source version.

The retained Linux native regression opens sixteen real-video sessions, drives
actual grid navigation, verifies sixteen three-image recordings through the Rust
decoder, checks separate endpoint credentials and restores the saved workspace.
Toolbar alignment and shared/individual control layouts were visually inspected.
[Validation scope](validation.md#automatic-desktop-mcp-and-shared-rig-controls--2026-11-05)
and [machine-readable evidence](benchmarks/desktop-mcp-20261105.json) retain the
original build hashes and execution details.

These checks do not establish sixteen-camera capacity, physical motion ground
truth or native desktop MCP execution on macOS/Windows. The existing codec,
optimizer, recovery and platform limits remain documented in
[parity](parity.md) and [v0.4.0's numerical evidence](release-v0.4.0.md).

The patch's additional native regression checks shared reset, single-click
settings and portable workspace relocation. A separate four-camera Linux check
uses serial-bound A01–A04 at 17°; its prior lab rotations remain unchanged pending
a physical forward-direction reference. These device profiles are local defaults
and a portable backup, not replacements for the bundled calibration examples.
[Patch evidence and limits](validation.md#patch-rig-controls-and-portable-workspaces--2026-11-06).

Platform packages are built and tested natively for Linux x86_64, Windows x64,
Apple Silicon and Intel Mac. Ubuntu 22.04/24.04 checks exercise the installed
Debian package and portable archive. macOS packages remain ad hoc signed and
Windows packages unsigned; [installation instructions](../book/src/install.md)
describe first launch.
