use anyhow::{Context, Result};
use std::path::Path;

/// Write `contents` to `path` atomically: write a temp file in the same
/// directory, then rename it over the target. A crash mid-write leaves either
/// the old file or the complete new file intact — never a truncated one.
///
/// The temp file lives in the same directory as the target so the rename stays
/// on one filesystem (cross-filesystem renames aren't atomic). rblxsync is
/// single-process, so a fixed `.<name>.tmp` temp name is sufficient.
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("rblxsync");
    let tmp = parent.join(format!(".{}.tmp", file_name));

    std::fs::write(&tmp, contents)
        .with_context(|| format!("Failed to write temp file {:?}", tmp))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("Failed to atomically replace {:?}", path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_overwrites_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("state.yml");

        atomic_write(&p, b"first").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "first");

        atomic_write(&p, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "second");

        // No leftover temp file should remain after a successful write.
        let temp_left = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().ends_with(".tmp"));
        assert!(!temp_left, "a .tmp file was left behind");
    }
}
