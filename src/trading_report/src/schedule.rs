//! One-shot scheduled collection helpers. No resident loop.

use anyhow::{bail, Context, Result};
use fs4::fs_std::FileExt;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub const LOCK_FILE: &str = ".collect.lock";
pub const LATEST_NAME: &str = "latest";

pub struct CollectLock {
    file: File,
}

impl Drop for CollectLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub fn try_acquire_lock(reports_dir: &Path) -> Result<Option<CollectLock>> {
    fs::create_dir_all(reports_dir)?;
    let path = reports_dir.join(LOCK_FILE);
    let file = File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    match file.try_lock_exclusive() {
        Ok(true) => Ok(Some(CollectLock { file })),
        Ok(false) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e).with_context(|| format!("lock {}", path.display())),
    }
}

pub fn unique_utc_dir(reports_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(reports_dir)?;
    let stamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string();
    let candidate = reports_dir.join(&stamp);
    if !candidate.exists() {
        fs::create_dir_all(&candidate)?;
        return Ok(candidate);
    }
    let extra = uuid::Uuid::new_v4().simple().to_string();
    let dir = reports_dir.join(format!("{stamp}-{extra}"));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Point `latest` at this run only after files and Mongo both succeeded.
/// An unverified report may be latest. An older verified report must not stay current.
pub fn publish_latest(reports_dir: &Path, unique_dir: &Path) -> Result<()> {
    let name = unique_dir
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("output directory has no name"))?;
    if unique_dir.parent() != Some(reports_dir) {
        bail!("latest pointer requires the output directory to live under the reports root");
    }
    let latest = reports_dir.join(LATEST_NAME);
    let tmp = reports_dir.join(".latest.tmp");
    let _ = fs::remove_file(&tmp);
    std::os::unix::fs::symlink(name, &tmp).with_context(|| format!("symlink {}", tmp.display()))?;
    fs::rename(&tmp, &latest).with_context(|| format!("publish {}", latest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_follows_the_newest_directory_not_verdict() {
        let root = tempfile::tempdir().unwrap();
        let older = unique_utc_dir(root.path()).unwrap();
        std::fs::write(older.join("report.json"), "{\"verdict\":\"verified\"}").unwrap();
        publish_latest(root.path(), &older).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let newer = unique_utc_dir(root.path()).unwrap();
        std::fs::write(newer.join("report.json"), "{\"verdict\":\"unverified\"}").unwrap();
        publish_latest(root.path(), &newer).unwrap();
        let latest = root.path().join(LATEST_NAME);
        let target = std::fs::read_link(&latest).unwrap();
        assert_eq!(target, newer.file_name().unwrap());
        let body = std::fs::read_to_string(latest.join("report.json")).unwrap();
        assert!(body.contains("unverified"));
    }

    #[test]
    fn second_lock_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let first = try_acquire_lock(root.path()).unwrap();
        assert!(first.is_some());
        let second = try_acquire_lock(root.path()).unwrap();
        assert!(second.is_none());
        drop(first);
        let third = try_acquire_lock(root.path()).unwrap();
        assert!(third.is_some());
    }
}
