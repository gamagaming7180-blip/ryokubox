//! Safe, approval-gated shortcuts for Ryoku Box. No shell is ever involved.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A shortcut runs one program with explicit arguments. `approved` gates execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shortcut {
    pub id: String,
    pub label: String,
    pub argv: Vec<String>,
    pub approved: bool,
}

impl Shortcut {
    /// True when this shortcut is safe to hand to `tokio::process::Command`.
    pub fn runnable(&self) -> bool {
        self.approved && !self.argv.is_empty() && !self.argv[0].is_empty()
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".config/ryokubox/shortcuts.json")
}

/// Atomically replace the persisted list: write a sibling temp file, then rename.
pub fn save_shortcuts(path: &Path, list: &[Shortcut]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.to_path_buf();
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("shortcuts.json");
    tmp.set_file_name(format!(".{name}.tmp"));
    let bytes = serde_json::to_vec_pretty(list)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_shortcuts(path: &Path) -> Vec<Shortcut> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<Shortcut>>(&b).ok())
        .unwrap_or_default()
}

pub fn configured_path() -> PathBuf {
    config_path()
}

/// Run only an approved shortcut, as a direct argv vector (never `sh -c`).
pub fn spawn_shortcut(sc: &Shortcut) -> std::io::Result<()> {
    if !sc.runnable() {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "shortcut not approved"));
    }
    tokio::process::Command::new(&sc.argv[0])
        .args(&sc.argv[1..])
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Shortcut> {
        vec![
            Shortcut { id: "term".into(), label: "Terminal".into(),
                argv: vec!["kitty".into()], approved: true },
            Shortcut { id: "note".into(), label: "Notes".into(),
                argv: vec!["kate".into(), "notes.md".into()], approved: false },
        ]
    }

    #[test]
    fn atomic_save_roundtrips_exact_order_and_flags() {
        let dir = std::env::temp_dir().join(format!("ryokubox-sc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shortcuts.json");
        let list = sample();
        save_shortcuts(&path, &list).unwrap();
        let loaded = load_shortcuts(&path);
        assert_eq!(loaded, list, "atomic save must preserve order and approval flags");
        assert!(!dir.join(".shortcuts.json.tmp").exists(), "temp file must be gone after rename");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unapproved_or_empty_shortcuts_are_not_runnable() {
        let list = sample();
        assert!(list[0].runnable());
        assert!(!list[1].runnable());
        let empty = Shortcut { id: "x".into(), label: "x".into(), argv: vec![], approved: true };
        assert!(!empty.runnable());
    }
}
