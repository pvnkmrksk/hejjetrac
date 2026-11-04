# v0.3.6 — camera freshness, exact bindings and calibration editing

Live camera acquisition continues draining during a calibration hold. Resuming
uses fresh images and explicitly reanchors tracking after discarded frames,
retaining the atlas and accumulated motion without inventing motion across the
gap. Exact serial bindings fail closed when missing or ambiguous; link/unlink
preserves the opened physical camera. Image age is visible beside the FPS rate.

Saved ball marks now select from the original pointer press, fixing fast drags
that previously missed the mark. Calibration offers a direct Edit action. The
M01–M04 rig files use the owner's **47° vertical FOV**, with angular sphere
geometry refitted at 320×240. M03's misplaced historical rim marks were replaced
using the retained real camera image. Existing lab orientation and exclusions
remain physically unverified after this refit.

![M03 ball fit in the native saved-mark editor](images/calibration-47-M03-20261104.png)

[M01](images/calibration-47-M01-20261104.png) ·
[M02](images/calibration-47-M02-20261104.png) ·
[M04](images/calibration-47-M04-20261104.png) ·
[Calibration controls](images/calibration-controls-20261104.png) ·
[Native screen capture of recorded camera replay](images/native-camera-replay-20261104.mp4).
These November 4 captures use the retained real 100 FPS camera recordings,
ordinary NV136 Mesa graphics and the native app. No live camera devices were
enumerated in this session. The earlier six-minute four-camera hardware stress
run is separately documented in [validation](validation.md).

All four complete retained camera takes also process with the new 47° fits:
**1281 images**, four initialization rows and **1277 tracked rows**, with no
rejections. Airflow jitter supplies observed motion, without physical ground
truth. [Calibration evidence](benchmarks/calibration-47-20261104.json) retains
the pixel points, angular fits and exact native binary hash.

Required checks pass: formatting, strict locked workspace/all-target Clippy and
**331 workspace tests** (zero failures or ignored tests). The release synthetic
demo and real sample pass; existing scientific tolerances and oracles are unchanged.

Preview-image age was **27.11 ms median**, **44.92 ms p95**, **48.86 ms maximum**
in the earlier sustained hardware run. The maximum occurred after 82.78 seconds,
not just at startup. Read the [timing definitions and cadence explanation](preview-age.md)
before interpreting these relative measurements as sensor-to-screen latency.

The [optional camera appendix](../book/src/appendix-cameras.md) identifies the
OV9281 Arducam hardware family, global shutter, lens/mount qualifications and
Windows serial-number programming instructions. Ordinary webcams can also be
used. The [multi-camera configuration guide](../book/src/configuration.md#optional-camera-binding)
explains binding independently of USB enumeration order.

Publishing native release assets remains pending GitHub authentication. This
document describes local validation; it is not evidence of a published release
or native execution on other operating systems.
