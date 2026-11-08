//! Import adapters (E14, M7): one-off exports turned into raw data.
//!
//! Each adapter reads a source and returns [`Evidence`], unchanged and uninterpreted. It does
//! not write anything itself: the caller loads the items with
//! [`symbiot_store::Store::record_evidence`] into the adapter's own store, so each adapter is a
//! device with its own chain (E7). Format: `docs/ROHDATEN.md`.

use std::path::Path;

use sha2::{Digest, Sha256};
use symbiot_core::Evidence;

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
}

pub type Result<T> = std::result::Result<T, Error>;

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

    /// Reads the source at `path`.
    pub fn read(self, path: &Path, opts: &Options) -> Result<Vec<Evidence>> {
        match self {
            Source::Telegram => telegram::parse(&std::fs::read_to_string(path)?),
            Source::WhatsApp => {
                let chat = opts.chat.clone().unwrap_or_else(|| {
                    path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "chat".into())
                });
                whatsapp::parse(&std::fs::read_to_string(path)?, &chat, opts.utc_offset_min)
            }
            Source::Mbox => mbox::parse(std::io::BufReader::new(std::fs::File::open(path)?)),
            Source::Git => git::read(path),
        }
    }
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
