//! A look at a ZIP archive's end records before the archive is opened.
//!
//! The ZIP reader parses the whole central directory when it opens an
//! archive and keeps one record per entry, and it keys entries by name,
//! so duplicate names would silently collapse into one. Reading the
//! declared entry count and directory size first (PKWARE APPNOTE 4.3.14 –
//! 4.3.16) bounds what opening can cost and lets the loader compare the
//! count with what the reader kept.

use std::io::{Read, Seek, SeekFrom};

use crate::limits::check;
use crate::{IoError, Limits};

const EOCD_SIG: u32 = 0x0605_4b50;
const EOCD_LEN: usize = 22;
const LOCATOR_SIG: u32 = 0x0706_4b50;
const LOCATOR_LEN: usize = 20;
const EOCD64_SIG: u32 = 0x0606_4b50;
const EOCD64_LEN: usize = 56;
/// The end record may be followed by a comment of up to 65 535 bytes.
const MAX_TAIL: u64 = (EOCD_LEN + u16::MAX as usize) as u64;

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().expect("4 bytes"))
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().expect("8 bytes"))
}

/// The declared number of entries, after checking it and the central
/// directory's size against `limits`. Not a ZIP archive (no end record):
/// [`IoError::NotADocument`].
pub(crate) fn declared_entries<R: Read + Seek>(r: &mut R, limits: &Limits) -> Result<u64, IoError> {
    let len = r.seek(SeekFrom::End(0))?;
    let tail_len = len.min(MAX_TAIL);
    let mut tail = vec![0; tail_len as usize];
    r.seek(SeekFrom::Start(len - tail_len))?;
    r.read_exact(&mut tail)?;
    // The last end record whose comment reaches exactly to the end.
    let at = (0..=tail.len().saturating_sub(EOCD_LEN))
        .rev()
        .find(|&i| {
            tail.len() >= EOCD_LEN
                && u32_at(&tail, i) == EOCD_SIG
                && i + EOCD_LEN + usize::from(u16_at(&tail, i + 20)) == tail.len()
        })
        .ok_or(IoError::NotADocument)?;
    let eocd = &tail[at..at + EOCD_LEN];
    let mut entries = u64::from(u16_at(eocd, 10));
    let mut dir_bytes = u64::from(u32_at(eocd, 12));
    let zip64 = entries == 0xffff || dir_bytes == 0xffff_ffff || u32_at(eocd, 16) == 0xffff_ffff;
    if zip64 {
        let malformed = || IoError::MalformedArchive("broken ZIP64 end record".to_owned());
        let locator_at = at.checked_sub(LOCATOR_LEN).ok_or_else(malformed)?;
        let locator = &tail[locator_at..at];
        if u32_at(locator, 0) != LOCATOR_SIG {
            return Err(malformed());
        }
        let record_at = u64_at(locator, 8);
        if record_at
            .checked_add(EOCD64_LEN as u64)
            .is_none_or(|end| end > len)
        {
            return Err(malformed());
        }
        let mut record = [0; EOCD64_LEN];
        r.seek(SeekFrom::Start(record_at))?;
        r.read_exact(&mut record)?;
        if u32_at(&record, 0) != EOCD64_SIG {
            return Err(malformed());
        }
        entries = u64_at(&record, 32);
        dir_bytes = u64_at(&record, 40);
    }
    check("archive entries", entries, limits.max_entries)?;
    check(
        "bytes of ZIP directory",
        dir_bytes,
        limits.max_directory_bytes,
    )?;
    if dir_bytes > len {
        return Err(IoError::MalformedArchive(
            "the ZIP directory is larger than the file".to_owned(),
        ));
    }
    r.seek(SeekFrom::Start(0))?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;

    fn archive(names: &[&str]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for n in names {
            w.start_file(*n, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"x").unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn counts_entries_of_real_archives() {
        let bytes = archive(&["a", "b", "c"]);
        let n = declared_entries(&mut Cursor::new(&bytes), &Limits::default()).unwrap();
        assert_eq!(n, 3);
        // With a trailing comment too.
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        w.set_comment("hello").unwrap();
        w.start_file("a", zip::write::SimpleFileOptions::default())
            .unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(
            declared_entries(&mut Cursor::new(&bytes), &Limits::default()).unwrap(),
            1
        );
    }

    #[test]
    fn rejects_non_archives_and_oversized_directories() {
        for junk in [
            &b""[..],
            b"PK",
            &[0u8; 100][..],
            b"not a zip file at all, really",
        ] {
            assert!(matches!(
                declared_entries(&mut Cursor::new(junk), &Limits::default()),
                Err(IoError::NotADocument)
            ));
        }
        let bytes = archive(&["a", "b", "c"]);
        let tight = Limits {
            max_entries: 2,
            ..Limits::default()
        };
        assert!(matches!(
            declared_entries(&mut Cursor::new(&bytes), &tight),
            Err(IoError::LimitExceeded {
                what: "archive entries",
                ..
            })
        ));
        // An end record claiming a huge directory.
        let mut forged = bytes.clone();
        let at = forged.len() - EOCD_LEN;
        forged[at + 12..at + 16].copy_from_slice(&0x7fff_ffffu32.to_le_bytes());
        assert!(declared_entries(&mut Cursor::new(&forged), &Limits::default()).is_err());
        // A ZIP64 marker without its records.
        let mut forged = bytes;
        let at = forged.len() - EOCD_LEN;
        forged[at + 10..at + 12].copy_from_slice(&0xffffu16.to_le_bytes());
        assert!(matches!(
            declared_entries(&mut Cursor::new(&forged), &Limits::default()),
            Err(IoError::MalformedArchive(_))
        ));
    }
}
