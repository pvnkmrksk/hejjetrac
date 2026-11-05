# High-speed replay with the actual capture clock

The newest November 5 recording is the M02 camera's 9,339-image take under
`VR5/take-1793865353745717424`. The folder number is a desktop session number;
the saved document names VR2 and binds the camera to serial M02. The preceding
M02 take has 11,719 images. Both retain the same 47° calibration. The saved rim
is visually aligned in the decoded images; no geometry was changed to improve
the replay score.

## Reproduction and correction

The AVI declares 100 FPS. Its live quality sidecar records intervals alternating
near 8 and 12 ms, plus occasional longer intervals. Replaying only the AVI's
uniform 10 ms clock conceals a failure: with the recorded clock, image 171 was
rejected despite independently supported visible acceleration. The ensuing
relative-motion gap lasted about 1.27 seconds. Identical pixels with uniform
timing did not reproduce that first gap.

The acceleration guard previously demanded increasing raw angle/time rates
before fitting its four-image acceleration model. Normal timestamp variation
could reverse those rates while the visible angular steps increased. Within
the existing factor-of-1.8 interval boundary, two increasing measured steps can
now enter the same fit. Fitted speeds still must increase, with the original
component residual, direction, spatial image support and final-error checks.
Published timestamps and increments are not smoothed. Isolated jumps, reversed
motion and extended source gaps retain their rejection rules.

The previous-image retry also has a precision proposal that interpolates known
support in both the persistent atlas and the previous accepted image. It
minimizes their summed proposal cost. Both references constrain the pose, rather
than letting a single image steer a partly unseen first sweep. Candidate and
seed still use the complete nearest-cell temporal score for selection, and all
final acceptance gates remain. Published error remains the persistent binary
MSE, not the summed proposal. There are still at most two local optimizers,
each bounded by the configured `opt_max_evals`; the recording's quality six and
50-evaluation limit are retained.

The retained comparisons include attempts that improved this recording but
regressed the older 47° take. Those attempts were discarded. More accepted rows
alone do not establish improved accuracy or justify a release.

## Reproduce the evidence

Build the Rust development replay executable:

```sh
cargo build --release --locked -p fictrac-io --example replay_evidence
```

Use a copy of the recorded config with only experiment outputs/recording
disabled. For every row in `epoch-0000-all-frames.dat`, replace columns 22 and 25
with the full-precision timestamp from the corresponding quality CSV row. No
row is removed or moved. That clock file is a development input to:

```sh
target/release/examples/replay_evidence raw.avi config.txt 9339 replay.jsonl exact-clock.dat
```

The application uses the same core and native capture timestamps directly;
this conversion is needed because AVI's constant-rate stream does not carry
the sidecar's acquisition intervals. `scripts/analyze_capture_replay.py` checks
every decoded image, exact clock preservation, finite output and zero relative
motion on initialization/recovery rows. Its original-FicTrac comparisons include
all decisions, including zeros on rejected images.

The live recording starts with an already learned atlas. A replay starting
empty is therefore a reproduction and stress fixture, not a claim of exact
live/file state equivalence. The saved cache copied after recording is not its
initial atlas. Original-FicTrac agreement is separate from prescribed-motion
synthetic ground truth. The private raw takes remain local; published evidence
identifies their hashes rather than redistributing them.
