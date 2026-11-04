# Appendix: an example camera and persistent serial numbers

A regular USB webcam can work when its exposure, delivered frame rate and image
quality suit the animal's motion. Arducam hardware and serial programming are
optional. For fast ball motion, prefer a **global shutter**, short exposure,
enough light and a lens that frames the ball sharply.

## Hardware used in the four-camera setup

Our example is the **Arducam OV9281 USB 2.0 monochrome global-shutter board,
B0332 family**, with **UC-844 Rev B** printed on the tested boards. See the
[vendor's B0332 product listing](https://www.uctronics.com/arducam-120fps-global-shutter-usb-camera-board-1mp-720p-ov9281-uvc-webcam-module-with-low-distortion-m12-lens.html)
and [manufacturer datasheet](https://www.uctronics.com/download/Amazon/B0332_OV9281_Global_Shutter_UVC_Camera_Datasheet.pdf).
The listing was US$49.99 before tax/shipping when checked on 4 November 2026;
price and availability can change.

| Item | Reproducibility detail |
|---|---|
| Sensor | OV9281, monochrome, global shutter, 1280×800 native pixels |
| Connection | USB 2.0, standard UVC; choose the USB version, not a MIPI/CSI module |
| Speed | Vendor's current B0332 listing specifies MJPEG at 100 FPS; older material advertises up to 120 FPS. Our four-camera test requests 320×240 at 100 FPS |
| Size and mounting | Approximately 38×38 mm; mounting-hole patterns are documented as 34×34 and 28×28 mm. A small bracket/spacers can secure the bare board and relieve USB-cable strain |
| Lens interface | M12×0.5. Choose the focal length, working distance and filter for the experiment, then measure vertical FOV and refit calibration |
| Tested identities | USB VID/PID `0c45:6366`, board revision UC-844 Rev B; user-programmed serials M01, M02, M03, M04 |

The compact board, mounting holes, global shutter and relatively low price make
this a practical example for several rigs. The M12 interface permits different
optical configurations, but **confirm a replaceable/unlocked lens holder for the
exact order**: the current vendor listing explicitly notes that some supplied
lenses are fixed and asks buyers to arrange replacements in advance. Record the
actual lens SKU/focal length and measured FOV; the stock horizontal-FOV number
is not HejjéTrac's vertical `vfov`. We do not infer a precise lens SKU from USB
metadata. See [experimental setup](hardware.md) for lighting, focus and FOV.

## Optional one-time serial programming on Windows

Distinct serials help when identical cameras run simultaneously and USB device
numbers change. If cameras already have unique persistent IDs, leave them as
they are. Numeric selection also works; explicit Linux USB-socket binding is an
alternative when a device has no unique serial.

Use Arducam's [official UVC configuration guide](https://docs.arducam.com/UVC-Camera/UVC-Camera-Configuration-Guide/#option-2-serial-number-tool),
**Option 2: Serial Number Tool for USB 2.0**. Its supported-device list includes
OV9281/B0332. The USB 3.0 Config Tool is a different utility. Obtain the download
from that guide so the tool matches the camera/batch.

1. Extract the Windows utility completely; keep its accompanying DLLs beside
   the executable. Close camera applications and connect one external Arducam.
2. Select that camera in the tool's **Port** list. Click **Read** and retain the
   original serial and board/batch information.
3. Enter a unique serial such as `M01`, click **Write**, and wait for success.
4. Unplug and reconnect the camera, then **Read** again. Move it to another USB
   socket and verify the same serial. Label the physical camera only after this
   readback succeeds.
5. Repeat one camera at a time with `M02`, `M03`, `M04`, or your own unique scheme.
6. Reconnect all cameras on the tracking computer and run `hejjetrac --camera-list`.
   Check that every serial identifies exactly one capture interface. In the
   tested Linux setup, M01 appears as `usb:0c45:6366:M01:video:0`: its device
   number was 6 on November 3 and 0 on November 4, while the binding stayed the same.

The setup owner completed this provisioning in Windows; Linux readback and the
four-camera release tests verify M01–M04. HejjéTrac itself does not write camera
firmware. Tool support varies by batch: if **Read** cannot identify the camera,
use a compatible vendor utility rather than trying a different model's firmware.
Arducam documents [B0332 tool-version compatibility reports](https://forum.arducam.com/t/serial-number-change-tool-no-longer-supports-b0332-cameras/12946).

## Bind and run the rigs

In **Source & camera**, select the intended serial and click the link icon.
Save one config per physical rig, with its own calibration and output ports.
Unlink keeps the currently open camera. Missing or duplicated saved IDs stop the
affected rig instead of opening another device or a demonstration video.

```text
# Example VR1.txt source fields; use your own calibration and output ports.
src_fn         : 6
rust_camera_id : usb:0c45:6366:M01:video:0
```

`rust_camera_id` follows the camera; `src_fn` is the volatile device number.
Linux serial binding has four-camera hardware validation. Windows can provision
the camera's serial, but the current Windows capture adapter does not expose a
binding ID; do not assume that programming alone enables application binding on
every OS. [Several simultaneous rigs](rigs.md) covers launch, ports and bandwidth.

The current M01–M04 rig uses **47° vertical FOV** at 320×240, as supplied by the rig owner. The shipped rig files refit angular ball geometry at that angle. This is not a universal OV9281 lens specification.
