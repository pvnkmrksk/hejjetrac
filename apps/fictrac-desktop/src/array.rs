//! Saved rig planning. One unreadable/malformed entry never aborts its peers.
use std::path::{Path, PathBuf};

pub(crate) fn rig_path(anchor: &Path, slot: u32) -> Option<PathBuf> {
    if !(1..=16).contains(&slot) {
        return None;
    }
    let stem = anchor.file_stem()?.to_str()?;
    for prefix in ["VR", "BR"] {
        if stem
            .strip_prefix(prefix)
            .is_some_and(|n| n.parse::<u32>().is_ok())
        {
            return Some(
                anchor.with_file_name(format!("{prefix}{slot}.{}", anchor.extension()?.to_str()?)),
            );
        }
        let parent = anchor.parent()?;
        let name = parent.file_name()?.to_str()?;
        if name
            .strip_prefix(prefix)
            .is_some_and(|n| n.parse::<u32>().is_ok())
        {
            return Some(
                parent
                    .with_file_name(format!("{prefix}{slot}"))
                    .join(anchor.file_name()?),
            );
        }
    }
    None
}

pub(crate) fn slot(anchor: &Path) -> Option<u32> {
    (1..=16).find(|&i| rig_path(anchor, i).as_deref() == Some(anchor))
}

pub(crate) fn saved_paths(anchor: &Path) -> Vec<PathBuf> {
    let paths: Vec<_> = (1..=16)
        .filter_map(|i| rig_path(anchor, i))
        .filter(|p| p.is_file() || p == anchor)
        .collect();
    if paths.is_empty() {
        vec![anchor.to_owned()]
    } else {
        paths
    }
}

pub(crate) fn read_text(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut text = String::new();
    file.take(1_000_001)
        .read_to_string(&mut text)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if text.len() > 1_000_000 {
        return Err(format!("{}: config exceeds 1 MB", path.display()));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plans_all_saved_slots_without_parsing_or_skipping_a_broken_rig()
    -> Result<(), Box<dyn std::error::Error>> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hejjetrac-array-{stamp}"));
        std::fs::create_dir(&root)?;
        for i in [1, 2, 3, 16] {
            std::fs::write(
                root.join(format!("VR{i}.txt")),
                if i == 2 {
                    "invalid config"
                } else {
                    "src_fn:0\n"
                },
            )?;
        }
        let anchor = root.join("VR3.txt");
        assert_eq!(slot(&anchor), Some(3));
        let plan = saved_paths(&anchor);
        assert_eq!(plan, [1, 2, 3, 16].map(|i| root.join(format!("VR{i}.txt"))));
        assert!(read_text(&plan[1])?.contains("invalid"));
        assert_eq!(
            rig_path(&root.join("BR1/config.txt"), 16),
            Some(root.join("BR16/config.txt"))
        );
        assert_eq!(rig_path(&anchor, 4), Some(root.join("VR4.txt")));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
