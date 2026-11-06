# Saved workspaces, compact grids and atlas startup

The rig manager opens up to sixteen independent sessions and can save them for
the next graphical launch. Each rig keeps its own calibration, camera link,
outputs, view choices and optional atlas cache.

## Install and launch

On Debian/Ubuntu, open the downloaded `.deb` in the graphical package installer.
Alternatively, install that downloaded file from its folder:

```sh
sudo apt install ./hejjetrac-linux-amd64.deb
```

Launch **HejjéTrac** from the applications menu as your normal user. Application
resources under `/usr/lib/hejjetrak` belong to the package manager. On an ordinary
first launch, writable default rig files are created in `~/.config/hejjetrac`
on Linux, `%APPDATA%\Hejjetrac` on Windows, or
`~/Library/Application Support/Hejjetrac` on macOS. Updates preserve existing
user configurations. You can keep experiment configs in another writable folder.

## Set up the rigs graphically

1. Open **More → Rig manager** in a tracking window.
2. **More → Add saved rigs…** selects existing configuration files, including files
   from different folders. **New rig** creates another editable session. The
   limit is sixteen open rigs.
3. Open a rig's settings icon, then **Source**. Select its camera or video. For live
   cameras, use the link icon to bind the exact physical identity, then save
   the configuration. A camera can be owned by only one session.
4. Choose **Calibration → Open graphical calibration…**. Review, edit or redraw
   the ball, exclusions and lab layers. Finish to save the calibration. The guided editor
   uses the full image and restores the previous normal view afterward. Each
   physical rig needs its own fit.
5. In **Outputs**, assign distinct enabled ports. For example, UDP destinations
   1111–1126 and ZeroMQ bind ports 9871–9886 can serve sixteen rigs when those
   ports are free. Disable unused outputs. A bind address describes this tracking
   computer, rather than the receiving computer.
6. In **View**, choose the full source or focused ball, diagnostics and outlines.
   Those choices and preview FPS return on clean close/relaunch of the saved
   rig, without rewriting its calibration text.

For numbered files in one folder, **More → Open saved array** opens the existing
`VR1.txt`–`VR16.txt` or `BR1.txt`–`BR16.txt` siblings and reports failures
independently. The play icon resumes paused rigs, retries stopped sources and
replays finished files. An array never replaces a failed camera with a sample.

## Fit every rig on screen

Finish or close calibration, then click the grid icon. Multiple rigs
share one hardware-rendered grid window. Each rig retains its independent
source, tracker, calibration, outputs and view preferences. Manager and settings
open within the same surface. The grid defaults to a square layout, or uses the
columns/rows selected in Rig Manager.

Double-click a tile to expand it for individual controls, diagnostics and graphical
calibration. The **Back to rig grid** icon returns to the grid and retains that rig's selected
view. Shared grid controls appear only in grid view; an expanded rig shows only
its individual controls and Back. All other trackers continue. Compact tiles show identity and status, without
individual toolbars. Shared view and recording controls are available in Rig
Manager and the grid toolbar. Closing the whole grid asks before discarding
unsaved configuration text. The optional terminal equivalent is:

```sh
hejjetrac --config VR1.txt --array --grid 4x4
```

Sixteen sessions do not establish sixteen-camera acquisition or simultaneous
encoding capacity on every computer. Test the actual formats, USB paths,
recording and view modes before an experiment. File replay preserves its images
and may run behind wall time under load. Numerical FPS alone does not establish
preview latency.

![Sixteen independent replay rigs sharing one native GPU surface](images/grid-sixteen-20261105.png)

The native Linux stress test processes all 5,157 VR3 images in each of sixteen
sessions. Its accepted outputs match a serial reference. This tests layout and
software concurrency, rather than sixteen-camera capture capacity.

## Restore the workspace from the app launcher

In the rig manager, enable **Restore saved workspace on graphical launch** and
choose **Save current rigs as startup workspace**. Save every rig configuration
first; unsaved configuration changes prevent workspace saving. The manifest
stores their full paths, so leave those files in their chosen folders. Re-save
the workspace after adding, replacing or removing rigs.

The next ordinary app-menu launch opens those rigs and arranges them for the
current screen. Each rig restores its full/focused image, debug, outlines,
magnifier and preview FPS. Camera failures stay visible independently. An
explicit `--config` launch uses the chosen configuration instead. To stop
automatic restoration, clear the checkbox and save the workspace again.

## Reuse and refresh the latest atlas

In **Settings → Atlas & startup**, enable **Reuse this rig's last atlas at
startup**. The default is off. Live rigs need a physical camera link first.
Keep the ball pattern, lens, capture geometry and calibration consistent.

Accepted tracking checkpoints the current atlas and last measured pose every
thirty seconds and on clean source shutdown. **Save latest atlas now** saves a
current accepted checkpoint. Later supported passes keep refining it. Gray
areas still mean unseen surface; reusing a partial atlas does not make it a
fully observed panorama.

