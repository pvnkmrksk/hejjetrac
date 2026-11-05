//! Small sidecars keep presentation/startup choices out of legacy config text.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ViewPreferences {
    pub full_image: bool,
    pub diagnostics: bool,
    pub overlays: bool,
    pub magnifier: bool,
    pub display_fps: u32,
    pub reuse_atlas: bool,
}
impl Default for ViewPreferences {
    fn default() -> Self {
        Self {
            full_image: false,
            diagnostics: false,
            overlays: false,
            magnifier: true,
            display_fps: 30,
            reuse_atlas: false,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Workspace {
    pub restore_on_launch: bool,
    pub rigs: Vec<PathBuf>,
}
pub fn sidecar(config: &Path, kind: &str) -> PathBuf {
    let name = config.file_name().unwrap_or_default().to_string_lossy();
    config
        .parent()
        .unwrap_or(Path::new("."))
        .join(".hejjetrac")
        .join(format!("{name}.{kind}.json"))
}
pub fn read<T: DeserializeOwned>(path: &Path, maximum: u64) -> Result<Option<T>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot open {}: {e}", path.display())),
    };
    let mut bytes = vec![];
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.len() as u64 > maximum {
        return Err(format!(
            "{} exceeds its {maximum}-byte limit",
            path.display()
        ));
    }
    serde_json::from_slice(&bytes).map(Some).map_err(|e| format!("Cannot read {}: {e}. Move this sidecar aside to restore defaults; calibration text is unchanged.", path.display()))
}
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("The state file has no parent folder")?;
    fs::create_dir_all(parent).map_err(|e| {
        format!(
            "Cannot create {}: {e}. Save the rig in a writable folder.",
            parent.display()
        )
    })?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(
        ".state-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| {
        format!(
            "Cannot save {}: {e}. Save the rig in a writable folder.",
            path.display()
        )
    })
}
pub fn workspace_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("HEJJETRAC_STATE_DIR") {
        return Ok(PathBuf::from(path).join("workspace.json"));
    }
    Ok(crate::startup::user_config_path()?.with_file_name("workspace.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sidecar_roundtrip_is_bounded_and_preserves_config() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "hejjetrac-view-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let config = dir.join("VR1.txt");
        fs::write(&config, "# keep exact\nsrc_fn : 0\n").map_err(|e| e.to_string())?;
        let path = sidecar(&config, "view");
        let view = ViewPreferences {
            full_image: true,
            diagnostics: true,
            reuse_atlas: true,
            ..Default::default()
        };
        write(&path, &view)?;
        assert_eq!(read::<ViewPreferences>(&path, 4096)?, Some(view.clone()));
        write(&path, &ViewPreferences::default())?;
        assert_eq!(
            read::<ViewPreferences>(&path, 4096)?,
            Some(ViewPreferences::default())
        );
        assert!(read::<ViewPreferences>(&path, 4).is_err());
        assert_eq!(
            fs::read_to_string(&config).map_err(|e| e.to_string())?,
            "# keep exact\nsrc_fn : 0\n"
        );
        fs::remove_dir_all(dir).map_err(|e| e.to_string())
    }
}
