//! The append-only, signed hash chain. One chain per device, one writer per chain.
//!
//! `hash = sha256(postcard(protocol, author, seq, prev, ts_ms, op))`, `sig = ed25519(hash)`.
//! Changing any byte, reordering, or dropping an entry breaks verification.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::PROTOCOL_VERSION;
use crate::bytes::{Hash, NodeId, PubKey, Sig};
use crate::edge::EdgeKind;
use crate::node::NodeKind;
use crate::proposal::Proposal;

/// What an entry does. Editing never overwrites — it appends an `UpdateNode`.
/// Externally tagged (serde default): postcard cannot read internally tagged enums.
/// postcard encodes the variant index, so new variants go at the END — reordering
/// would change the hashes of existing chains.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    CreateNode {
        kind: NodeKind,
        title: String,
        body: String,
    },
    UpdateNode {
        node: NodeId,
        title: String,
        body: String,
    },
    Link {
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
    },
    /// The human accepted an AI proposal: a new node that derives from `proposal.goal`.
    AcceptProposal { proposal: Proposal },
    /// The human rejected an AI proposal. The graph stays as it is; the decision is kept.
    RejectProposal { proposal: Proposal },
    /// Marks a node as sensitive (or not): sensitive nodes never leave the home network.
    /// A separate entry, not a node field, so nodes keep the same shape everywhere.
    MarkSensitive { node: NodeId, sensitive: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub author: PubKey,
    pub seq: u64,
    pub prev: Hash,
    pub ts_ms: u64,
    pub op: Op,
    pub hash: Hash,
    pub sig: Sig,
}

impl LedgerEntry {
    /// Recomputes the hash from the signed content (ignores the stored `hash`).
    pub fn content_hash(&self) -> Result<Hash, postcard::Error> {
        content_hash(&self.author, self.seq, &self.prev, self.ts_ms, &self.op)
    }
}

fn content_hash(
    author: &PubKey,
    seq: u64,
    prev: &Hash,
    ts_ms: u64,
    op: &Op,
) -> Result<Hash, postcard::Error> {
    let bytes = postcard::to_allocvec(&(PROTOCOL_VERSION, author, seq, prev, ts_ms, op))?;
    Ok(Hash(Sha256::digest(&bytes).into()))
}

/// A device's signing identity. The seed comes from the platform's RNG and is
/// stored by the app (file, keychain, secure element) — the core never touches I/O.
pub struct Signer {
    key: SigningKey,
}

impl Signer {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Signer {
            key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn public(&self) -> PubKey {
        PubKey(self.key.verifying_key().to_bytes())
    }

