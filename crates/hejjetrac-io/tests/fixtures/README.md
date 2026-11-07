`h264-annexb.avi` contains the first ten compressed video frames from the attributed FicTrac `examples/sample.mp4`. It was remuxed, without transcoding, with FFmpeg as an independent development fixture:

```
ffmpeg -v error -i examples/sample.mp4 -frames:v 10 -an -c:v copy -bsf:v h264_mp4toannexb -r 30 -fps_mode passthrough h264-annexb.avi
```

Application code and tests never invoke FFmpeg. The Rust AVI adapter's decoded RGB must exactly equal the corresponding Rust MP4 frames. The source and its CC BY-NC-SA 3.0 attribution in the repository root apply to this derivative fixture.

The cut is at a complete B-picture group: these ten compressed packets correspond to presentation pictures 0–9. A cut at eight compressed packets would include picture 8 while omitting picture 7.

`lab-rigs/VR1.txt` through `VR4.txt` freeze the repository's calibrated 47-degree
rig documents from commit `ab6b564`. Their bytes, fitted-rim tolerances and
per-rig port/source expectations are unchanged. Configuration regression tests
use these copies because repository-root rig files can contain the user's live
calibration edits. Fresh release defaults are tested separately under
`examples/defaults` and never inherit these hardware bindings.
