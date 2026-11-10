//! Import adapters (E14, M7): one-off exports turned into raw data.
//!
//! Each adapter reads a source and returns [`Evidence`], unchanged and uninterpreted. It does
//! not write anything itself: the caller loads the items with
//! [`symbiot_store::Store::record_evidence`] into the adapter's own store, so each adapter is a
//! device with its own chain (E7). Format: `docs/ROHDATEN.md`.

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use symbiot_core::{Evidence, FileRef, Hash};

pub mod git;
pub mod mbox;
pub mod telegram;
pub mod whatsapp;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Datei nicht lesbar: {0}")]
    Io(#[from] std::io::Error),
    #[error("kein gültiges JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Format(String),
    #[error("git: {0}")]
    Git(String),
    #[error("{0}")]
    Store(#[from] symbiot_store::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// What an adapter read: the raw data and the files it references (#28). The caller stores
/// the files first, then the raw data.
#[derive(Debug, Default)]
pub struct Import {
    pub items: Vec<Evidence>,
    pub files: Vec<PendingFile>,
}

/// A file to put into the file store, with the hash the raw data uses for it.
#[derive(Debug)]
pub struct PendingFile {
    pub hash: Hash,
    pub data: FileData,
}

#[derive(Debug)]
pub enum FileData {
    /// A file next to the export (photos of a chat export); copied, never read whole.
    Path(PathBuf),
    /// Bytes from inside the export (mail attachments).
    Bytes(Vec<u8>),
}

/// The sources this crate can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Telegram Desktop export, machine-readable (`result.json`), one chat or the whole account.
    Telegram,
    /// WhatsApp „Chat exportieren“ (`.txt`, with or without media).
    WhatsApp,
    /// A mailbox file in mbox format, e.g. from Google Takeout or Thunderbird.
    Mbox,
    /// The history of a local Git repository.
    Git,
}

/// Settings some sources need.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Name of the chat (WhatsApp exports do not contain it). Defaults to the file name.
    pub chat: Option<String>,
    /// Offset of the export's local times to UTC in minutes, e.g. 60 for CET (WhatsApp only).
    pub utc_offset_min: i32,
}

impl Source {
    pub const ALL: [Source; 4] = [
        Source::Telegram,
        Source::WhatsApp,
        Source::Mbox,
        Source::Git,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Source::Telegram => "telegram",
            Source::WhatsApp => "whatsapp",
            Source::Mbox => "mbox",
            Source::Git => "git",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.name() == s)
    }

    /// Reads the source at `path`. Files next to an export (photos, media) are found relative
    /// to the export's folder.
    pub fn read(self, path: &Path, opts: &Options) -> Result<Import> {
        let dir = path.parent();
        match self {
            Source::Telegram => telegram::parse(&std::fs::read_to_string(path)?, dir),
            Source::WhatsApp => {
                let chat = opts.chat.clone().unwrap_or_else(|| {
                    path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "chat".into())
                });
                whatsapp::parse(
                    &std::fs::read_to_string(path)?,
                    &chat,
                    opts.utc_offset_min,
                    dir,
                )
            }
            Source::Mbox => mbox::parse(std::io::BufReader::new(std::fs::File::open(path)?)),
            Source::Git => git::read(path),
        }
    }
}

impl Source {
    /// #44: whether the source can be read from one file chosen in the app (no folder next to
    /// it, so no media files). A Git history needs the repository folder.
    pub fn from_one_file(self) -> bool {
        !matches!(self, Source::Git)
    }

    /// Reads an export the app received as bytes, e.g. a file picked on the phone. `name` is the
    /// file name (the default chat name for WhatsApp).
    pub fn read_bytes(self, name: &str, bytes: &[u8], opts: &Options) -> Result<Import> {
        let text = || {
            std::str::from_utf8(bytes)
                .map(|t| t.trim_start_matches('\u{feff}'))
                .map_err(|_| Error::Format("Datei ist kein Text (UTF-8)".into()))
        };
        match self {
            Source::Telegram => telegram::parse(text()?, None),
            Source::WhatsApp => {
                let chat = opts
                    .chat
                    .clone()
                    .filter(|c| !c.trim().is_empty())
                    .unwrap_or_else(|| {
                        Path::new(name)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "chat".into())
                    });
                whatsapp::parse(text()?, &chat, opts.utc_offset_min, None)
            }
            Source::Mbox => mbox::parse(bytes),
            Source::Git => Err(Error::Format(
                "Git braucht den Ordner des Repositorys: symbiot-import git <pfad>".into(),
            )),
        }
    }
}

