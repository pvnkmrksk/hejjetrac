# Preview age, tracking rate and display cadence

## Additional four-camera cache check — 2026-11-05

The final 0.4.0 build's separate 35-second cold and 15-second cached-start
observations contain 972 sampled rig states. Pooled relative preview age is
**27.06 ms median**, **46.34 ms p95**, and **112.90 ms maximum**. Twelve samples
exceed 50 ms. The maximum is M02 at 29.46 seconds into the cold observation,
**during steady state**, not startup. Its preceding/following samples are
23.42 / 33.18 ms, about 0.23 / 0.21 seconds away. This is an isolated measured
tail with a prompt return to ordinary age, not evidence of a growing seconds-long
queue. The sampling does not identify the cause of the tail or establish a hard
50 ms latency ceiling. Actual four-camera throughput remains 100.72–101.17 FPS.

These observations start only after all rigs track, excluding earlier startup.
A cached process confirms all four rigs within 1.98 seconds of process launch;
that includes process, graphics, camera opening and first confirmation. Relative
worker image age does not measure physical exposure-to-photon latency, and
background tracking FPS does not establish UI paint responsiveness. The separate
native pointer tests measure actual painting.
[Counts, quantile definition, per-phase maxima and cache refresh evidence](benchmarks/live-cache-20261105.json).


The current atlas-learning build's **47° four-camera run on November 4** measures
**26.50 ms median**, **45.56 ms p95**, **105.74 ms maximum** over 2,824 steady-state
samples. Nine samples exceed 50 ms; one exceeds 100 ms. The maximum is M01,
144.25 seconds into the second three-minute phase. M02 has a simultaneous
97.25 ms preview age, with a spike in relative delivery delay and worker cost.
At the next sample about 0.51 seconds later, M01/M02 age is 14.17/13.66 ms and
relative backlog is 4.69/4.50 ms. Early/late medians stay similar. This supports
a transient stall rather than an accumulating queue; its exact cause and
sub-sample duration are not established. A hard 50 ms bound is not claimed.
The 30 FPS publication limit still yields about 25.37 preview updates/s while
tracking runs at about 101 frames/s. These are relative image-age clocks, not
absolute sensor-to-screen measurements.
[Current distribution and adjacent samples](benchmarks/atlas-preview-age-20261104.json) ·
[Current native report](benchmarks/atlas-live-four-20261104.json).

The earlier **47° four-camera run on November 4** measures **27.30 ms median**,
**44.84 ms p95**, **48.53 ms maximum** across 2796 samples. The maximum is M03 at
148.42 seconds into the first phase, with relative capture backlog at 4.34 ms;
the run-wide relative backlog maximum is 6.11 ms. Both three-minute phases remain
bounded across the four calibration cycles. [Final distribution](benchmarks/preview-age-20261104-final.json)
and [native hardware report](benchmarks/live-four-47-20261104-final.json) retain the
exact binary and timing scope. The cadence explanation below still applies.
Hand-driven recovery loss is a separate unresolved tracking limitation documented
in [validation](validation.md#fresh-four-camera-47-hardware-run--2026-11-04).

The earlier November 3 measurements are retained below for comparison.

The four-camera six-minute native run measured **27.11 ms median**, **44.92 ms
p95**, and **48.86 ms maximum** preview-image age across 2796 samples from the
two steady-state phases. The maximum was camera M03, **82.78 seconds into the
first phase**. It was not confined to startup.

| Camera | Median | p95 | Maximum |
|---|---:|---:|---:|
| M01 | 27.78 ms | 44.92 ms | 48.09 ms |
| M02 | 27.37 ms | 45.13 ms | 48.18 ms |
| M03 | 27.54 ms | 45.00 ms | 48.86 ms |
| M04 | 26.15 ms | 44.85 ms | 47.85 ms |

The display limit was 30 FPS while each numerical worker processed about
101 frames/s. The minimum publication interval is 33.33 ms, checked when source
frames arrive. With roughly 10 ms source intervals, publication commonly occurs
on the fourth arrival, about 40 ms later. The measured median preview rate is
**25.34 updates/s**. Source delivery/conversion, fitting and the time until an
image is published/read contribute to its age. At the maximum, the reported
worker cost was 3.84 ms and tracking cost 3.01 ms. Those are separate measurements,
not a precise additive decomposition of every image's delay.

This is expected scheduling at that display limit, but it is a real preview
delay. It does **not** establish exposure-to-screen latency: the clock is relative
to the fastest observed delivery and does not measure sensor/USB baseline latency
or the display's scanout. The numerical row/output cadence is independent of the
preview limit. A green FPS number alone cannot prove a current image; live image
age continues increasing if publication stops, with visible stale-image warnings.

The early/late distributions stay similar and relative capture backlog remains
bounded. That argues against an accumulating queue in this fixture; it does not
prove that every camera, encoder, machine or physical motion pattern is covered.
The earlier multi-second first-resume defect was separately reproduced and fixed.

[Distribution, per-camera values and early/late windows](benchmarks/preview-age-20261103.json)
and [complete native validation](benchmarks/live-regression-20261103.json)
retain the timing definitions and exact fixture/build scope. The native run used
ordinary Mesa NV136 GL graphics and all four real cameras; the captures show the
actual native application.

To reproduce the analysis from retained JSONL samples:

```sh
python3 scripts/analyze_preview_age.py runs/live-four-final-stress-20261103 \
  --output runs/preview-age-summary.json
```
