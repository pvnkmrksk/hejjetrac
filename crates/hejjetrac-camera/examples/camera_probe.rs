//! Local hardware diagnostic; never opens a shell or stores camera pixels.
use std::{process::ExitCode, time::Instant};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let fps = std::env::args()
        .nth(1)
        .map(|s| s.parse::<f64>())
        .transpose()?
        .unwrap_or(30.0);
    let cameras = hejjetrac_camera::list_cameras()?;
    for camera in &cameras {
        println!("{}: {}", camera.id, camera.name);
    }
    hejjetrac_camera::request_permission()?;
    let explicit_device = std::env::var("HEJJETRAC_CAMERA_PROBE_DEVICE")
        .ok()
        .map(|s| s.parse::<u32>())
        .transpose()?;
    let device = if let Some(device) = explicit_device {
        device
    } else if let Ok(name) = std::env::var("HEJJETRAC_CAMERA_PROBE_NAME") {
        let matches = cameras
            .iter()
            .filter(|c| c.name.to_lowercase().contains(&name.to_lowercase()))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!("camera name {name:?} has {} matches; choose HEJJETRAC_CAMERA_PROBE_DEVICE explicitly", matches.len()).into());
        }
        matches[0].id
    } else {
        0
    };
    let request = hejjetrac_camera::CameraFormatRequest {
        width: std::env::var("HEJJETRAC_CAMERA_PROBE_WIDTH")
            .ok()
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(640),
        height: std::env::var("HEJJETRAC_CAMERA_PROBE_HEIGHT")
            .ok()
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(480),
        fps: Some(fps),
    };
    let controls = std::env::var_os("HEJJETRAC_CAMERA_PROBE_CONTROLS").is_some();
    let exact_format = controls
        || std::env::var_os("HEJJETRAC_CAMERA_PROBE_WIDTH").is_some()
        || std::env::var_os("HEJJETRAC_CAMERA_PROBE_HEIGHT").is_some();
    let mut camera = if exact_format {
        hejjetrac_camera::Camera::open_with_format(device, request)?
    } else {
        hejjetrac_camera::Camera::open(device, fps)?
    };
    println!("opened_source={}", camera.source_label());
    if controls {
        let original = camera.capabilities()?;
        println!("capabilities={}", serde_json::to_string(&original)?);
        for mode in &original.supported_exposure_modes {
            let result = camera
                .set_control(hejjetrac_camera::CameraControlRequest::ExposureMode { mode: *mode });
            // Restore the initial mode even if a later mode fails readback.
            if let Err(error) = result {
                if let Some(mode) = original.exposure_mode {
                    let _ = camera
                        .set_control(hejjetrac_camera::CameraControlRequest::ExposureMode { mode });
                }
                return Err(error.into());
            }
            println!(
                "exposure_mode_readback={:?}",
                camera.capabilities()?.exposure_mode
            );
        }
        if let Some(mode) = original
            .exposure_mode
            .filter(|mode| original.supported_exposure_modes.contains(mode))
        {
            camera.set_control(hejjetrac_camera::CameraControlRequest::ExposureMode { mode })?;
        }
        if original.supported_exposure_modes.is_empty() {
            for mode in [
                hejjetrac_camera::ExposureMode::Automatic,
                hejjetrac_camera::ExposureMode::Locked,
            ] {
                if camera
                    .set_control(hejjetrac_camera::CameraControlRequest::ExposureMode { mode })
                    .is_ok()
                {
                    return Err("unsupported exposure mode unexpectedly accepted".into());
                }
                println!("unsupported_exposure_mode_rejected={mode:?}");
            }
        }
        for control in [
            hejjetrac_camera::CameraControlRequest::Exposure { value: 0.001 },
            hejjetrac_camera::CameraControlRequest::Gain { value: 100.0 },
        ] {
            let writable = match control {
                hejjetrac_camera::CameraControlRequest::Exposure { .. } => {
                    original.exposure.writable
                }
                hejjetrac_camera::CameraControlRequest::Gain { .. } => original.gain.writable,
                _ => false,
            };
            if writable {
                println!("numeric_control_probe_skipped=writable; preserve current exposure/gain");
                continue;
            }
            let result = camera.set_control(control);
            println!("numeric_control_supported={}", result.is_ok());
            if let Err(error) = result {
                println!("numeric_control_error={error}");
            }
        }
    }
    let started = Instant::now();
    let mut frames = 0;
    let mut first = None;
    let mut last = None;
    while frames < 30 && started.elapsed().as_secs() < 10 {
        if let Some(frame) = camera.next_frame()? {
            frames += 1;
            if first.is_none() {
                println!(
                    "actual={}x{} first_pts_ms={}",
                    frame.gray.width(),
                    frame.gray.height(),
                    frame.gray.timestamp_ms()
                );
                first = Some(frame.gray.timestamp_ms());
            }
            last = Some(frame.gray.timestamp_ms());
        }
    }
    if frames != 30 {
        return Err(format!("only {frames} matching frames in ten seconds").into());
    }
    if controls {
        println!(
            "final_capabilities={}",
            serde_json::to_string(&camera.capabilities()?)?
        );
    }
    println!(
        "frames={frames} wall_seconds={:.6} pts_span_ms={:.6}",
        started.elapsed().as_secs_f64(),
        last.unwrap_or(0.0) - first.unwrap_or(0.0)
    );
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("camera probe: {e}");
            ExitCode::FAILURE
        }
    }
}