Startup checks camera/source identity and the full numerical configuration.
On a mismatch, the app starts fresh and explains why. The cached pose is a
search seed: fresh images must confirm the current pose before ordinary
relative tracking resumes. Instantaneous first-frame tracking is not guaranteed.
Movement while the app was closed remains unmeasured.

The **top-left refresh arrow**, **Shift+R**, or **Refresh atlas** in settings
discards the current atlas and, with reuse enabled, removes its saved cache.
Fresh accepted views build the replacement. Source time and already measured
integrated displacement remain; pose and path start a new epoch. The next
checkpoint/clean close saves the replacement. Pause/resume retains the atlas.
Agent `reset` keeps its existing reset policy; `map_refresh` explicitly clears
the atlas.

The advanced PNG import/export section is manual. Standalone PNGs do not carry
the automatic cache's source identity, numerical configuration or saved pose.
Import them only with a matching calibration and localization settings.

## Config text and saved files

**Advanced → View exact configuration text…** opens a read-only view. Explicitly
enable **Enable text editing** to change it. Text edits require **Apply**;
automatic application is disabled for those edits. Invalid numerical values
leave the running tracker intact. **Save configuration** validates and saves
the exact document, preserving comments, unknown fields and unrelated marks.
External file conflicts remain visible.

View preferences live beside the config in `.hejjetrac/VR1.txt.view.json`;
the opt-in atlas uses `.hejjetrac/VR1.txt.atlas.json`. Each name follows its
actual configuration filename. The startup manifest is `workspace.json` in the
per-user HejjéTrac configuration directory. Keep `.hejjetrac` with the rig when
moving it. Removing a view sidecar restores display defaults; removing an atlas
sidecar forces fresh learning. Neither action changes the calibration file.

## Shared grid controls and saved dimensions

Multiple rigs now share the rig grid, including small arrays. The default is the
nearest square column count: sixteen rigs use 4×4. Rig Manager's **Grid layout**
can select a column count and explicit rows (0 means automatic rows). A layout
needs at least one cell per rig. Save the workspace to keep its rig list and
restore it on graphical launch. Grid dimensions and the grid-window size are
remembered; each separately opened rig also remembers its window size beside its
configuration. Saving a config saves that view sidecar as well. Older sidecars
without dimensions still use defaults. Display/framebuffer limits apply on restore.

The reset tracking, full-frame/zoom, footage/debug, outline and recording icons
apply to all open rigs. Recording uses each rig's folder, raw/debug selection and
codec; stop waits for each worker to finalize its files. Each rig reports its own
errors. Compact tiles show VR, camera serial/number, ZMQ port and actual device
path when known, including whether the camera is bound. Rig Manager also shows
the USB hardware port, output readiness and output failures. The grid/settings
icon opens Rig Manager. Click a rig image once to expand it; double-click the
expanded image or use **Back to rig grid** to return. Each tile and manager row
has its own reset and settings buttons. Settings toggles the inspector open/closed,
and the inspector opens away from its launching controls. View choices
made for that rig remain selected.

**Reset tracking · all rigs** appears in both the grid toolbar and Rig Manager.
Each tile, expanded rig and manager row also has its own one-click reset.
It restarts pose, heading, path and tracking history while retaining source time,
source counters and integrated displacement. It requires every rig to be running
and outside calibration. Each rig reports its own queued completion or error.

In **Settings → Outputs**, choose **ZeroMQ + UDP**, **ZeroMQ**, or **UDP socket**.
Fresh rig defaults enable both. Switching to a single transport remembers its
custom ports for the next switch back to both. The selector has no disabled
choice; turning off the last ZeroMQ publisher enables UDP. Legacy configuration
files that intentionally disable outputs still load unchanged. A UDP bridge
that already owns the ZeroMQ port needs UDP output from HejjéTrac, with direct
ZeroMQ disabled, to avoid two publishers competing for the same port.

## Portable copies and other builds

In Rig Manager, choose **Rigs and workspace → Save workspace copy…** and select
a parent folder. The new folder contains every rig's configuration, view sidecars
and `workspace.json` with relative rig paths. Move or back up that whole folder.
Calibration, serial bindings and output choices travel with the configurations;
source videos and learned atlases are not copied. Video configurations retain
absolute paths to their existing source media.

In any compatible build, choose **Rigs and workspace → Load workspace…** and
select that copy's `workspace.json`. Save any current rig edits and finish
calibration/recordings first. All documents are validated before current sources
are released; unavailable cameras remain explicit errors on their own rigs.
Select **Restore on launch → Save workspace** to make the loaded setup this
device's startup default. The per-user configuration folder persists when the
application binary is replaced by a different build.

Finishing calibration closes the inspector and saves changes to an existing rig
configuration. Unsaved/temporary configurations still need their ordinary first
Save As. Disk conflicts and write failures preserve edits and remain visible;
there is no second Save calibration button.

Rig Manager also shows the automatically started local **Agent MCP** endpoint and
can copy its client connection JSON. [Agent setup and screenshot/input tools](../../docs/agent-control.md#automatic-desktop-mcp).
