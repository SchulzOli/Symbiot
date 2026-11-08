//! The append-only, signed hash chain. One chain per device, one writer per chain.
//!
//! An entry is a signed **head** plus removable **content** (E2, E13):
//! `hash = sha256("symbiot/entry/v2" ‖ postcard(protocol, head))`, `sig = ed25519(hash)`.
//! The head carries the Merkle root over the content parts, so content can be removed
//! later and the chain still verifies. Changing any byte of the head, reordering, or
//! dropping an entry breaks verification; changed content no longer matches its root.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::PROTOCOL_VERSION;
use crate::bytes::{Blob, EdgeId, Hash, NodeId, PubKey, RuleId, Salt, Sig};
use crate::edge::EdgeKind;
use crate::evidence::Evidence;
use crate::identity::Decision;
use crate::kind;
use crate::merkle;
use crate::node::NodeKind;
use crate::proposal::Proposal;

/// Domain separation: an entry hash can never be mistaken for a leaf, tree or checkpoint hash.
const ENTRY_DOMAIN: &[u8] = b"symbiot/entry/v2";
const SALT_DOMAIN: &[u8] = b"symbiot/salt/v2";

/// What an entry does — the language of the apps. Editing never overwrites, it appends an
/// `UpdateNode`. In the chain an op is split by [`Op::encode`] into a fixed kind id
/// ([`crate::kind`]), head params and removable content (title and body).
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
        /// The version that was edited (last hash in the node's history). Another device may
        /// have changed the node meanwhile — then this update is concurrent (M4).
        /// `None` in entries written before M4.
        base: Option<Hash>,
    },
    Link {
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
    },
    /// The human accepted an AI proposal: a new node that derives from `proposal.goal`.
    AcceptProposal {
        proposal: Proposal,
    },
    /// The human rejected an AI proposal. The graph stays as it is; the decision is kept.
    RejectProposal {
        proposal: Proposal,
    },
    /// Marks a node as sensitive (or not): sensitive nodes never leave the home network.
    /// A separate entry, not a node field, so nodes keep the same shape everywhere.
    MarkSensitive {
        node: NodeId,
        sensitive: bool,
    },
    /// Deletion marker (E2). The node disappears; if it is deletable (E3), devices also
    /// remove the content of every entry that shaped it.
    DeleteNode {
        node: NodeId,
    },
    /// Removes a connection. The `Link` entry stays, it has no content.
    Unlink {
        edge: EdgeId,
    },
    /// Whether a node is deletable and exportable (E3). Can be tightened and loosened later;
    /// every change is a new entry.
    SetPolicy {
        node: NodeId,
        deletable: bool,
        exportable: bool,
    },
    /// The person confirms a device (E5): „this device is mine“. Stands in the device's own chain.
    CertifyDevice {
        device: PubKey,
        name: String,
        person: PubKey,
        /// Person's signature, see [`crate::Person::certify`].
        sig: Sig,
    },
    /// The person revokes a lost device: its entries after `last_seq` no longer count.
    RevokeDevice {
        device: PubKey,
        last_seq: u64,
        person: PubKey,
        sig: Sig,
    },
    /// E4: from now on, nodes containing one of the keywords are sensitive. Accepted by the
    /// human, possibly suggested by `model`. Never removable content: rules carry proof.
    AddSensitivityRule {
        label: String,
        keywords: Vec<String>,
        model: Option<String>,
    },
    RemoveSensitivityRule {
        rule: RuleId,
    },
    /// E14: raw data from a source, unchanged. Written by adapters (each one a device with its
    /// own chain, E7). The head keeps one key per item, the content one part per item.
    RecordEvidence {
        items: Vec<Evidence>,
    },
    /// #31: the human decides which identifiers are one person. The identifiers are content
    /// (personal data, removable); the head only says which kind of decision it was.
    DecideIdentity {
        decision: Decision,
    },
}

fn enc<T: Serialize + ?Sized>(v: &T) -> Vec<u8> {
    postcard::to_allocvec(v).expect("postcard serialization into a Vec cannot fail")
}

fn dec<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Decode> {
    postcard::from_bytes(bytes).map_err(|_| Decode::Malformed)
}

