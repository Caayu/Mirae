//! Replacing a file atomically: the new content is written to a temporary
//! file next to the target (same directory, so the same file system),
//! flushed and synced, and only then renamed over the target. A failure at
//! any point before the rename leaves the old file exactly as it was and
//! removes the temporary file; the rename itself replaces the target in
//! one step (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` on Windows,
//! `rename(2)` elsewhere).

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::IoError;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A temporary file next to `target` that does not exist yet.
fn create_temp(target: &Path) -> Result<(PathBuf, File), IoError> {
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| {
            IoError::Filesystem(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the path has no file name",
            ))
        })?;
    let pid = std::process::id();
    let mut last = None;
    for _ in 0..64 {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(".{name}.{pid}-{n}.tmp"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(e.into()),
        }
    }
    Err(last
        .unwrap_or_else(|| std::io::Error::other("no temporary name"))
        .into())
}

/// Writes `target` through `write`, atomically (see the module docs).
pub(crate) fn write_file(
    target: &Path,
    write: impl FnOnce(&mut BufWriter<File>) -> Result<(), IoError>,
) -> Result<(), IoError> {
    let (temp, file) = create_temp(target)?;
    let result = (|| {
        let mut w = BufWriter::with_capacity(1 << 20, file);
        write(&mut w)?;
        w.flush()?;
        let file = w.into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, target)?;
        Ok(())
    })();
    if result.is_err() {
        // Best effort: the temporary file is ours and holds nothing useful.
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tela-io-atomic-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn replaces_the_target_only_when_everything_succeeded() {
        let dir = scratch("replace");
        let target = dir.join("doc.mirae");
        std::fs::write(&target, b"old").unwrap();
        write_file(&target, |w| Ok(w.write_all(b"new content")?)).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new content");
        assert!(leftovers(&dir).is_empty());
        // A failure half-way through keeps the old file and cleans up.
        let failed = write_file(&target, |w| {
            w.write_all(&[7; 3 << 20])?;
            Err(IoError::Unsupported("simulated failure".to_owned()))
        });
        assert!(failed.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"new content");
        assert!(leftovers(&dir).is_empty(), "temporary file removed");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn creates_new_files_and_reports_impossible_targets() {
        let dir = scratch("new");
        let target = dir.join("fresh.mirae");
        write_file(&target, |w| Ok(w.write_all(b"x")?)).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"x");
        // A directory in the way: the rename fails, nothing is lost.
        let blocked = dir.join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("inside"), b"keep").unwrap();
        assert!(matches!(
            write_file(&blocked, |w| Ok(w.write_all(b"x")?)),
            Err(IoError::Filesystem(_))
        ));
        assert_eq!(std::fs::read(blocked.join("inside")).unwrap(), b"keep");
        assert!(leftovers(&dir).is_empty());
        // A missing directory: nothing is created.
        let nowhere = dir.join("missing").join("doc.mirae");
        assert!(write_file(&nowhere, |w| Ok(w.write_all(b"x")?)).is_err());
        assert!(!nowhere.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
