//! Portable workspace copies contain rig text and view preferences, with relative rig paths.
use crate::{Desktop, array, session::Session, state};
use hejjetrac_io::{ConfigDocument, SourceSpec};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn read_workspace(path: &Path) -> Result<state::Workspace, String> {
    let mut workspace = state::read::<state::Workspace>(path, 65536)?
        .ok_or_else(|| format!("Workspace {} does not exist", path.display()))?;
    if workspace.rigs.is_empty()
        || workspace.rigs.len() > 16
        || workspace.columns.is_some_and(|v| !(1..=16).contains(&v))
        || workspace.rows.is_some_and(|v| !(1..=16).contains(&v))
        || workspace
            .columns
            .zip(workspace.rows)
            .is_some_and(|(c, r)| c * r < workspace.rigs.len())
        || workspace
            .grid_window_size
            .is_some_and(|v| !state::valid_window_size(v))
    {
        return Err("Workspace needs 1–16 rigs, a valid layout and a valid window size".into());
    }
    let parent = path.parent().ok_or("Workspace has no parent folder")?;
    for rig in &mut workspace.rigs {
        *rig = fs::canonicalize(if rig.is_absolute() {
            rig.clone()
        } else {
            parent.join(&*rig)
        })
        .map_err(|e| format!("Cannot load rig {}: {e}", rig.display()))?;
    }
    let unique: std::collections::BTreeSet<_> = workspace.rigs.iter().collect();
    if unique.len() != workspace.rigs.len() {
        return Err("Workspace contains the same rig configuration more than once".into());
    }
    Ok(workspace)
}

impl Desktop {
    pub(crate) fn export_workspace(&self, directory: &Path) -> Result<PathBuf, String> {
        let active: Vec<_> = self.sessions.iter().filter(|s| !s.closed).collect();
        if active.is_empty() || active.iter().any(|s| s.show_calibration) {
            return Err("Open a rig and finish calibration before saving a workspace copy".into());
        }
        let mut documents = vec![];
        for session in &active {
            let mut doc = session.document()?;
            if let SourceSpec::Video(path) = doc
                .source(session.path.as_deref().or(session.source_base.as_deref()))
                .map_err(|e| e.to_string())?
            {
                let path = fs::canonicalize(&path)
                    .map_err(|e| format!("Cannot resolve source {}: {e}", path.display()))?;
                doc.set("src_fn", &path.to_string_lossy())
                    .map_err(|e| e.to_string())?;
            }
            documents.push(doc);
        }
        fs::create_dir(directory)
            .map_err(|e| format!("Choose a new workspace folder {}: {e}", directory.display()))?;
        let mut workspace = self.workspace.clone();
        workspace.rigs.clear();
        for (i, (session, doc)) in active.iter().zip(documents).enumerate() {
            let relative = PathBuf::from(format!("VR{}.txt", i + 1));
            let path = directory.join(&relative);
            fs::write(&path, doc.text())
                .map_err(|e| format!("Cannot save {}: {e}", path.display()))?;
            state::write(&state::sidecar(&path, "view"), &session.view_preferences())?;
            workspace.rigs.push(relative);
        }
        let path = directory.join("workspace.json");
        state::write(&path, &workspace)?;
        Ok(path)
    }

    pub(crate) fn load_workspace(&mut self, path: &Path) -> Result<(), String> {
        if self.sessions.iter().any(|s| {
            !s.closed
                && (s.history.text != s.saved
                    || s.show_calibration
                    || s.snapshot.recording
                    || s.snapshot
                        .recording_finalizations
                        .iter()
                        .any(|v| matches!(v.state, "queued" | "finalizing")))
        }) {
            return Err("Save rig changes, finish calibration and stop/finalize recordings before loading a workspace".into());
        }
        let workspace = read_workspace(path)?;
        let mut pending = vec![];
        for (i, path) in workspace.rigs.iter().enumerate() {
            let doc = ConfigDocument::parse(array::read_text(path)?).map_err(|e| e.to_string())?;
            doc.validate_frontend()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let dimension = |key| {
                doc.get(key)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v >= 2)
            };
            let width = dimension("rust_camera_width").unwrap_or(320);
            let height = dimension("rust_camera_height").unwrap_or(240);
            hejjetrac_io::load_config_text(doc.text(), width, height)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if let SourceSpec::Video(source) = doc.source(Some(path)).map_err(|e| e.to_string())? {
                hejjetrac_io::VideoSource::open(&source)
                    .map_err(|e| format!("Cannot load source {}: {e}", source.display()))?;
            }
            let mut session = Session::new(i as u64 + 1, doc, Some(path.clone()));
            if let Some(first) = self.sessions.first() {
                session.framebuffer_limit = first.framebuffer_limit;
                session.max_window_inner_size = first.max_window_inner_size;
            }
            pending.push(session);
        }
        // Validate every document before releasing live sources. Device startup
        // failures then remain visible on their own rig rather than substituting input.
        self.sessions.clear();
        self.sessions = pending;
        self.workspace = workspace;
        self.next_id = self.sessions.len() as u64 + 1;
        self.dashboard_focus = None;
        self.dashboard_columns = self.workspace.columns;
        self.args.grid = None;
        self.tile_array_requested = true;
        for i in 0..self.sessions.len() {
            self.refresh_camera_occupancy();
            let result = self.sessions[i].start();
            self.sessions[i].report(result);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_paths_are_relative_to_the_selected_file_and_duplicates_fail() -> Result<(), String>
    {
        let dir = std::env::temp_dir().join(format!("hejjetrac-profile-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("workspace.json");
        fs::write(dir.join("VR1.txt"), "src_fn:0\n").map_err(|e| e.to_string())?;
        let mut workspace = state::Workspace {
            rigs: vec!["VR1.txt".into()],
            ..Default::default()
        };
        state::write(&path, &workspace)?;
        assert_eq!(
            read_workspace(&path)?.rigs,
            vec![fs::canonicalize(dir.join("VR1.txt")).map_err(|e| e.to_string())?]
        );
        workspace.rigs.push("VR1.txt".into());
        state::write(&path, &workspace)?;
        assert!(read_workspace(&path).is_err());
        fs::remove_dir_all(dir).map_err(|e| e.to_string())
    }
}