impl Op {
    /// Splits the op into its kind id, the head params and the removable content parts.
    /// Only title and body (and raw data) are content; ids, kinds, sources and the model stay
    /// in the head.
    pub fn encode(&self) -> (u16, Vec<u8>, Vec<Vec<u8>>) {
        let (kind, params, content) = match self {
            Op::CreateNode {
                kind: k,
                title,
                body,
            } => (kind::CREATE_NODE, enc(k), Some(enc(&(title, body)))),
            Op::UpdateNode {
                node,
                title,
                body,
                base,
            } => {
                // Without a base the params stay as before M4, so old hashes keep their meaning.
                let params = match base {
                    Some(b) => enc(&(node, b)),
                    None => enc(node),
                };
                (kind::UPDATE_NODE, params, Some(enc(&(title, body))))
            }
            Op::Link { from, to, kind: k } => (kind::LINK, enc(&(from, to, k)), None),
            Op::AcceptProposal { proposal: p } => (
                kind::ACCEPT_PROPOSAL,
                enc(&(p.goal, &p.sources, p.kind, &p.model)),
                Some(enc(&(&p.title, &p.body))),
            ),
            Op::RejectProposal { proposal: p } => (
                kind::REJECT_PROPOSAL,
                enc(&(p.goal, &p.sources, p.kind, &p.model)),
                Some(enc(&(&p.title, &p.body))),
            ),
            Op::MarkSensitive { node, sensitive } => {
                (kind::MARK_SENSITIVE, enc(&(node, sensitive)), None)
            }
            Op::DeleteNode { node } => (kind::DELETE_NODE, enc(node), None),
            Op::Unlink { edge } => (kind::UNLINK, enc(edge), None),
            Op::SetPolicy {
                node,
                deletable,
                exportable,
            } => (kind::SET_POLICY, enc(&(node, deletable, exportable)), None),
            Op::CertifyDevice {
                device,
                name,
                person,
                sig,
            } => (
                kind::CERTIFY_DEVICE,
                enc(&(device, name, person, sig)),
                None,
            ),
            Op::RevokeDevice {
                device,
                last_seq,
                person,
                sig,
            } => (
                kind::REVOKE_DEVICE,
                enc(&(device, last_seq, person, sig)),
                None,
            ),
            Op::AddSensitivityRule {
                label,
                keywords,
                model,
            } => (
                kind::ADD_SENSITIVITY_RULE,
                enc(&(label, keywords, model)),
                None,
            ),
            Op::RemoveSensitivityRule { rule } => (kind::REMOVE_SENSITIVITY_RULE, enc(rule), None),
            Op::DecideIdentity { decision } => {
                let tag: u8 = match decision {
                    Decision::Merge { .. } => 1,
                    Decision::Split { .. } => 2,
                    Decision::KeepApart { .. } => 3,
                };
                (kind::DECIDE_IDENTITY, enc(&tag), Some(enc(decision)))
            }
            Op::RecordEvidence { items } => {
                let keys: Vec<Hash> = items.iter().map(Evidence::key).collect();
                return (
                    kind::EVIDENCE_BATCH,
                    enc(&keys),
                    items.iter().map(enc).collect(),
                );
            }
        };
        (kind, params, content.into_iter().collect())
    }
}

/// Why an entry cannot be read as an [`Op`]. It may still be validly signed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decode {
    /// A kind this version does not know — written by a newer app.
    Unknown(u16),
    /// Params or content do not fit the kind.
    Malformed,
}

impl fmt::Display for Decode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Decode::Unknown(k) => write!(f, "unbekannte Eintragsart {k}"),
            Decode::Malformed => f.write_str("Eintrag passt nicht zu seiner Art"),
        }
    }
}

/// The signed part of an entry. Never removed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Head {
    pub author: PubKey,
    pub seq: u64,
    pub prev: Hash,
    pub ts_ms: u64,
    /// Fixed id from [`crate::kind`].
    pub kind: u16,
    /// postcard bytes of the kind's head fields.
    pub params: Blob,
    /// Commitment to the content, `None` for entries without content.
    pub content: Option<ContentRef>,
}

impl Head {
    pub fn hash(&self) -> Result<Hash, postcard::Error> {
        let bytes = postcard::to_allocvec(&(PROTOCOL_VERSION, self))?;
        let mut h = Sha256::new();
        h.update(ENTRY_DOMAIN);
        h.update(&bytes);
        Ok(Hash(h.finalize().into()))
    }
}

