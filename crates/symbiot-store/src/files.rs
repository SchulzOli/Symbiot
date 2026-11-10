//! #28: the file store. Files referenced by raw data (photos, attachments) live next to the
//! database under their SHA-256, `files/<first two hex chars>/<hash>`. The chain holds only the
//! hash (in [`symbiot_core::FileRef`]), so the same file is stored once however often it is
//! referenced, and a file that arrives from another device is checked against its hash.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use symbiot_core::Hash;

use crate::{Error, Result, Store, crypt};

/// Hashes and counts what is read through it.
struct Counted<R> {
    inner: R,
    hasher: Sha256,
    len: u64,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.len > MAX_FILE_BYTES {
            return Err(io::Error::other("zu groß"));
        }
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        self.len += n as u64;
        Ok(n)
    }
}

/// Most a single file may have; larger ones are left out of an import (and the sync).
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// What the store knows about a file from the raw data that references it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FileInfo {
    pub hash: Hash,
    pub name: String,
    pub mime: String,
    pub len: u64,
}

fn io_err(e: io::Error) -> Error {
    Error::Files(e.to_string())
}

impl Store {
    /// Where files go. Set by [`Store::open_dir`]; a store without it has no files.
    pub fn set_files_dir(&mut self, dir: PathBuf) {
        self.files = Some(dir);
    }

    fn files_dir(&self) -> Result<&Path> {
        self.files
            .as_deref()
            .ok_or(Error::Invalid("Dieser Speicher hat keinen Dateiordner"))
    }

    fn file_path(dir: &Path, hash: &Hash) -> PathBuf {
        let hex = hash.to_hex();
        dir.join(&hex[..2]).join(hex)
    }

    /// Stores bytes and returns their hash. Storing the same bytes twice changes nothing.
    pub fn put_file(&self, bytes: &[u8]) -> Result<Hash> {
        let hash = Hash(Sha256::digest(bytes).into());
        self.write_file(&hash, &mut &bytes[..])?;
        Ok(hash)
    }

    /// Stores bytes that must have `hash`, e.g. a file from another device.
    pub fn put_file_checked(&self, hash: &Hash, bytes: &[u8]) -> Result<()> {
        if Hash(Sha256::digest(bytes).into()) != *hash {
            return Err(Error::Files("Datei passt nicht zu ihrem Hash".into()));
        }
        self.write_file(hash, &mut &bytes[..])
    }

