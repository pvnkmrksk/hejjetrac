use crate::{CameraInfo, Error, Result};

pub fn validate_hardware_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 512
        || value.trim() != value
        || value.chars().any(|c| c.is_control() || c == '#')
    {
        return Err(Error("hardware ID must be 1..512 bytes without surrounding whitespace, control characters or #; copy it from the camera list".into()));
    }
    Ok(())
}

/// Exact matching only: never substitute another camera or an enumeration index.
pub fn resolve_camera(cameras: &[CameraInfo], hardware_id: &str) -> Result<u32> {
    validate_hardware_id(hardware_id)?;
    let mut matches = cameras.iter().filter(|c| {
        c.hardware_id.as_deref() == Some(hardware_id) || c.port_id.as_deref() == Some(hardware_id)
    });
    let camera = matches.next().ok_or_else(|| Error(format!(
        "required camera {hardware_id:?} is missing; connect it or explicitly unlink this rig's camera binding"
    )))?;
    if matches.next().is_some() {
        return Err(Error(format!(
            "hardware ID {hardware_id:?} is duplicated by multiple capture devices; this rig cannot start until the ambiguity is resolved"
        )));
    }
    Ok(camera.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_burned_serials_follow_devices_and_never_fall_back_to_rig_order() -> Result<()> {
        let mut cameras = [(0, "M04"), (2, "M02"), (4, "M03"), (6, "M01")]
            .into_iter()
            .map(|(id, serial)| CameraInfo {
                id,
                name: "Arducam OV9281 USB Camera".into(),
                hardware_id: Some(format!("usb:0c45:6366:{serial}:video:0")),
                port_id: Some(format!("usb-port:0000:00:14.0:4.1.{id}:video:0")),
            })
            .collect::<Vec<_>>();
        let binding = |serial| format!("usb:0c45:6366:{serial}:video:0");
        for (serial, id) in [("M01", 6), ("M02", 2), ("M03", 4), ("M04", 0)] {
            assert_eq!(resolve_camera(&cameras, &binding(serial))?, id);
        }
        // A camera reconnect can change both its USB socket and capture node.
        cameras[3].id = 12;
        cameras[3].port_id = Some("usb-port:0000:00:14.0:9.2:video:0".into());
        cameras.reverse();
        assert_eq!(resolve_camera(&cameras, &binding("M01"))?, 12);
        // The other three cameras must never substitute for a missing M01,
        // even if one now occupies M01's old numeric source.
        cameras.retain(|camera| camera.id != 12);
        cameras[0].id = 6;
        assert!(resolve_camera(&cameras, &binding("M01")).is_err());
        assert!(resolve_camera(&cameras, "M01").is_err());
        let mut duplicate = cameras[0].clone();
        duplicate.id = 14;
        let duplicate_id = duplicate
            .hardware_id
            .clone()
            .ok_or_else(|| Error("missing test camera identity".into()))?;
        cameras.push(duplicate);
        assert!(resolve_camera(&cameras, &duplicate_id).is_err());
        Ok(())
    }
    #[test]
    fn duplicate_serials_fail_closed_but_explicit_usb_socket_bindings_are_distinct() -> Result<()> {
        let cameras: Vec<_> = (0..2)
            .map(|i| CameraInfo {
                id: i * 2,
                name: "Arducam OV9281".into(),
                hardware_id: Some("usb:0c45:6366:UC762:video:0".into()),
                port_id: Some(format!("usb-port:0000:00:14.0:4.1.{}:video:0", i + 3)),
            })
            .collect();
        assert!(resolve_camera(&cameras, "usb:0c45:6366:UC762:video:0").is_err());
        for camera in &cameras {
            assert_eq!(
                resolve_camera(
                    &cameras,
                    camera
                        .port_id
                        .as_deref()
                        .ok_or_else(|| Error("test socket missing".into()))?
                )?,
                camera.id
            );
        }
        Ok(())
    }
    #[test]
    fn sixteen_identical_models_resolve_by_identity_after_reordering() -> Result<()> {
        let mut cameras: Vec<_> = (0..16)
            .map(|i| CameraInfo {
                id: 30 - i * 2,
                name: "same model".into(),
                hardware_id: Some(format!("usb:0c45:6366:UC{i}")),
                port_id: None,
            })
            .collect();
        cameras.reverse();
        for i in 0..16 {
            assert_eq!(
                resolve_camera(&cameras, &format!("usb:0c45:6366:UC{i}"))?,
                30 - i * 2
            );
        }
        cameras.retain(|c| c.hardware_id.as_deref() != Some("usb:0c45:6366:UC0"));
        assert!(resolve_camera(&cameras, "usb:0c45:6366:UC0").is_err());
        assert_eq!(resolve_camera(&cameras, "usb:0c45:6366:UC3")?, 24);
        cameras.push(cameras[0].clone());
        assert!(
            resolve_camera(
                &cameras,
                cameras[0]
                    .hardware_id
                    .as_deref()
                    .ok_or_else(|| Error("missing test ID".into()))?
            )
            .is_err()
        );
        for value in [
            "",
            " serial",
            "serial\n",
            "serial#comment",
            &"x".repeat(513),
        ] {
            assert!(validate_hardware_id(value).is_err());
        }
        Ok(())
    }
}