/// What the head commits to: Merkle root over the parts, their number and total length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRef {
    pub root: Hash,
    pub parts: u32,
    pub len: u32,
}

impl ContentRef {
    pub fn of(parts: &[Part]) -> Self {
        let leaves: Vec<Hash> = parts
            .iter()
            .map(|p| merkle::leaf(&p.salt, &p.data.0))
            .collect();
        ContentRef {
            root: merkle::root(&leaves),
            parts: parts.len() as u32,
            len: parts.iter().map(|p| p.data.0.len() as u32).sum(),
        }
    }
}

/// One removable piece of content. The salt goes with it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub salt: Salt,
    pub data: Blob,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub head: Head,
    /// `None` once removed (or never had any) — the entry stays valid.
    pub content: Option<Vec<Part>>,
    pub hash: Hash,
    pub sig: Sig,
}

impl LedgerEntry {
    /// Recomputes the hash from the signed head (ignores the stored `hash`).
    pub fn head_hash(&self) -> Result<Hash, postcard::Error> {
        self.head.hash()
    }

    /// True when content is missing although the head commits to some.
    pub fn is_pruned(&self) -> bool {
        self.head.content.is_some() && self.content.is_none()
    }

    /// Content that is present must match the root in the head. Missing content is fine.
    pub fn content_matches(&self) -> bool {
        match (&self.head.content, &self.content) {
            (_, None) => true,
            (Some(r), Some(parts)) => ContentRef::of(parts) == *r,
            (None, Some(_)) => false,
        }
    }

    /// The keys of a raw-data batch (E14), read from the head: still there when the content
    /// was removed. Empty for every other kind.
    pub fn evidence_keys(&self) -> Result<Vec<Hash>, Decode> {
        if self.head.kind != kind::EVIDENCE_BATCH {
            return Ok(Vec::new());
        }
        dec(&self.head.params.0)
    }

    /// Reads the entry as an [`Op`]. Removed content reads as empty title and body —
    /// check [`LedgerEntry::is_pruned`] to tell the two apart.
    pub fn op(&self) -> Result<Op, Decode> {
        let p = &self.head.params.0[..];
        Ok(match self.head.kind {
            kind::CREATE_NODE => {
                let (title, body) = self.text()?;
                Op::CreateNode {
                    kind: dec(p)?,
                    title,
                    body,
                }
            }
            kind::UPDATE_NODE => {
                let (title, body) = self.text()?;
                let (node, base) = match dec::<(NodeId, Hash)>(p) {
                    Ok((node, base)) => (node, Some(base)),
                    Err(_) => (dec(p)?, None),
                };
                Op::UpdateNode {
                    node,
                    title,
                    body,
                    base,
                }
            }
            kind::LINK => {
                let (from, to, kind) = dec(p)?;
                Op::Link { from, to, kind }
            }
            kind::ACCEPT_PROPOSAL => Op::AcceptProposal {
                proposal: self.proposal()?,
            },
            kind::REJECT_PROPOSAL => Op::RejectProposal {
                proposal: self.proposal()?,
            },
            kind::MARK_SENSITIVE => {
                let (node, sensitive) = dec(p)?;
                Op::MarkSensitive { node, sensitive }
            }
            kind::DELETE_NODE => Op::DeleteNode { node: dec(p)? },
            kind::UNLINK => Op::Unlink { edge: dec(p)? },
            kind::SET_POLICY => {
                let (node, deletable, exportable) = dec(p)?;
                Op::SetPolicy {
                    node,
                    deletable,
                    exportable,
                }
            }
            kind::CERTIFY_DEVICE => {
                let (device, name, person, sig) = dec(p)?;
                Op::CertifyDevice {
                    device,
                    name,
                    person,
                    sig,
                }
            }
            kind::REVOKE_DEVICE => {
                let (device, last_seq, person, sig) = dec(p)?;
                Op::RevokeDevice {
                    device,
                    last_seq,
                    person,
                    sig,
                }
            }
            kind::ADD_SENSITIVITY_RULE => {
                let (label, keywords, model) = dec(p)?;
                Op::AddSensitivityRule {
                    label,
                    keywords,
                    model,
                }
            }
            kind::REMOVE_SENSITIVITY_RULE => Op::RemoveSensitivityRule { rule: dec(p)? },
            kind::EVIDENCE_BATCH => {
                let keys: Vec<Hash> = dec(p)?;
                let items = match &self.content {
                    None => Vec::new(),
                    Some(parts) if parts.len() == keys.len() => parts
                        .iter()
                        .map(|part| dec(&part.data.0))
                        .collect::<Result<_, _>>()?,
                    Some(_) => return Err(Decode::Malformed),
                };
                Op::RecordEvidence { items }
            }
            // A decision whose content was removed no longer says anything: it is skipped.
            kind::DECIDE_IDENTITY => match self.content.as_deref() {
                Some([part]) => Op::DecideIdentity {
                    decision: dec(&part.data.0)?,
                },
                _ => return Err(Decode::Malformed),
            },
            other => return Err(Decode::Unknown(other)),
        })
    }

