//! Preserve native V4L indices when device numbers contain holes.
use crate::{CameraInfo, Error, Result};

pub(crate) fn indexed_camera_infos(
    devices: impl IntoIterator<Item = (String, String)>,
) -> Result<Vec<CameraInfo>> {
    devices
        .into_iter()
        .map(|(index, name)| {
            let id = index.parse::<u32>().map_err(|_| {
                Error(format!(
                    "native V4L camera index {index:?} is not numeric; the camera chooser requires a backend exposing numeric V4L device IDs"
                ))
            })?;
            Ok(CameraInfo { id, name, hardware_id: None, port_id: None })
        })
        .collect()
}

/// USB descriptors identify the physical device; V4L's index distinguishes its
/// capture interfaces without incorporating a volatile bus address or /dev number.
#[cfg(target_os = "linux")]
#[derive(PartialEq, Eq)]
pub(crate) struct CameraIdentity {
    device_path: std::path::PathBuf,
    pub hardware_id: Option<String>,
    pub port_id: Option<String>,
}
#[cfg(target_os = "linux")]
pub(crate) fn camera_identity(id: u32) -> Option<CameraIdentity> {
    let path = std::path::Path::new("/sys/class/video4linux").join(format!("video{id}"));
    camera_identity_at(&path)
}
#[cfg(target_os = "linux")]
fn camera_identity_at(path: &std::path::Path) -> Option<CameraIdentity> {
    let device_path = std::fs::canonicalize(path).ok()?;
    Some(CameraIdentity {
        hardware_id: usb_hardware_id(&device_path),
        port_id: usb_port_id(&device_path),
        device_path,
    })
}
#[cfg(target_os = "linux")]
fn usb_port_id(video: &std::path::Path) -> Option<String> {
    let index = std::fs::read_to_string(video.join("index"))
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()?;
    let device = video.ancestors().find(|p| p.join("idVendor").exists())?;
    let ports = device.file_name()?.to_str()?.split_once('-')?.1;
    let root = device.ancestors().find(|p| {
        p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            n.strip_prefix("usb")
                .is_some_and(|n| n.parse::<u32>().is_ok())
        })
    })?;
    let controller = root.parent()?.file_name()?.to_str()?;
    let value = format!("usb-port:{controller}:{ports}:video:{index}");
    crate::validate_hardware_id(&value).ok().map(|()| value)
}

#[cfg(any(target_os = "linux", test))]
fn usb_hardware_id(video: &std::path::Path) -> Option<String> {
    let resolved = std::fs::canonicalize(video).ok()?;
    let index = std::fs::read_to_string(resolved.join("index")).ok()?;
    let index = index.trim().parse::<u32>().ok()?;
    for parent in resolved.ancestors() {
        if !parent.join("idVendor").exists() {
            continue;
        }
        let vendor = std::fs::read_to_string(parent.join("idVendor")).ok()?;
        let product = std::fs::read_to_string(parent.join("idProduct")).ok()?;
        let serial = std::fs::read_to_string(parent.join("serial")).ok()?;
        let (vendor, product, serial) = (vendor.trim(), product.trim(), serial.trim());
        if vendor.len() != 4
            || product.len() != 4
            || serial.is_empty()
            || !vendor
                .chars()
                .chain(product.chars())
                .all(|c| c.is_ascii_hexdigit())
        {
            return None;
        }
        let value = format!(
            "usb:{}:{}:{serial}:video:{index}",
            vendor.to_ascii_lowercase(),
            product.to_ascii_lowercase()
        );
        return crate::validate_hardware_id(&value).ok().map(|()| value);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn replacement_at_the_same_video_path_cannot_relabel_an_opened_identity()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "hejjetrac-usb-replacement-{}-{stamp}",
            std::process::id()
        ));
        let usb = root.join("0000:00:14.0/usb1/1-4.1.1");
        let video = usb.join("video4linux/video6");
        std::fs::create_dir_all(&video)?;
        for (key, value) in [
            ("idVendor", "0c45\n"),
            ("idProduct", "6366\n"),
            ("serial", "M01\n"),
        ] {
            std::fs::write(usb.join(key), value)?;
        }
        std::fs::write(video.join("index"), "0\n")?;
        let opened =
            camera_identity_at(&video).ok_or_else(|| Error("test identity unavailable".into()))?;
        std::fs::write(usb.join("serial"), "M04\n")?;
        let replacement = camera_identity_at(&video)
            .ok_or_else(|| Error("replacement identity unavailable".into()))?;
        assert!(opened != replacement);
        assert_eq!(
            opened.hardware_id.as_deref(),
            Some("usb:0c45:6366:M01:video:0")
        );
        assert_eq!(
            replacement.hardware_id.as_deref(),
            Some("usb:0c45:6366:M04:video:0")
        );
        assert_eq!(opened.port_id, replacement.port_id);
        assert_eq!(
            opened.port_id.as_deref(),
            Some("usb-port:0000:00:14.0:4.1.1:video:0")
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn usb_serial_survives_port_and_video_number_changes_and_requires_real_serial()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "hejjetrac-usb-identity-{}-{stamp}",
            std::process::id()
        ));
        let usb = root.join("port-1");
        let video = usb.join("video4linux/video0");
        std::fs::create_dir_all(&video)?;
        for (key, value) in [
            ("idVendor", "0c45\n"),
            ("idProduct", "6366\n"),
            ("serial", "UC762\n"),
        ] {
            std::fs::write(usb.join(key), value)?;
        }
        std::fs::write(video.join("index"), "0\n")?;
        let expected = "usb:0c45:6366:UC762:video:0";
        assert_eq!(usb_hardware_id(&video).as_deref(), Some(expected));
        let moved = root.join("port-9");
        std::fs::rename(&usb, &moved)?;
        let moved_video = moved.join("video4linux/video30");
        std::fs::rename(moved.join("video4linux/video0"), &moved_video)?;
        assert_eq!(usb_hardware_id(&moved_video).as_deref(), Some(expected));
        std::fs::write(moved_video.join("index"), "1\n")?;
        assert_ne!(usb_hardware_id(&moved_video).as_deref(), Some(expected));
        std::fs::remove_file(moved.join("serial"))?;
        assert!(usb_hardware_id(&moved_video).is_none());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn gapped_linux_devices_keep_actual_ids_and_names() -> Result<()> {
        let cameras = indexed_camera_infos([
            ("0".into(), "built-in".into()),
            ("2".into(), "left rig".into()),
            ("7".into(), "right rig".into()),
        ])?;
        assert_eq!(
            cameras.iter().map(|camera| camera.id).collect::<Vec<_>>(),
            [0, 2, 7]
        );
        assert_eq!(cameras[2].name, "right rig");
        Ok(())
    }

    #[test]
    fn unsupported_string_device_ids_never_become_fabricated_ordinals() {
        let error = indexed_camera_infos([
            ("0".into(), "built-in".into()),
            ("not-a-numeric-v4l-device".into(), "external".into()),
        ])
        .err();
        assert!(error.is_some_and(|error| error.to_string().contains("not numeric")));
    }
}
