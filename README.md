# HejjéTrac

**Native tracking for spherical treadmills.**

**HejjéTrac** (`hejjetrac`) is a modern Rust reworking of [FicTrac](https://github.com/rjdmoore/fictrac), with a compact desktop app, guided calibration and independent sessions for several rigs. Track a patterned ball, inspect its motion and record the data from the same window.

**[Download now](https://pvnkmrksk.github.io/hejjetrac/?download=1)** · [User manual](https://pvnkmrksk.github.io/hejjetrac/manual/) · [Website](https://pvnkmrksk.github.io/hejjetrac/) · [GitHub](https://github.com/pvnkmrksk/hejjetrac)

[v0.5.0 release notes](docs/release-v0.5.0.md): remote recording control, per-frame synchronization metadata, portable unbound defaults and clearer Ubuntu packaging.
[v0.4.1](docs/release-v0.4.1.md) adds automatic desktop MCP, shared rig controls
and saved window/layout restoration.

![HejjéTrac tracking the supplied FicTrac sample](docs/images/tracking-demo.gif)

*The native app processing the supplied FicTrac sample. [Watch the video](docs/images/tracking-demo.mp4).*

[Updated real-camera calibration screenshots and native screen capture](docs/release-v0.4.0.md)
show the saved 47° M01–M04 outlines. To adjust a saved outline, choose **Edit ball**
and drag a numbered rim mark. Those historical lab profiles used M01–M04 serials; current release defaults are unbound.

The minimized-rig-manager UI freeze is fixed by the corrected hardware-GPU
presentation path. [Reproduction and native-input checks](docs/ui-rendering.md)
separate actual window responsiveness from background tracking FPS.

**Several live cameras:** run simultaneous sessions with one saved calibration
and serial link per rig. [Multi-camera setup](book/src/rigs.md) explains linking,
array launch and separate outputs. [Example hardware and optional Arducam serial
programming](book/src/appendix-cameras.md) records our compact OV9281 global-shutter
boards for replication; a suitable regular webcam also works.

[Saved workspaces and atlas startup](book/src/workspaces.md) explains graphical
setup of up to sixteen rigs, a single-surface grid for larger arrays, compact arrangement, app-launch restoration,
remembered window sizes, square/custom grid layouts, shared view/record controls and optional reuse of the latest accepted atlas.

Created by **[Pavan Kumar Kaushik](https://github.com/pvnkmrksk)**. The name combines **ಹೆಜ್ಜೆ** (*hejjé*, roughly **HEY-jay**), Kannada for a footstep, footprint or path, with **Trac**, following FicTrac’s spelling: **HEY-jay-track**. [Word origin](https://alar.ink/dictionary/kannada/english/%E0%B2%B9%E0%B3%86%E0%B2%9C%E0%B3%8D%E0%B2%9C%E0%B3%86).

## Open and start

Download the package for your computer, extract it and open **HejjéTrac**. The app tries the first available camera and falls back to the bundled sample when a camera cannot open or deliver frames. Your own camera needs its own [calibration](https://pvnkmrksk.github.io/hejjetrac/manual/calibration.html).

| Platform | First launch |
|---|---|
| macOS · Apple Silicon or Intel | Open `hejjetrac.app`. If blocked as an unidentified developer, attempt to open it, then use **System Settings → Privacy & Security → Open Anyway**. [Apple's instructions](https://support.apple.com/en-us/102445). |
| Windows · x64 | Extract the ZIP and open `hejjetrac.exe`. If SmartScreen offers it, use **More info → Run anyway** for the downloaded release. [Windows details](https://pvnkmrksk.github.io/hejjetrac/manual/install.html#windows). |
| Linux · x86_64 | On Ubuntu **22.04 or newer**, install the `.deb`, or extract the portable archive and launch `hejjetrac`. New Linux packages target **glibc 2.35+**. [Distribution requirements](https://pvnkmrksk.github.io/hejjetrac/manual/install.html#linux). |

Current downloads are ad hoc signed on Mac and unsigned on Windows. Use the project's release files; managed computers may require administrator approval for a first-launch exception. [Installation and permissions](https://pvnkmrksk.github.io/hejjetrac/manual/install.html) covers the exact prompts. No Rust installation or compiler is needed. New packages ship only `hejjetrac` and `hejjetrac-cli`.

## Built for the experiment

- **A focused tracking window.** Ball preview, path, delivered FPS and tracking time stay visible. Settings open in a separate inspector.
- **Guided calibration.** The ball outline fits live from three rim marks. Redraw exclusions and lab axes in turn, with a precision magnifier, right-click correction and the original FicTrac coordinate conventions.
- **Independent rigs.** Each window has its own source, calibration, recording and outputs. View up to sixteen independent rigs in a hardware-rendered grid. Double-click a tile for individual controls and return with Back to rigs.
- **Complete recording.** Save data, source video and diagnostic video. Every take retains all-frame numeric data, quality decisions and its configuration.
- **Live output.** Use UDP, serial or ZeroMQ for downstream experiments. ZeroMQ is enabled by default on `0.0.0.0:9871`, serving localhost and IPv4 LAN receivers; each rig needs its own unused port. An explicit [legacy MATREX VR mode](docs/matrex-vr.md) matches the Python JSON republisher and old NetMQ handshake; native FT remains the default. The CLI, JSONL and MCP interfaces call the same numerical engine as the desktop. Every desktop launch also starts a local [agent MCP endpoint](docs/agent-control.md#automatic-desktop-mcp) with screenshots and JSON control of the running app.
- **A Rust application.** Native windows and camera APIs, Rust video codecs and a shared engine. No OpenCV, FFmpeg executable, Python runtime or webview is required.

![The native tracking window](docs/images/preview.png)

[First session](https://pvnkmrksk.github.io/hejjetrac/manual/quick-start.html) · [Hardware and lighting](https://pvnkmrksk.github.io/hejjetrac/manual/hardware.html) · [Calibration](https://pvnkmrksk.github.io/hejjetrac/manual/calibration.html)

## Companion experiments and analysis

[MATREX VR](https://github.com/pvnkmrksk/matrexVR) supplies configurable Unity visual environments for animal experiments, with a [complete manual](https://pvnkmrksk.github.io/matrexVR/manual/) covering rigs, display mapping, tracking units, recording and replay. Match its exact release/source input contract to HejjéTrac's native FT output or the built-in [legacy MATREX VR JSON mode](docs/matrex-vr.md); native FT decoding and modern transport updates are current-source additions after MATREX VR 0.3, not a change to that archived binary. [Connection guide](https://pvnkmrksk.github.io/matrexVR/manual/input-contracts.html).

[Daari Deepa](https://github.com/pvnkmrksk/trajectory-dashboard) provides browser-based trajectory analysis. [Rekkefly](https://github.com/pvnkmrksk/rekkefly) is the companion flight/wing tracker. These are separate tools with their own releases and output contracts.

## Coordinates and configuration

The manual retains FicTrac’s original [coordinate guide](book/src/upstream-coordinates.md), [complete parameter table](book/src/upstream-parameters.md), [hardware and algorithm notes](book/src/upstream-setup.md) and [25-column data reference](book/src/upstream-data.md), with their source and license. [Current configuration](book/src/configuration.md) and [output semantics](docs/output.md) describe the Rust application’s additions.

![FicTrac camera, animal and world coordinate frames](docs/images/upstream/coordinate-frames.png)

*Original FicTrac illustration by R. J. D. Moore and contributors, CC BY-NC-SA 3.0. [Source and unchanged image](docs/images/upstream/README.md).*

## Several rigs, one launch

The desktop accepts a grid directly:

```sh
hejjetrac --windows 4 --grid 2x2
hejjetrac --windows 16 --grid 4x4
hejjetrac --config VR1.txt --window-size 480x360
hejjetrac --config VR1.txt --array
```

A fresh installation opens **one unbound camera**. Release `VR1.txt`–`VR4.txt`
come from [`examples/defaults`](examples/defaults), with no camera serial, USB
socket, machine path or forced capture format. First-run seeding chooses actual
enumerated camera IDs, including Linux device-number gaps. The centered ball
outline and 45° FOV are preview placeholders: calibrate your own camera before
collecting measurements. Existing saved rigs/workspaces remain authoritative.

Open four rigs using **Rig Manager → Start saved array**, or `hejjetrac --windows 4 --grid 2x2`.
Each starts unbound and has its own ZeroMQ port (9871–9874); UDP is disabled until
configured. Use the camera chooser to assign each source, optionally link its
unique hardware ID, then save its calibration. Missing, duplicated or already
owned cameras leave that rig stopped. Unlink keeps the actually opened camera
and its tracking epoch. [Multi-rig setup](book/src/rigs.md) ·
[Binding and platform limits](book/src/configuration.md#optional-camera-binding).

The repository-root VR files and `examples/rigs` retain historical lab settings
for reproducibility; they are not the configurations installed as release defaults.
[`scripts/run_four_bridged.sh`](scripts/run_four_bridged.sh) remains an explicit
adapter for calibrated lab files and existing UDP-to-ZMQ republishers.

**More → Start saved rig array** starts the saved numbered rig files and reports
each failure independently. Array startup never substitutes a stock video.

**Separate tracking and VR computers:** launch with
`hejjetrac --control-bind 0.0.0.0:17871`, then send authenticated recording
commands from the VR computer. Individual or all-rig takes accept common trial
IDs/tags. Every take retains a wall-clock/monotonic frame ledger for manual
alignment. [Remote trigger and synchronization guide](docs/remote-recording.md).

Display FPS controls preview updates independently of processing. The recording
speed/size control selects the configured codec, faster MJPEG, or fastest
uncompressed AVI. Live media workers have bounded queues: an overloaded video
stream becomes an explicit partial recording while numeric data and tracking
continue. Recorded-file sessions wait for encoding to preserve every image.
[Recording behavior and measured limits](docs/recording-performance.md).

For recorded video:

```sh
hejjetrac-cli track --config examples/benchmark/config.txt --input examples/sample.mp4 --output runs/benchmark
```

The companion CLI is in the extracted folder; on macOS it is inside `hejjetrac.app/Contents/MacOS`. [CLI reference](https://pvnkmrksk.github.io/hejjetrac/manual/cli.html) · [Agent control](docs/agent-control.md).

## Tracking and validation

[Atlas learning](docs/atlas-learning.md) describes early consecutive-image
refinement and how later accepted passes can correct saturated map cells.
[Recorded-clock high-speed checks](docs/recorded-clock.md) explain why a
constant-rate replay could conceal failures seen with the live camera's
8/12 ms capture intervals, and how the same core is tested with them.

HejjéTrac preserves FicTrac's legacy config text and data conventions, and checks the full supplied video against the supplied FicTrac output. Known-motion synthetic sequences cover rotation accuracy. The Rust optimizer and recovery policy differ from upstream; [compatibility](docs/parity.md), [validation](docs/validation.md) and [output semantics](docs/output.md) document those details and platform coverage. Rejected frames remain in the all-frame data; motion during a missing interval is left unmeasured.

[VR3 replay validation](docs/vr3-validation.md) additionally checks the supplied
5,157-frame recording, its high-speed section and complete trajectory, with
explicit accuracy limits and a separate post-occlusion control.

Native release CI runs on Windows, Linux, Apple Silicon and Intel Mac. Camera and GUI hardware checks are documented per platform. [Tracking performance](docs/tracking-performance.md) and [resource use](docs/resources.md) contain the fixture, hardware and timing boundaries for the published benchmarks.

The November 4 47° four-camera six-minute run measured **27.30 ms median preview age**, 44.84 ms
p95 and 48.53 ms maximum while each rig processed about 101 frames/s. The maximum
occurred during steady state. Preview cadence is independent of tracking;
[the timing explanation and distributions](docs/preview-age.md) describe the
30 FPS display limit, observed publication rate and relative-clock measurement.
The separate historical hand-driven test exposes a 6.61-second M02 tracking
recovery gap; [fresh validation](docs/validation.md) preserves that failure separately.

## Build and contribute

Install [Rust](https://rustup.rs/) 1.95 or later, then:

```sh
cargo run --release --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Cargo builds `hejjetrac` (desktop) and `hejjetrac-cli` directly. The workspace uses `hejjetrac-core`, `hejjetrac-io`, `hejjetrac-camera`, `hejjetrac-control`, `hejjetrac-cli` and `hejjetrac-desktop`; the engine has no UI, filesystem, network or codec dependencies. [Architecture](docs/architecture.md) · [Contributor guide](AGENTS.md) · [Build instructions](https://pvnkmrksk.github.io/hejjetrac/manual/development.html).

## Citation and license

If you use HejjéTrac in research, cite the version you used and the original FicTrac method:

> Pavan Kumar Kaushik. *HejjéTrac: native spherical-motion tracking.* [Software repository](https://github.com/pvnkmrksk/hejjetrac).
>
> R. J. D. Moore et al. (2014). *FicTrac: a visual method for tracking spherical motion and generating fictive animal paths.* Journal of Neuroscience Methods 225, 106–119. [doi:10.1016/j.jneumeth.2014.01.010](https://doi.org/10.1016/j.jneumeth.2014.01.010).

[CITATION.cff](CITATION.cff) supplies machine-readable citation metadata. [Archiving instructions](https://pvnkmrksk.github.io/hejjetrac/manual/citing.html) describe connecting a release to Zenodo.

FicTrac was developed by R. J. D. Moore, Saul Thurrowgood and collaborators. This project preserves its **CC BY-NC-SA 3.0** license and openly credits its algorithms, examples, illustrations and documentation. [Attribution](ATTRIBUTION.md) · [License](LICENSE) · [Dependency notices](THIRD_PARTY.md).