    fn text(&self) -> Result<(String, String), Decode> {
        if self.head.content.is_none() {
            return Err(Decode::Malformed);
        }
        match self.content.as_deref() {
            None => Ok((String::new(), String::new())),
            Some([part]) => dec(&part.data.0),
            Some(_) => Err(Decode::Malformed),
        }
    }

    fn proposal(&self) -> Result<Proposal, Decode> {
        let (goal, sources, kind, model) = dec(&self.head.params.0)?;
        let (title, body) = self.text()?;
        Ok(Proposal {
            goal,
            kind,
            title,
            body,
            sources,
            model,
        })
    }
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

    /// Signs a message that is not an entry, e.g. a sync request. The message must start with
    /// its own domain (like `symbiot/sync-auth/v1`) so it can never pass for an entry hash.
    pub fn sign_message(&self, message: &[u8]) -> Sig {
        debug_assert!(message.starts_with(b"symbiot/"), "messages carry a domain");
        Sig(self.key.sign(message).to_bytes())
    }

    /// Appends `op` after `prev` (the current tip of this device's chain, `None` for the first entry).
    pub fn append(&self, prev: Option<&LedgerEntry>, ts_ms: u64, op: Op) -> LedgerEntry {
        let (kind, params, parts) = op.encode();
        self.sign(prev, ts_ms, kind, params, parts)
    }

    /// Signs an entry of any kind with the given content parts (none → no content).
    fn sign(
        &self,
        prev: Option<&LedgerEntry>,
        ts_ms: u64,
        kind: u16,
        params: Vec<u8>,
        parts: Vec<Vec<u8>>,
    ) -> LedgerEntry {
        let author = self.public();
        let (seq, prev_hash) = match prev {
            Some(p) => {
                debug_assert_eq!(p.head.author, author, "a chain has exactly one writer");
                (p.head.seq + 1, p.hash)
            }
            None => (0, Hash::ZERO),
        };
        let content: Option<Vec<Part>> = (!parts.is_empty()).then(|| {
            parts
                .into_iter()
                .enumerate()
                .map(|(i, data)| Part {
                    salt: self.salt(seq, i as u32),
                    data: Blob(data),
                })
                .collect()
        });
        let head = Head {
            author,
            seq,
            prev: prev_hash,
            ts_ms,
            kind,
            params: Blob(params),
            content: content.as_deref().map(ContentRef::of),
        };
        let hash = head
            .hash()
            .expect("postcard serialization into a Vec cannot fail");
        let sig = Sig(self.key.sign(&hash.0).to_bytes());
        LedgerEntry {
            head,
            content,
            hash,
            sig,
        }
    }

    /// Derived from the device secret, so no RNG is needed (same on a microcontroller).
    /// Unguessable for anyone without the key — and whoever has it can sign anyway.
    fn salt(&self, seq: u64, part: u32) -> Salt {
        let mut h = Sha256::new();
        h.update(SALT_DOMAIN);
        h.update(self.key.to_bytes());
        h.update(seq.to_le_bytes());
        h.update(part.to_le_bytes());
        let digest: [u8; 32] = h.finalize().into();
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&digest[..16]);
        Salt(salt)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyError {
    /// Entries missing, duplicated or reordered.
    WrongSeq { expected: u64, found: u64 },
    /// `prev` does not point to the previous entry's hash.
    BrokenLink { seq: u64 },
    /// The head was changed after signing.
    BadHash { seq: u64 },
    /// Signature does not match the author's key.
    BadSignature { seq: u64 },
    /// A different device wrote into this chain.
    ForeignAuthor { seq: u64 },
    /// Content is present but does not match what the head committed to.
    BadContent { seq: u64 },
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
                write!(f, "Eintrag {seq}: Kopf wurde nachträglich verändert")
            }
            VerifyError::BadSignature { seq } => write!(f, "Eintrag {seq}: Signatur ungültig"),
            VerifyError::ForeignAuthor { seq } => {
                write!(f, "Eintrag {seq}: stammt von einem anderen Gerät")
            }
            VerifyError::BadContent { seq } => {
                write!(f, "Eintrag {seq}: Inhalt wurde nachträglich verändert")
            }
        }
    }
}

