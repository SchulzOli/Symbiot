//! E6: encryption at rest. One key for the whole store: SQLCipher encrypts the database file
//! with it, and the files of the file store are sealed with a key derived from it.
//!
//! The key never lives next to the data in the clear: on the home server it comes from a
//! secret (`SYMBIOT_DB_KEY` or `SYMBIOT_DB_KEY_FILE`), in the apps from the keychain of the
//! operating system. A store opened without a key works as before, unencrypted.

use std::fmt;
use std::io::{self, Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// The key of a store: 32 bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct StoreKey([u8; 32]);

impl fmt::Debug for StoreKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StoreKey(…)")
    }
}

impl StoreKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        StoreKey(bytes)
    }

    /// 64 hex characters, as `openssl rand -hex 32` prints them.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        if hex.len() != 64 || !hex.is_ascii() {
            return None;
        }
        let mut key = [0u8; 32];
        for (i, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(StoreKey(key))
    }

    /// The key from the environment: `SYMBIOT_DB_KEY` (64 hex characters) or
    /// `SYMBIOT_DB_KEY_FILE` (a file that holds them, e.g. a Docker secret). `None` if neither
    /// is set; an error if one is set but is not a key, so a typo never ends in an
    /// unencrypted store.
    pub fn from_env() -> Result<Option<Self>> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let hex = match (var("SYMBIOT_DB_KEY"), var("SYMBIOT_DB_KEY_FILE")) {
            (Some(hex), _) => hex,
            (None, Some(path)) => std::fs::read_to_string(path)
                .map_err(|_| Error::Invalid("SYMBIOT_DB_KEY_FILE ist nicht lesbar"))?,
            (None, None) => return Ok(None),
        };
        StoreKey::from_hex(&hex)
            .map(Some)
            .ok_or(Error::Invalid(
                "Der Schlüssel für die Datenbank muss aus 64 Hex-Zeichen bestehen (openssl rand -hex 32)",
            ))
    }

    /// The statement that unlocks a SQLCipher database with this key (raw key, no passphrase
    /// stretching: the key is random already).
    #[cfg_attr(not(feature = "encryption"), allow(dead_code))]
    pub(crate) fn pragma(&self) -> String {
        let hex: String = self.0.iter().map(|b| format!("{b:02x}")).collect();
        format!("PRAGMA key = \"x'{hex}'\";")
    }

    /// The key for the file store, derived so that neither key gives away the other.
    #[cfg_attr(not(feature = "encryption"), allow(dead_code))]
    pub(crate) fn for_files(&self) -> FileKey {
        let mut h = Sha256::new();
        h.update(b"symbiot/files/v1");
        h.update(self.0);
        FileKey(h.finalize().into())
    }
}

/// Seals the files of the file store (photos, attachments).
#[derive(Clone)]
pub(crate) struct FileKey([u8; 32]);

/// A sealed file starts with this, then 16 random bytes, then the chunks.
const MAGIC: &[u8; 6] = b"SYMF1\n";
/// Plain bytes per chunk; a sealed chunk is 16 bytes longer (the tag).
const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;
const LAST: u64 = 1 << 63;

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