    /// Copies a file into the store without reading it into memory. Returns hash and length.
    pub fn put_file_from(&self, path: &Path) -> Result<(Hash, u64)> {
        let dir = self.files_dir()?;
        fs::create_dir_all(dir).map_err(io_err)?;
        let tmp = dir.join(format!(".import-{}", std::process::id()));
        let mut src = Counted {
            inner: fs::File::open(path).map_err(io_err)?,
            hasher: Sha256::new(),
            len: 0,
        };
        let mut out = fs::File::create(&tmp).map_err(io_err)?;
        let written = self.copy_in(&mut src, &mut out);
        if src.len > MAX_FILE_BYTES {
            let _ = fs::remove_file(&tmp);
            return Err(Error::Files(format!("{} ist zu groß", path.display())));
        }
        written.and_then(|()| out.sync_all()).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            io_err(e)
        })?;
        let hash = Hash(src.hasher.finalize().into());
        self.place(&tmp, &hash)?;
        Ok((hash, src.len))
    }

    /// Writes `src` as it is stored: sealed if this store has a key (E6), plain otherwise.
    fn copy_in(&self, src: &mut impl Read, out: &mut impl Write) -> io::Result<()> {
        match &self.file_key {
            Some(key) => key.seal(src, out),
            None => io::copy(src, out).map(|_| ()),
        }
    }

    fn write_file(&self, hash: &Hash, src: &mut impl Read) -> Result<()> {
        let dir = self.files_dir()?;
        if Self::file_path(dir, hash).exists() {
            return Ok(());
        }
        fs::create_dir_all(dir).map_err(io_err)?;
        let tmp = dir.join(format!(".write-{}-{}", std::process::id(), hash.to_hex()));
        let mut out = fs::File::create(&tmp).map_err(io_err)?;
        self.copy_in(src, &mut out).map_err(io_err)?;
        out.sync_all().map_err(io_err)?;
        self.place(&tmp, hash)
    }

    /// E6: seals every file that still lies here in the clear — once, when a store that was
    /// unencrypted gets its key. Returns how many files were sealed.
    #[cfg_attr(not(feature = "encryption"), allow(dead_code))]
    pub(crate) fn seal_files(&self) -> Result<usize> {
        let (Some(dir), Some(key)) = (self.files.as_deref(), &self.file_key) else {
            return Ok(0);
        };
        let Ok(shards) = fs::read_dir(dir) else {
            return Ok(0);
        };
        let mut sealed = 0;
        for shard in shards.flatten() {
            let Ok(files) = fs::read_dir(shard.path()) else {
                continue;
            };
            for f in files.flatten() {
                let path = f.path();
                let name = f.file_name();
                let Some(hash) = name.to_str().and_then(Hash::from_hex) else {
                    continue;
                };
                let bytes = fs::read(&path).map_err(io_err)?;
                // Already sealed (an interrupted run), unless it only looks like it.
                if crypt::FileKey::is_sealed(&bytes) && key.open(&bytes).is_ok() {
                    continue;
                }
                let tmp = dir.join(format!(".seal-{}-{}", std::process::id(), hash.to_hex()));
                let mut out = fs::File::create(&tmp).map_err(io_err)?;
                key.seal(&mut &bytes[..], &mut out).map_err(io_err)?;
                out.sync_all().map_err(io_err)?;
                fs::rename(&tmp, &path).map_err(io_err)?;
                sealed += 1;
            }
        }
        Ok(sealed)
    }

    /// Moves a finished temporary file to its place; a file that is already there stays.
    fn place(&self, tmp: &Path, hash: &Hash) -> Result<()> {
        let target = Self::file_path(self.files_dir()?, hash);
        if target.exists() {
            let _ = fs::remove_file(tmp);
            return Ok(());
        }
        fs::create_dir_all(target.parent().expect("file path has a parent")).map_err(io_err)?;
        fs::rename(tmp, &target).map_err(io_err)
    }

    pub fn has_file(&self, hash: &Hash) -> bool {
        self.files
            .as_deref()
            .is_some_and(|d| Self::file_path(d, hash).exists())
    }

    /// The bytes of a file, checked against its hash. `None` if it is not here.
    pub fn file(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        let Some(dir) = self.files.as_deref() else {
            return Ok(None);
        };
        let stored = match fs::read(Self::file_path(dir, hash)) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_err(e)),
        };
        // Sealed files are opened with the key. A file that is not sealed (from before the
        // key, or one that merely starts like a sealed one) is taken as it is — the hash
        // below decides either way.
        let bytes = match &self.file_key {
            Some(key) if crypt::FileKey::is_sealed(&stored) => key.open(&stored).unwrap_or(stored),
            _ => stored,
        };
        if Hash(Sha256::digest(&bytes).into()) != *hash {
            return Err(Error::Files(format!("Datei {hash} ist beschädigt")));
        }
        Ok(Some(bytes))
    }

    /// Name, type and size of a file, as the raw data that references it says.
    pub fn file_info(&self, hash: &Hash) -> Result<Option<FileInfo>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row(
                "SELECT name, mime, len FROM evidence_file WHERE hash = ?1 LIMIT 1",
                [hash.0.as_slice()],
                |r| {
                    Ok(FileInfo {
                        hash: *hash,
                        name: r.get(0)?,
                        mime: r.get(1)?,
                        len: r.get::<_, i64>(2)? as u64,
                    })
                },
            )
            .optional()?)
    }

    /// Files that raw data with content references but that are not here yet. Empty for a
    /// store without a file folder.
    pub fn missing_files(&self) -> Result<Vec<Hash>> {
        if self.files.is_none() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT f.hash FROM evidence_file AS f
             JOIN evidence AS e ON e.key = f.key
             WHERE e.item IS NOT NULL AND f.len <= ?1",
        )?;
        let rows = stmt.query_map([MAX_FILE_BYTES as i64], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for row in rows {
            let b = row?;
            let Ok(raw) = <[u8; 32]>::try_from(b.as_slice()) else {
                continue;
            };
            let hash = Hash(raw);
            if !self.has_file(&hash) {
                out.push(hash);
            }
        }
        Ok(out)
    }

    /// E2: removes files no raw data with content references any more. Returns how many.
    pub fn prune_files(&self) -> Result<usize> {
        let Some(dir) = self.files.as_deref() else {
            return Ok(0);
        };
        let mut removed = 0;
        let Ok(shards) = fs::read_dir(dir) else {
            return Ok(0);
        };
        let mut stmt = self.conn.prepare(
            "SELECT 1 FROM evidence_file AS f JOIN evidence AS e ON e.key = f.key
             WHERE f.hash = ?1 AND e.item IS NOT NULL LIMIT 1",
        )?;
        for shard in shards.flatten() {
            let Ok(files) = fs::read_dir(shard.path()) else {
                continue;
            };
            for f in files.flatten() {
                let name = f.file_name();
                let Some(hash) = name.to_str().and_then(Hash::from_hex) else {
                    continue;
                };
                if !stmt.exists([hash.0.as_slice()])? {
                    fs::remove_file(f.path()).map_err(io_err)?;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use symbiot_core::Signer;
    use symbiot_core::evidence::{Actor, Evidence, FileRef, role};

    use super::*;

    fn store(dir: &Path) -> Store {
        let mut s = Store::open_in_memory(Signer::from_seed([1; 32])).unwrap();
        s.set_files_dir(dir.join("files"));
        s
    }

    fn photo(n: u32, file: FileRef) -> Evidence {
        Evidence {
            id: format!("telegram:1/{n}"),
            kind: "schema:Message".into(),
            at_ms: 10 + n as u64,
            actors: vec![Actor {
                id: "telegram:anna".into(),
                role: role::FROM.into(),
                name: "Anna".into(),
            }],
            text: String::new(),
            fields: vec![],
            files: vec![file],
        }
    }

    #[test]
    fn put_read_and_dedupe() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(dir.path());
        let h = s.put_file(b"Foto").unwrap();
        assert_eq!(s.put_file(b"Foto").unwrap(), h);
        assert_eq!(s.file(&h).unwrap().as_deref(), Some(&b"Foto"[..]));

        let src = dir.path().join("bild.jpg");
        fs::write(&src, b"Foto").unwrap();
        assert_eq!(s.put_file_from(&src).unwrap(), (h, 4));

        let other = Hash([7; 32]);
        assert!(s.put_file_checked(&other, b"Foto").is_err());
        assert_eq!(s.file(&other).unwrap(), None);

        // A file changed on disk is noticed.
        let hex = h.to_hex();
        fs::write(
            dir.path().join("files").join(&hex[..2]).join(&hex),
            b"anders",
        )
        .unwrap();
        assert!(s.file(&h).is_err());
    }

    #[test]
    fn missing_info_and_prune() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(dir.path());
        let here = s.put_file(b"hier").unwrap();
        let away = Hash(Sha256::digest(b"woanders").into());
        let unused = s.put_file(b"ungenutzt").unwrap();
        let file = |hash, name: &str| FileRef {
            hash,
            name: name.into(),
            mime: "image/jpeg".into(),
            len: 4,
        };
        s.record_evidence(vec![
            photo(1, file(here, "a.jpg")),
            photo(2, file(away, "b.jpg")),
        ])
        .unwrap();

        assert_eq!(s.missing_files().unwrap(), [away]);
        assert_eq!(s.file_info(&here).unwrap().unwrap().name, "a.jpg");
        assert_eq!(
            s.prune_files().unwrap(),
            1,
            "only the unreferenced file goes"
        );
        assert!(!s.has_file(&unused));
        assert!(s.has_file(&here));

        s.put_file_checked(&away, b"woanders").unwrap();
        assert!(s.missing_files().unwrap().is_empty());
    }

    #[test]
    fn store_without_folder() {
        let s = Store::open_in_memory(Signer::from_seed([1; 32])).unwrap();
        assert!(s.missing_files().unwrap().is_empty());
        assert_eq!(s.file(&Hash([0; 32])).unwrap(), None);
        assert!(s.put_file(b"x").is_err());
    }
}