/// Checks a signature made with [`Signer::sign_message`].
pub fn verify_message(key: &PubKey, message: &[u8], sig: &Sig) -> bool {
    VerifyingKey::from_bytes(&key.0)
        .and_then(|k| k.verify_strict(message, &Signature::from_bytes(&sig.0)))
        .is_ok()
}

/// Verifies one device's chain from its first entry. Returns the number of entries.
/// Entries whose content was removed stay valid.
pub fn verify_chain(entries: &[LedgerEntry]) -> Result<usize, VerifyError> {
    let Some(first) = entries.first() else {
        return Ok(0);
    };
    let author = first.head.author;
    let key =
        VerifyingKey::from_bytes(&author.0).map_err(|_| VerifyError::BadSignature { seq: 0 })?;

    let mut prev = Hash::ZERO;
    for (i, e) in entries.iter().enumerate() {
        let h = &e.head;
        let expected = i as u64;
        if h.seq != expected {
            return Err(VerifyError::WrongSeq {
                expected,
                found: h.seq,
            });
        }
        if h.author != author {
            return Err(VerifyError::ForeignAuthor { seq: h.seq });
        }
        if h.prev != prev {
            return Err(VerifyError::BrokenLink { seq: h.seq });
        }
        if e.head_hash().ok() != Some(e.hash) {
            return Err(VerifyError::BadHash { seq: h.seq });
        }
        let sig = Signature::from_bytes(&e.sig.0);
        if key.verify_strict(&e.hash.0, &sig).is_err() {
            return Err(VerifyError::BadSignature { seq: h.seq });
        }
        if !e.content_matches() {
            return Err(VerifyError::BadContent { seq: h.seq });
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
    /// Entries whose content was removed (E2) — still verified by head and signature.
    pub pruned: usize,
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
            pruned: entries.iter().filter(|e| e.is_pruned()).count(),
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
        assert_eq!(e.head.seq, 0);
        assert_eq!(e.head.prev, Hash::ZERO);
        assert_eq!(e.head.kind, kind::CREATE_NODE);
        assert_eq!(e.head_hash().unwrap(), e.hash);
        assert_eq!(
            e.hash.to_hex(),
            "4ea40b02ea11acaaf996158ac2041e2574450dd19887ebc0eea98b54a0e64f6e"
        );
    }

    #[test]
    fn every_op_round_trips() {
        let s = signer();
        let a = NodeId([1; 16]);
        let b = NodeId([2; 16]);
        let proposal = Proposal {
            goal: a,
            kind: NodeKind::Goal,
            title: "Laufschuhe kaufen".into(),
            body: "Alte sind durch".into(),
            sources: alloc::vec![a, b],
            model: "qwen3:8b".into(),
        };
        let ops = [
            create("Ziel"),
            Op::UpdateNode {
                node: a,
                title: "Ziel 2".into(),
                body: "mehr".into(),
                base: Some(Hash([4; 32])),
            },
            Op::Link {
                from: b,
                to: a,
                kind: EdgeKind::Supports,
            },
            Op::AcceptProposal {
                proposal: proposal.clone(),
            },
            Op::RejectProposal { proposal },
            Op::MarkSensitive {
                node: a,
                sensitive: true,
            },
            Op::DeleteNode { node: b },
            Op::Unlink {
                edge: EdgeId([3; 16]),
            },
            Op::SetPolicy {
                node: a,
                deletable: false,
                exportable: true,
            },
        ];
        let c = build_chain(&s, ops.iter().cloned().map(|op| (1, op)));
        for (e, op) in c.iter().zip(&ops) {
            assert_eq!(e.op().as_ref(), Ok(op));
        }
        assert_eq!(verify_chain(&c), Ok(ops.len()));
        // Links and markers carry no content.
        assert!(c[2].content.is_none() && c[5].content.is_none());
    }

    /// Updates written before M4 have no base; their params keep the old form and still decode.
    #[test]
    fn updates_without_base_keep_the_old_params() {
        let node = NodeId([5; 16]);
        let op = Op::UpdateNode {
            node,
            title: "alt".into(),
            body: String::new(),
            base: None,
        };
        let e = signer().append(None, 1, op.clone());
        assert_eq!(e.head.params.0, enc(&node));
        assert_eq!(e.op(), Ok(op));
    }

    #[test]
    fn removed_content_keeps_the_chain_valid() {
        let mut c = chain(3);
        c[1].content = None;
        assert_eq!(verify_chain(&c), Ok(3));
        assert!(c[1].is_pruned());
        // The head still says what kind of entry it was; title and body are gone.
        assert_eq!(
            c[1].op(),
            Ok(Op::CreateNode {
                kind: NodeKind::Goal,
                title: String::new(),
                body: String::new(),
            })
        );
        assert_eq!(VerifyReport::of(signer().public(), &c).pruned, 1);
    }

    #[test]
    fn content_is_salted() {
        // Same text, different entries: the leaves differ, so a removed text cannot be guessed.
        let c = build_chain(&signer(), [(1, create("ja")), (2, create("ja"))]);
        assert_ne!(c[0].head.content, c[1].head.content);
        let other = Signer::from_seed([8; 32]).append(None, 1, create("ja"));
        assert_ne!(c[0].head.content, other.head.content);
    }

    #[test]
    fn unknown_kinds_verify_but_do_not_decode() {
        let s = signer();
        let a = s.append(None, 1, create("Ziel"));
        let future = s.sign(Some(&a), 2, 999, alloc::vec![1, 2, 3], alloc::vec![]);
        let c = alloc::vec![a, future];
        assert_eq!(verify_chain(&c), Ok(2));
        assert_eq!(c[1].op(), Err(Decode::Unknown(999)));
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
        c[2].head.seq = 2;
        assert_eq!(verify_chain(&c), Err(VerifyError::ForeignAuthor { seq: 2 }));
    }

    #[test]
    fn json_round_trip_keeps_chain_valid() {
        let c = chain(3);
        let json = serde_json::to_string(&c).unwrap();
        let back: Vec<LedgerEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(verify_chain(&back), Ok(3));
        assert_eq!(back, c);
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
        Content(usize, String),
        Timestamp(usize, u64),
        Kind(usize, u16),
        ParamsBit(usize, usize),
        RootBit(usize, usize),
        HashBit(usize, usize),
        SigBit(usize, usize),
        Drop(usize),
        Swap(usize, usize),
        Duplicate(usize),
    }

    fn tamper() -> impl Strategy<Value = Tamper> {
        let i = 0usize..64;
        prop_oneof![
            (i.clone(), "[a-z]{1,8}").prop_map(|(i, s)| Tamper::Content(i, s)),
            (i.clone(), any::<u64>()).prop_map(|(i, t)| Tamper::Timestamp(i, t)),
            (i.clone(), any::<u16>()).prop_map(|(i, k)| Tamper::Kind(i, k)),
            (i.clone(), 0usize..8).prop_map(|(i, b)| Tamper::ParamsBit(i, b)),
            (i.clone(), 0usize..256).prop_map(|(i, b)| Tamper::RootBit(i, b)),
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
                Tamper::Content(i, s) => {
                    let parts = c[i % n].content.as_mut().unwrap();
                    parts[0].data.0.extend_from_slice(s.as_bytes());
                }
                Tamper::Timestamp(i, ts) => {
                    let i = i % n;
                    prop_assume!(c[i].head.ts_ms != ts);
                    c[i].head.ts_ms = ts;
                }
                Tamper::Kind(i, k) => {
                    let i = i % n;
                    prop_assume!(c[i].head.kind != k);
                    c[i].head.kind = k;
                }
                Tamper::ParamsBit(i, b) => { c[i % n].head.params.0[0] ^= 1 << b; }
                Tamper::RootBit(i, b) => {
                    let r = c[i % n].head.content.as_mut().unwrap();
                    r.root.0[b / 8] ^= 1 << (b % 8);
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
