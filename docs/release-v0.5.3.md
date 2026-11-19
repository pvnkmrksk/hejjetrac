# v0.5.3 — cropped-camera atlases and camera reconnection

This release packages the merged camera-reconnection and atlas fixes that
follow v0.5.2.

- **Camera reconnection:** after an established live stream fails or times out,
  the affected rig retries automatically with bounded backoff. Linked rigs keep
  their saved camera/socket identity across device renumbering. Pause state and
  active recordings survive; the timing ledger preserves the outage and motion
  during that interval remains unmeasured. [Rig behavior](../book/src/rigs.md).
- **Cropped-camera atlas support:** fitting and learning exclude sites without
  a complete bilinear source-image footprint. Off-camera regions remain unknown
  instead of becoming fabricated binary observations. Automatic atlas cache
  version 2 starts fresh rather than reusing maps learned under the earlier
  sampling policy. Visible holders and wires still need exclusion masks.
  [Sampling policy](atlas-learning.md).
- **Atlas PNGs:** **Save atlas PNG** writes directly to the app's atlas folder,
  using the displayed rig name and numbered suffixes that preserve existing
  files. Manual import/export uses an asynchronous, parented chooser, with a
  direct filename alternative. Tracking and previews continue while it is open.
  [Folders and controls](../book/src/workspaces.md).
- **Documentation and CI:** the generated manual includes the experiment
  [recording mailbox contract](recording-mailbox.md). Ubuntu 24.04 is the Linux
  portability/runtime test target; Windows and macOS checks remain. Linux
  packages retain their Ubuntu 22.04/glibc 2.35 build baseline.

The detailed [validation evidence](validation.md) separates known-motion
cropped-sphere tests and real-video replays from physical-motion ground truth.
Native chooser/default-save checks were performed on Linux X11; they do not
establish other desktop systems' dialog behavior. Camera recovery coverage
includes a scripted worker outage and a reported Linux lab reconnect, rather
than an exhaustive physical camera/hub matrix. Existing optimizer, codec and
platform boundaries remain in the [parity assessment](parity.md).

The version bump and test-matrix change add no further numerical behavior.
Native v0.5.3 package-build results are recorded in the release's GitHub Actions
run; historical validation entries retain their original execution scope.