/// Reads until `buf` is full or the source ends. Returns how much was read.
fn read_full(src: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match src.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Each chunk has its own nonce: the file's random prefix, the chunk's number, and a mark on
/// the last one — so chunks cannot be swapped, repeated or cut off unnoticed.
fn nonce(prefix: &[u8; 16], chunk: u64, last: bool) -> XNonce {
    let mut n = [0u8; 24];
    n[..16].copy_from_slice(prefix);
    n[16..].copy_from_slice(&(chunk | if last { LAST } else { 0 }).to_be_bytes());
    *XNonce::from_slice(&n)
}

impl FileKey {
    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(Key::from_slice(&self.0))
    }

    /// Whether these bytes are a sealed file.
    pub(crate) fn is_sealed(bytes: &[u8]) -> bool {
        bytes.starts_with(MAGIC)
    }

    /// Seals `src` into `out`, chunk by chunk, so a large file never has to fit into memory.
    pub(crate) fn seal(&self, src: &mut impl Read, out: &mut impl Write) -> io::Result<()> {
        let mut prefix = [0u8; 16];
        getrandom::fill(&mut prefix).map_err(io::Error::other)?;
        out.write_all(MAGIC)?;
        out.write_all(&prefix)?;
        let cipher = self.cipher();
        let mut buf = vec![0u8; CHUNK];
        let mut chunk = 0u64;
        loop {
            let n = read_full(src, &mut buf)?;
            let last = n < CHUNK;
            let sealed = cipher
                .encrypt(&nonce(&prefix, chunk, last), &buf[..n])
                .map_err(|_| bad("Datei nicht verschlüsselbar"))?;
            out.write_all(&sealed)?;
            if last {
                return Ok(());
            }
            chunk += 1;
        }
    }

    /// Opens a sealed file. Fails if it was changed, cut off or sealed with another key.
    pub(crate) fn open(&self, sealed: &[u8]) -> io::Result<Vec<u8>> {
        let body = sealed
            .strip_prefix(MAGIC)
            .filter(|b| b.len() >= 16)
            .ok_or_else(|| bad("keine verschlüsselte Datei"))?;
        let (prefix, mut rest) = body.split_at(16);
        let prefix: [u8; 16] = prefix.try_into().expect("16 bytes");
        let cipher = self.cipher();
        let mut out = Vec::with_capacity(rest.len());
        let mut chunk = 0u64;
        loop {
            let n = rest.len().min(CHUNK + TAG);
            let last = n < CHUNK + TAG;
            let plain = cipher
                .decrypt(&nonce(&prefix, chunk, last), &rest[..n])
                .map_err(|_| bad("Datei ist beschädigt oder der Schlüssel passt nicht"))?;
            out.extend_from_slice(&plain);
            if last {
                return Ok(out);
            }
            rest = &rest[n..];
            chunk += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(b: u8) -> FileKey {
        StoreKey::from_bytes([b; 32]).for_files()
    }

    #[test]
    fn sealed_files_open_again_and_only_unchanged() {
        for len in [0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 3 * CHUNK + 17] {
            let plain: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let mut sealed = Vec::new();
            key(1).seal(&mut &plain[..], &mut sealed).unwrap();
            assert!(FileKey::is_sealed(&sealed));
            assert_eq!(key(1).open(&sealed).unwrap(), plain, "{len} bytes");
            assert!(key(2).open(&sealed).is_err(), "another key");
            // Nothing of the content shows, and nothing can be changed or cut off.
            if len >= 64 {
                assert!(!sealed.windows(32).any(|w| w == &plain[..32]));
            }
            let mut flipped = sealed.clone();
            let at = flipped.len() - 1;
            flipped[at] ^= 1;
            assert!(key(1).open(&flipped).is_err());
            assert!(key(1).open(&sealed[..sealed.len() - 1]).is_err());
            if len > CHUNK {
                // Without its last chunk the file is not a shorter valid file.
                let cut = MAGIC.len() + 16 + CHUNK + TAG;
                assert!(key(1).open(&sealed[..cut]).is_err());
            }
        }
        // Sealing twice gives different bytes: the prefix is random.
        let (mut a, mut b) = (Vec::new(), Vec::new());
        key(1).seal(&mut &b"gleich"[..], &mut a).unwrap();
        key(1).seal(&mut &b"gleich"[..], &mut b).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn keys_are_64_hex_characters() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let key = StoreKey::from_hex(&format!("  {hex}\n")).unwrap();
        assert_eq!(key.pragma(), format!("PRAGMA key = \"x'{hex}'\";"));
        assert!(StoreKey::from_hex(&hex[..62]).is_none());
        assert!(StoreKey::from_hex(&hex.replace('0', "g")).is_none());
        assert_eq!(format!("{key:?}"), "StoreKey(…)", "never printed");
    }
}
