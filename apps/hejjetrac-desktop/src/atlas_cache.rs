//! Opt-in, per-rig cache. Identity and full numerical configuration must match.
use hejjetrac_core::{Config, SphereTemplate, TrackRow, Tracker};
use hejjetrac_io::ConfigDocument;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedAtlas {
    version: u32,
    identity: String,
    config: String,
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    pose_camera: [f64; 3],
    source_frame: u64,
}
fn signature(config: &Config) -> Result<String, String> {
    let mut config = config.clone();
    config.source = None;
    serde_json::to_string(&config).map_err(|e| e.to_string())
}
pub fn identity(doc: &ConfigDocument, source_label: &str) -> Result<String, String> {
    let binding = doc.camera_binding().map_err(|e| e.to_string())?;
    if doc
        .get("src_fn")
        .is_some_and(|source| source.parse::<u32>().is_ok())
        && binding.is_none()
    {
        return Err("Link this rig to its physical camera before reusing an atlas".into());
    }
    if let Some(binding) = binding {
        return Ok(binding.to_owned());
    }
    let file = std::fs::metadata(source_label)
        .map_err(|e| format!("Cannot identify atlas source {source_label}: {e}"))?;
    let modified = file
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "{source_label}|{}|{}",
        file.len(),
        modified.as_nanos()
    ))
}
pub fn load(
    path: &Path,
    tracker: &mut Tracker,
    config: &Config,
    identity: &str,
) -> Result<bool, String> {
    let Some(saved) = crate::state::read::<SavedAtlas>(path, 2_000_000)? else {
        return Ok(false);
    };
    if saved.version != 1 || saved.identity != identity || saved.config != signature(config)? {
        return Err("Saved atlas belongs to another source or calibration. Starting with an empty atlas; Refresh atlas replaces the old cache after fresh tracking.".into());
    }
    let template =
        SphereTemplate::new(saved.width, saved.height, saved.pixels).map_err(|e| e.to_string())?;
    tracker
        .load_sphere_template_at_pose(template, saved.pose_camera)
        .map_err(|e| e.to_string())?;
    Ok(true)
}
pub fn save(
    path: &Path,
    tracker: &Tracker,
    config: &Config,
    identity: &str,
    row: &TrackRow,
) -> Result<bool, String> {
    if !row.accepted || row.status != "tracked" {
        return Ok(false);
    }
    let map = tracker
        .sphere_template()
        .ok_or("No spherical atlas is available")?;
    map.validate().map_err(|e| e.to_string())?;
    crate::state::write(
        path,
        &SavedAtlas {
            version: 1,
            identity: identity.into(),
            config: signature(config)?,
            width: map.width,
            height: map.height,
            pixels: map.pixels,
            pose_camera: row.absolute_camera,
            source_frame: row.frame,
        },
    )?;
    Ok(true)
}
pub fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "Cannot replace saved atlas {}: {e}",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_video_cache_requires_matching_identity_and_calibration()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut video = hejjetrac_io::VideoSource::open(&root.join("examples/sample.mp4"))?;
        let doc = ConfigDocument::parse(std::fs::read_to_string(
            root.join("examples/upstream-config.txt"),
        )?)?;
        let config =
            hejjetrac_io::load_config_text(doc.text(), video.info.width, video.info.height)?.config;
        let mut original = Tracker::new(config.clone())?;
        let mut last = None;
        for _ in 0..80 {
            last = Some(original.process(video.next_tracking_frame(None)?.ok_or("short sample")?)?);
        }
        let row = last.ok_or("no source rows")?;
        let directory =
            std::env::temp_dir().join(format!("hejjetrac-atlas-cache-{}", std::process::id()));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("last.json");
        assert!(save(&path, &original, &config, "same-video", &row)?);
        let bytes = std::fs::read(&path)?;
        let mut refused = row.clone();
        refused.accepted = false;
        assert!(!save(&path, &original, &config, "same-video", &refused)?);
        assert_eq!(std::fs::read(&path)?, bytes);
        let mut resumed = Tracker::new(config.clone())?;
        let before = resumed.sphere_template();
        assert!(load(&path, &mut resumed, &config, "different-camera").is_err());
        let mut changed = config.clone();
        changed.vfov_deg += 0.1;
        assert!(load(&path, &mut resumed, &changed, "same-video").is_err());
        assert_eq!(resumed.sphere_template(), before);
        assert!(load(&path, &mut resumed, &config, "same-video")?);
        let mut tracked = 0;
        let mut squared = glam::DVec3::ZERO;
        for i in 0..50 {
            let frame = video
                .next_tracking_frame(None)?
                .ok_or("short resumed sample")?;
            let expected = original.process(frame.clone())?;
            let row = resumed.process(frame)?;
            if i == 0 {
                assert!(!row.accepted);
            }
            if row.status == "tracked" {
                assert!(expected.accepted);
                let error = glam::DVec3::from_array(row.delta_camera)
                    - glam::DVec3::from_array(expected.delta_camera);
                squared += error * error;
                tracked += 1;
            } else {
                assert_eq!(row.delta_camera, [0.0; 3]);
            }
        }
        assert!(
            tracked >= 40,
            "cached sample reacquisition measured only {tracked}/50 images"
        );
        let rms = (squared / tracked as f64).to_array().map(f64::sqrt);
        assert!(
            rms.iter().all(|r| *r < 0.020),
            "real sample cached-increment RMS {rms:?}"
        );
        remove(&path)?;
        assert!(!load(&path, &mut resumed, &config, "same-video")?);
        std::fs::remove_dir_all(directory)?;
        Ok(())
    }
}
