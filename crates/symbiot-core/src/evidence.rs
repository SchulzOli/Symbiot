//! Raw data (E14): what the integration layer loads, unchanged, and the processing layer reads.
//!
//! This format is the contract between the two layers. An adapter turns whatever its source
//! delivers into [`Evidence`]; it never interprets. A processor only reads evidence; it never
//! asks a source. Neither knows the other.
//!
//! Evidence is written in batches ([`crate::Op::RecordEvidence`], kind
//! [`crate::kind::EVIDENCE_BATCH`]). The signed head holds one [`Evidence::key`] per item, the
//! removable content one part per item. So a deleted item leaves its key behind, and a second
//! import of the same data recognises it and does not bring it back.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bytes::Hash;

/// Domain separation for [`Evidence::key`].
const KEY_DOMAIN: &[u8] = b"symbiot/evidence/v1";

/// At most this many items per batch (E13). The core refuses larger batches.
pub const MAX_BATCH_ITEMS: usize = 256;
/// A batch is closed once its content reaches this size (E13). An item that is larger on its
/// own still gets a batch of its own.
pub const MAX_BATCH_BYTES: usize = 16 * 1024;
/// Longest id and kind, in bytes.
pub const MAX_ID_LEN: usize = 512;

/// Roles of a person in a piece of evidence. Plain strings, so a newer adapter can add one
/// without older devices failing to read the batch.
pub mod role {
    pub const FROM: &str = "from";
    pub const TO: &str = "to";
    pub const CC: &str = "cc";
    pub const AUTHOR: &str = "author";
    pub const MENTION: &str = "mention";
    pub const PARTICIPANT: &str = "participant";
}

/// One observation from a source, unchanged: a message, a mail, a commit, a photo, a visited
/// page, a game session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Stable and unique within its origin, with the origin as prefix:
    /// `telegram:<chat>/<message>`, `mail:<Message-ID>`, `git:<repo>@<commit>`.
    /// The same thing loaded twice — by the same adapter or by another one — has the same id.
    pub id: String,
    /// What it is, as a vocabulary term: `schema:Message`, `schema:EmailMessage`,
    /// `schema:SoftwareSourceCode`, `schema:Photograph`, `schema:WebPage`.
    pub kind: String,
    /// When it happened in the source (ms since the epoch), not when it was loaded.
    pub at_ms: u64,
    /// People as the source names them. Joining them to persons is the processing layer's job.
    pub actors: Vec<Actor>,
    /// The text, unchanged. Empty if there is none (e.g. a photo).
    pub text: String,
    /// Further fields as the source has them, e.g. `("subject", …)`, `("thread", …)`, `("url", …)`.
    pub fields: Vec<(String, String)>,
    /// Attached files; the files themselves live in the file store under their hash.
    pub files: Vec<FileRef>,
}

/// A person as the source names them, with a scheme prefix: `tel:+4930…`, `mailto:a@b.de`,
/// `telegram:user123`, `github:octocat`, or `name:Anna` when the source only has a name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub id: String,
    /// See [`role`].
    pub role: String,
}

/// A file that belongs to a piece of evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRef {
    /// SHA-256 of the file's bytes.
    pub hash: Hash,
    pub name: String,
    pub mime: String,
    pub len: u64,
}

/// Why a piece of evidence is not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    /// The id is empty, too long or has no origin prefix (`origin:…`).
    Id,
    /// The kind is empty, too long or not a vocabulary term (`prefix:Term`).
    Kind,
    /// No point in time.
    Time,
    /// An actor without a scheme prefix or without a role.
    Actor,
}

impl core::fmt::Display for Invalid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Invalid::Id => "Kennung fehlt, ist zu lang oder hat keine Herkunft (herkunft:…)",
            Invalid::Kind => "Art fehlt oder ist kein Begriff aus einem Vokabular (präfix:Begriff)",
            Invalid::Time => "Zeitpunkt fehlt",
            Invalid::Actor => "Person ohne Schema (z. B. mailto:, tel:) oder ohne Rolle",
        })
    }
}

impl Evidence {
    /// The dedup key: the same id gives the same key on every device.
    pub fn key(&self) -> Hash {
        key_of(&self.id)
    }

    /// Checks what every adapter must deliver.
    pub fn check(&self) -> Result<(), Invalid> {
        if !prefixed(&self.id) {
            return Err(Invalid::Id);
        }
        if !prefixed(&self.kind) {
            return Err(Invalid::Kind);
        }
        if self.at_ms == 0 {
            return Err(Invalid::Time);
        }
        if self
            .actors
            .iter()
            .any(|a| !prefixed(&a.id) || a.role.is_empty())
        {
            return Err(Invalid::Actor);
        }
        Ok(())
    }

    /// Size of the item in a batch's content.
    pub fn encoded_len(&self) -> usize {
        postcard::to_allocvec(self).map_or(0, |b| b.len())
    }
}

/// The key of an evidence id, see [`Evidence::key`].
pub fn key_of(id: &str) -> Hash {
    let mut h = Sha256::new();
    h.update(KEY_DOMAIN);
    h.update(id.as_bytes());
    Hash(h.finalize().into())
}

/// `origin:rest` with both parts non-empty and no whitespace in the origin.
fn prefixed(s: &str) -> bool {
    if s.len() > MAX_ID_LEN {
        return false;
    }
    match s.split_once(':') {
        Some((origin, rest)) => {
            !origin.is_empty() && !rest.is_empty() && !origin.contains(char::is_whitespace)
        }
        None => false,
    }
}