    /// Appends `op` after `prev` (the current tip of this device's chain, `None` for the first entry).
    pub fn append(&self, prev: Option<&LedgerEntry>, ts_ms: u64, op: Op) -> LedgerEntry {
        let author = self.public();
        let (seq, prev_hash) = match prev {
            Some(p) => {
                debug_assert_eq!(p.author, author, "a chain has exactly one writer");
                (p.seq + 1, p.hash)
            }
            None => (0, Hash::ZERO),
        };
        let hash = content_hash(&author, seq, &prev_hash, ts_ms, &op)
            .expect("postcard serialization into a Vec cannot fail");
        let sig = Sig(self.key.sign(&hash.0).to_bytes());
        LedgerEntry {
            author,
            seq,
            prev: prev_hash,
            ts_ms,
            op,
            hash,
            sig,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyError {
    /// Entries missing, duplicated or reordered.
    WrongSeq { expected: u64, found: u64 },
    /// `prev` does not point to the previous entry's hash.
    BrokenLink { seq: u64 },
    /// Content was changed after signing.
    BadHash { seq: u64 },
    /// Signature does not match the author's key.
    BadSignature { seq: u64 },
    /// A different device wrote into this chain.
    ForeignAuthor { seq: u64 },
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::WrongSeq { expected, found } => write!(
                f,
                "Eintrag {expected} erwartet, {found} gefunden (fehlt, doppelt oder vertauscht)"
            ),
            VerifyError::BrokenLink { seq } => {
                write!(f, "Eintrag {seq}: Verweis auf den Vorgänger stimmt nicht")
            }
            VerifyError::BadHash { seq } => {
                write!(f, "Eintrag {seq}: Inhalt wurde nachträglich verändert")
            }
            VerifyError::BadSignature { seq } => write!(f, "Eintrag {seq}: Signatur ungültig"),
            VerifyError::ForeignAuthor { seq } => {
                write!(f, "Eintrag {seq}: stammt von einem anderen Gerät")
            }
        }
    }
}

/// Verifies one device's chain from its first entry. Returns the number of entries.
pub fn verify_chain(entries: &[LedgerEntry]) -> Result<usize, VerifyError> {
    let Some(first) = entries.first() else {
        return Ok(0);
    };
    let author = first.author;
    let key =
        VerifyingKey::from_bytes(&author.0).map_err(|_| VerifyError::BadSignature { seq: 0 })?;

    let mut prev = Hash::ZERO;
    for (i, e) in entries.iter().enumerate() {
        let expected = i as u64;
        if e.seq != expected {
            return Err(VerifyError::WrongSeq {
                expected,
                found: e.seq,
            });
        }
        if e.author != author {
            return Err(VerifyError::ForeignAuthor { seq: e.seq });
        }
        if e.prev != prev {
            return Err(VerifyError::BrokenLink { seq: e.seq });
        }
        if e.content_hash().ok() != Some(e.hash) {
            return Err(VerifyError::BadHash { seq: e.seq });
        }
        let sig = Signature::from_bytes(&e.sig.0);
        if key.verify_strict(&e.hash.0, &sig).is_err() {
            return Err(VerifyError::BadSignature { seq: e.seq });
        }
        prev = e.hash;
    }
    Ok(entries.len())
}

/// Result of checking one device's chain — what the UI shows behind „Kette prüfen“.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub entries: usize,
    pub device: PubKey,
    pub tip: Option<Hash>,
    pub error: Option<String>,
}

impl VerifyReport {
    pub fn of(device: PubKey, entries: &[LedgerEntry]) -> Self {
        let result = verify_chain(entries);
        VerifyReport {
            ok: result.is_ok(),
            entries: entries.len(),
            device,
            tip: entries.last().map(|e| e.hash),
            error: result.err().map(|e| alloc::string::ToString::to_string(&e)),
        }
    }
}

/// Convenience for tests and tools: builds a chain from a list of ops.
pub fn build_chain(signer: &Signer, ops: impl IntoIterator<Item = (u64, Op)>) -> Vec<LedgerEntry> {
    let mut chain: Vec<LedgerEntry> = Vec::new();
    for (ts, op) in ops {
        let e = signer.append(chain.last(), ts, op);
        chain.push(e);
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn signer() -> Signer {
        Signer::from_seed([7; 32])
    }

    fn create(title: &str) -> Op {
        Op::CreateNode {
            kind: NodeKind::Goal,
            title: title.into(),
            body: String::new(),
        }
    }

    fn chain(n: usize) -> Vec<LedgerEntry> {
        build_chain(
            &signer(),
            (0..n).map(|i| (1_000 + i as u64, create(&alloc::format!("Ziel {i}")))),
        )
    }

    #[test]
    fn empty_chain_is_valid() {
        assert_eq!(verify_chain(&[]), Ok(0));
    }

    #[test]
    fn fresh_chain_verifies() {
        assert_eq!(verify_chain(&chain(5)), Ok(5));
    }

    #[test]
    fn hashes_are_stable_across_platforms() {
        // Pinned: if this changes, devices can no longer verify each other's chains.
        let e = signer().append(None, 1_000, create("Ziel 0"));
        assert_eq!(e.seq, 0);
        assert_eq!(e.prev, Hash::ZERO);
        assert_eq!(e.content_hash().unwrap(), e.hash);
        assert_eq!(
            e.hash.to_hex(),
            "77cf04ae9349400104de686c71cbdb4978e811c4466b494b3a5de33600d55dbc"
        );
    }

    #[test]
    fn link_round_trips_and_verifies() {
        let s = signer();
        let a = s.append(None, 1, create("Ziel"));
        let b = s.append(Some(&a), 2, create("Beleg"));
        let l = s.append(
            Some(&b),
            3,
            Op::Link {
                from: NodeId::from_entry_hash(&b.hash),
                to: NodeId::from_entry_hash(&a.hash),
                kind: EdgeKind::Supports,
            },
        );
        let chain = alloc::vec![a, b, l];
        let back: Vec<LedgerEntry> =
            postcard::from_bytes(&postcard::to_allocvec(&chain).unwrap()).unwrap();
        assert_eq!(verify_chain(&back), Ok(3));
    }

    #[test]
    fn foreign_writer_is_rejected() {
        let mut c = chain(2);
        let other = Signer::from_seed([9; 32]);
        c.push(other.append(None, 2_000, create("fremd")));
        c[2].seq = 2;
        assert_eq!(verify_chain(&c), Err(VerifyError::ForeignAuthor { seq: 2 }));
    }

    #[test]
    fn json_round_trip_keeps_chain_valid() {
        let c = chain(3);
        let json = serde_json::to_string(&c).unwrap();
        let back: Vec<LedgerEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(verify_chain(&back), Ok(3));
    }

    #[test]
    fn postcard_round_trip_keeps_chain_valid() {
        let c = chain(3);
        let bytes = postcard::to_allocvec(&c).unwrap();
        let back: Vec<LedgerEntry> = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(verify_chain(&back), Ok(3));
    }

    #[derive(Debug, Clone)]
    enum Tamper {
        Title(usize, String),
        Timestamp(usize, u64),
        HashBit(usize, usize),
        SigBit(usize, usize),
        Drop(usize),
        Swap(usize, usize),
        Duplicate(usize),
    }

    fn tamper() -> impl Strategy<Value = Tamper> {
        let i = 0usize..64;
        prop_oneof![
            (i.clone(), "[a-z]{1,8}").prop_map(|(i, s)| Tamper::Title(i, s)),
            (i.clone(), any::<u64>()).prop_map(|(i, t)| Tamper::Timestamp(i, t)),
            (i.clone(), 0usize..256).prop_map(|(i, b)| Tamper::HashBit(i, b)),
            (i.clone(), 0usize..512).prop_map(|(i, b)| Tamper::SigBit(i, b)),
            i.clone().prop_map(Tamper::Drop),
            (i.clone(), i.clone()).prop_map(|(a, b)| Tamper::Swap(a, b)),
            i.prop_map(Tamper::Duplicate),
        ]
    }

    proptest! {
        /// F2 acceptance: every changed byte, swapped or missing entry is detected.
        #[test]
        fn any_tampering_is_detected(n in 2usize..8, t in tamper()) {
            let mut c = chain(n);
            let original = c.clone();
            match t {
                Tamper::Title(i, s) => {
                    let i = i % n;
                    if let Op::CreateNode { title, .. } = &mut c[i].op { title.push_str(&s); }
                }
                Tamper::Timestamp(i, ts) => {
                    let i = i % n;
                    prop_assume!(c[i].ts_ms != ts);
                    c[i].ts_ms = ts;
                }
                Tamper::HashBit(i, b) => { let i = i % n; c[i].hash.0[b / 8] ^= 1 << (b % 8); }
                Tamper::SigBit(i, b) => { let i = i % n; c[i].sig.0[b / 8] ^= 1 << (b % 8); }
                // Dropping the tip leaves a valid shorter chain — that is caught by
                // sync (peers know the tip), not by verification. Drop inside only.
                Tamper::Drop(i) => { c.remove(i % (n - 1)); }
                Tamper::Swap(a, b) => {
                    let (a, b) = (a % n, b % n);
                    prop_assume!(a != b);
                    c.swap(a, b);
                }
                Tamper::Duplicate(i) => { let e = c[i % n].clone(); c.insert(i % n, e); }
            }
            prop_assert_ne!(&c, &original);
            prop_assert!(verify_chain(&c).is_err());
        }
    }
}
