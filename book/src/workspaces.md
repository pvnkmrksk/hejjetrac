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
2. **Add saved rigs…** selects existing configuration files, including files
   from different folders. **New rig** creates another editable session. The
   limit is sixteen open rigs.
3. Open a rig's **Settings… → Source**. Select its camera or video. For live
   cameras, use the link icon to bind the exact physical identity, then save
   the configuration. A camera can be owned by only one session.
4. Choose **Calibration → Open graphical calibration…**. Review, edit or redraw
   the ball, exclusions and lab layers. Save the calibration. The guided editor
   uses the full image and restores the previous normal view afterward. Each
   physical rig needs its own fit.
5. In **Outputs**, assign distinct enabled ports. For example, UDP destinations
   1111–1126 and ZeroMQ bind ports 9871–9886 can serve sixteen rigs when those
   ports are free. Disable unused outputs. A bind address describes this tracking
   computer, rather than the receiving computer.
6. In **View**, choose the full source or focused ball, diagnostics and outlines.
   Those choices and preview FPS return on clean close/relaunch of the saved
   rig, without rewriting its calibration text.

For numbered files in one folder, **Start saved array** opens the existing
`VR1.txt`–`VR16.txt` or `BR1.txt`–`BR16.txt` siblings and reports failures
independently. **Start all** resumes paused rigs, retries stopped sources and
replays finished files. An array never replaces a failed camera with a sample.

## Fit every rig on screen

Finish or close calibration, then choose **Arrange in a grid**. Up to four rigs
keep independent native windows fitted to their image aspect ratios and the
current monitor's available space. Close or minimize the rig manager to see them.

**Five to sixteen rigs use one hardware-rendered Rig grid window.** Each tile
retains its independent source, tracker, calibration, outputs and saved view.
This bounds presentation to one GPU surface and avoids the observed XWayland /
Nouveau crash when creating many native surfaces on a hybrid-GPU desktop. The
app keeps hardware Vulkan rendering; it does not switch the grid to CPU graphics.
The rig manager and settings open inside this grid window too.

Choose **Open** on a tile for a larger view, diagnostics or graphical calibration;
choose **Grid** or **All rigs** to return to the complete grid. All other trackers
continue while one rig is expanded. Use a tile’s **More → Close this rig**
to remove it; unsaved edits offer Save, Discard or Cancel. Closing the first
rig leaves the grid and other trackers running. Closing the whole grid asks
before discarding any rig’s unsaved configuration text. Calibration uses the same source-space
editor and shared numerical commands as independent windows. Refresh, recording
and More remain available in compact tiles. Hover over the status for timing.
Images fit their own tile, so unused space can remain around the group when
source/debug aspect ratios differ. The optional terminal equivalent is:

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