/// Splits items into batches within [`MAX_BATCH_ITEMS`] and [`MAX_BATCH_BYTES`], keeping order.
pub fn batches(items: Vec<Evidence>) -> Vec<Vec<Evidence>> {
    let mut out: Vec<Vec<Evidence>> = Vec::new();
    let mut current: Vec<Evidence> = Vec::new();
    let mut bytes = 0usize;
    for item in items {
        let len = item.encoded_len();
        let full = current.len() >= MAX_BATCH_ITEMS || bytes + len > MAX_BATCH_BYTES;
        if !current.is_empty() && full {
            out.push(core::mem::take(&mut current));
            bytes = 0;
        }
        bytes += len;
        current.push(item);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
pub(crate) fn sample(id: &str, at_ms: u64, text: &str) -> Evidence {
    Evidence {
        id: alloc::format!("test:{id}"),
        kind: "schema:Message".into(),
        at_ms,
        actors: alloc::vec![Actor {
            id: "name:Anna".into(),
            role: role::FROM.into(),
        }],
        text: text.into(),
        fields: alloc::vec![("thread".into(), "1".into())],
        files: alloc::vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_depends_only_on_the_id() {
        let a = sample("1", 10, "Hallo");
        let mut b = sample("1", 20, "anders");
        b.kind = "schema:EmailMessage".into();
        assert_eq!(a.key(), b.key());
        assert_ne!(a.key(), sample("2", 10, "Hallo").key());
    }

    #[test]
    fn check_needs_prefixes_and_time() {
        assert_eq!(sample("1", 10, "").check(), Ok(()));
        let mut e = sample("1", 10, "");
        e.id = "ohne-herkunft".into();
        assert_eq!(e.check(), Err(Invalid::Id));
        let mut e = sample("1", 10, "");
        e.kind = "Message".into();
        assert_eq!(e.check(), Err(Invalid::Kind));
        assert_eq!(sample("1", 0, "").check(), Err(Invalid::Time));
        let mut e = sample("1", 10, "");
        e.actors[0].id = "Anna".into();
        assert_eq!(e.check(), Err(Invalid::Actor));
        let mut e = sample("1", 10, "");
        e.id = alloc::format!("test:{}", "x".repeat(MAX_ID_LEN));
        assert_eq!(e.check(), Err(Invalid::Id));
    }

    #[test]
    fn batches_respect_count_and_size() {
        let many: Vec<Evidence> = (0..600)
            .map(|i| sample(&alloc::format!("{i}"), 1, ""))
            .collect();
        let b = batches(many);
        assert!(b.iter().all(|x| x.len() <= MAX_BATCH_ITEMS));
        assert_eq!(b.iter().map(Vec::len).sum::<usize>(), 600);

        let big = "x".repeat(10 * 1024);
        let b = batches(alloc::vec![
            sample("a", 1, &big),
            sample("b", 1, &big),
            sample("c", 1, &"y".repeat(40 * 1024)),
        ]);
        assert_eq!(b.iter().map(Vec::len).collect::<Vec<_>>(), [1, 1, 1]);
        assert_eq!(b[0][0].id, "test:a");
    }

    use crate::ledger::{Op, Signer, verify_chain};
    use crate::node::{ApplyError, Change, State};

    fn batch(s: &Signer, items: Vec<Evidence>) -> crate::LedgerEntry {
        s.append(None, 5, Op::RecordEvidence { items })
    }

    #[test]
    fn batch_round_trips_and_keeps_keys_without_content() {
        let s = Signer::from_seed([7; 32]);
        let items = alloc::vec![sample("1", 10, "Hallo"), sample("2", 11, "Tschüss")];
        let e = batch(&s, items.clone());
        assert_eq!(verify_chain(core::slice::from_ref(&e)), Ok(1));
        assert_eq!(
            e.op(),
            Ok(Op::RecordEvidence {
                items: items.clone()
            })
        );
        assert_eq!(e.content.as_ref().map(Vec::len), Some(2));

        let mut st = State::default();
        let Ok(Change::Evidence {
            keys, items: got, ..
        }) = st.apply(&e)
        else {
            panic!("batch must apply");
        };
        assert_eq!(keys, [items[0].key(), items[1].key()]);
        assert_eq!(got, items);
        assert!(
            st.nodes.is_empty(),
            "raw data never becomes a node by itself"
        );

        // E2: the content goes, the keys stay — the chain still verifies.
        let mut pruned = e.clone();
        pruned.content = None;
        assert_eq!(verify_chain(core::slice::from_ref(&pruned)), Ok(1));
        assert_eq!(pruned.evidence_keys(), Ok(keys.clone()));
        let Ok(Change::Evidence {
            keys: k2,
            items: none,
            ..
        }) = State::default().apply(&pruned)
        else {
            panic!("a pruned batch still applies");
        };
        assert_eq!((k2, none.len()), (keys, 0));
    }

    #[test]
    fn batch_breaking_the_contract_is_refused() {
        let s = Signer::from_seed([7; 32]);
        let refused = |items: Vec<Evidence>| State::default().apply(&batch(&s, items)).err();
        assert_eq!(refused(alloc::vec![]), Some(ApplyError::BadEvidence));
        assert_eq!(
            refused(alloc::vec![sample("1", 1, ""), sample("1", 2, "")]),
            Some(ApplyError::BadEvidence)
        );
        assert_eq!(
            refused(alloc::vec![sample("1", 0, "")]),
            Some(ApplyError::BadEvidence)
        );
        let too_many = (0..=MAX_BATCH_ITEMS)
            .map(|i| sample(&alloc::format!("{i}"), 1, ""))
            .collect();
        assert_eq!(refused(too_many), Some(ApplyError::BadEvidence));
    }
}