/// Loads what an adapter read into a store: the files first, then the raw data — once the raw
/// data is in the chain, its files should be there.
pub fn load(store: &mut symbiot_store::Store, import: Import) -> Result<symbiot_store::Recorded> {
    for f in &import.files {
        let hash = match &f.data {
            FileData::Path(p) => store.put_file_from(p).map(|(h, _)| h),
            FileData::Bytes(b) => store.put_file(b),
        }?;
        if hash != f.hash {
            return Err(Error::Format(
                "Eine Datei hat sich während des Imports geändert".into(),
            ));
        }
    }
    Ok(store.record_evidence(import.items)?)
}

/// The MIME type for a file name, from its extension.
pub fn mime_of(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("heic") => "image/heic",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("opus" | "ogg" | "oga") => "audio/ogg",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain",
        Some("vcf") => "text/vcard",
        _ => "application/octet-stream",
    }
}

/// A file next to an export: hashed without reading it whole. `None` if it is not there or
/// too large for the file store.
pub(crate) fn file_at(
    path: &Path,
    name: &str,
    mime: Option<&str>,
) -> Option<(FileRef, PendingFile)> {
    let mut f = std::fs::File::open(path).ok()?;
    if !f.metadata().ok()?.is_file() {
        return None;
    }
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut len = 0u64;
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        len += n as u64;
        if len > symbiot_store::MAX_FILE_BYTES {
            return None;
        }
        h.update(&buf[..n]);
    }
    let hash = Hash(h.finalize().into());
    Some((
        FileRef {
            hash,
            name: name.into(),
            mime: mime.unwrap_or_else(|| mime_of(name)).into(),
            len,
        },
        PendingFile {
            hash,
            data: FileData::Path(path.to_path_buf()),
        },
    ))
}

/// A file from inside an export.
pub(crate) fn file_of(bytes: Vec<u8>, name: &str, mime: &str) -> Option<(FileRef, PendingFile)> {
    if bytes.len() as u64 > symbiot_store::MAX_FILE_BYTES {
        return None;
    }
    let hash = Hash(Sha256::digest(&bytes).into());
    Some((
        FileRef {
            hash,
            name: name.into(),
            mime: mime.into(),
            len: bytes.len() as u64,
        },
        PendingFile {
            hash,
            data: FileData::Bytes(bytes),
        },
    ))
}

/// A short, stable fingerprint for ids of sources without their own ids.
pub(crate) fn fingerprint(parts: &[&[u8]]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    let digest: [u8; 32] = h.finalize().into();
    hex::encode(&digest[..16])
}

/// Milliseconds since the epoch for a civil date and time (proleptic Gregorian, UTC).
pub(crate) fn civil_ms(y: i64, m: u32, d: u32, h: u32, min: u32, s: u32) -> Option<u64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || min > 59 || s > 60 {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h as i64 * 3600 + min as i64 * 60 + s as i64;
    u64::try_from(secs).ok().map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_ms(1970, 1, 1, 0, 0, 0), Some(0));
        assert_eq!(civil_ms(2024, 3, 12, 14, 5, 0), Some(1_710_252_300_000));
        assert_eq!(civil_ms(2024, 13, 1, 0, 0, 0), None);
    }

    #[test]
    fn names_round_trip() {
        for s in Source::ALL {
            assert_eq!(Source::parse(s.name()), Some(s));
        }
    }
}

#[cfg(test)]
mod bytes_tests {
    use super::*;

    #[test]
    fn exports_from_one_file() {
        let chat =
            "\u{feff}12.03.24, 14:05 - Anna: Wandern am Samstag?\n12.03.24, 14:06 - Ben: Gerne\n";
        let opts = Options {
            utc_offset_min: 60,
            ..Options::default()
        };
        let i = Source::WhatsApp
            .read_bytes("WhatsApp Chat mit Wandergruppe.txt", chat.as_bytes(), &opts)
            .unwrap();
        assert_eq!(i.items.len(), 2);
        assert!(
            i.items[0]
                .fields
                .contains(&("chat".into(), "WhatsApp Chat mit Wandergruppe".into()))
        );

        let mail = "From a@example.org Mon Mar 11 09:00:00 2024\nFrom: a@example.org\nTo: b@example.org\nSubject: Hallo\nDate: Mon, 11 Mar 2024 09:00:00 +0000\nMessage-ID: <1@example.org>\n\nText\n";
        let i = Source::Mbox
            .read_bytes("x.mbox", mail.as_bytes(), &opts)
            .unwrap();
        assert_eq!(i.items.len(), 1);

        assert!(Source::Git.read_bytes("x", b"", &opts).is_err());
        assert!(!Source::Git.from_one_file());
        assert!(
            Source::Telegram
                .read_bytes("r.json", b"\xff", &opts)
                .is_err()
        );
    }
}
