//! M4: the person key (E5) and exchanging chains with the other devices (F5).
//!
//! A device belongs to a person once its own chain holds a certificate signed with the person
//! key. The key is derived from 24 recovery words when needed and never stored. Chains of other
//! devices are only taken in if they verify, attach to what is already known, and belong to the
//! same person as this device.

use serde::{Deserialize, Serialize};
use symbiot_core::{Hash, LedgerEntry, NodeId, Op, Person, PubKey, Trust, verify_chain};

use crate::{
    Error, Result, Store, all_chains, chain_of, insert_entry, node_ids, rebuild_projection,
};

/// Where a chain stands: its last entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainHead {
    pub author: PubKey,
    pub seq: u64,
    pub hash: Hash,
}

/// A device of the person, as the chains on this device know it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub key: PubKey,
    pub name: String,
    pub revoked_after: Option<u64>,
    /// Entries of its chain known here.
    pub entries: u64,
    /// Time of its newest entry known here.
    pub last_ms: Option<u64>,
    pub this_device: bool,
}

/// 24 new recovery words for a new person key. Nothing is stored: show them once.
pub fn new_words() -> Result<String> {
    let mut entropy = [0u8; 32];
    getrandom::fill(&mut entropy).map_err(|_| Error::Invalid("kein Zufall verfügbar"))?;
    let words = bip39::Mnemonic::from_entropy(&entropy)
        .map_err(|_| Error::Invalid("Wörter nicht erzeugbar"))?;
    Ok(words.to_string())
}

/// The person key behind the words. Spaces, line breaks and case do not matter.
fn person_from(words: &str) -> Result<Person> {
    let normal = words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let m = bip39::Mnemonic::parse_normalized(&normal)
        .map_err(|_| Error::Invalid("Die Wörter stimmen nicht (24 Wörter, Prüfsumme)"))?;
    let entropy: [u8; 32] = m
        .to_entropy()
        .try_into()
        .map_err(|_| Error::Invalid("Es braucht 24 Wörter"))?;
    Ok(Person::from_entropy(entropy))
}

/// The node whose content an entry carries — to keep what is not exportable at home.
fn content_node(e: &LedgerEntry) -> Option<NodeId> {
    match e.op().ok()? {
        Op::CreateNode { .. } | Op::AcceptProposal { .. } => Some(NodeId::from_entry_hash(&e.hash)),
        Op::UpdateNode { node, .. } => Some(node),
        Op::RejectProposal { proposal } => Some(proposal.goal),
        _ => None,
    }
}

impl Store {
    /// The person this device belongs to, if it was set up.
    pub fn person(&self) -> Result<Option<PubKey>> {
        Ok(Trust::person_of(&chain_of(&self.conn, &self.device())?))
    }

    /// E5: confirms this device for the person behind `words`. Repeating it is harmless; a
    /// device that already belongs to another person cannot be taken over.
    pub fn certify_self(&mut self, words: &str, name: &str) -> Result<PubKey> {
        let person = person_from(words)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("Das Gerät braucht einen Namen"));
        }
        match self.person()? {
            Some(p) if p == person.public() => return Ok(p),
            Some(_) => {
                return Err(Error::Invalid(
                    "Dieses Gerät gehört schon zu einem anderen Personen-Schlüssel",
                ));
            }
            None => {}
        }
        self.append(person.certify(self.device(), name))?;
        // Chains received earlier may count now.
        let me = self.device();
        let tx = self.conn.transaction()?;
        rebuild_projection(&tx, &me)?;
        tx.commit()?;
        Ok(person.public())
    }

    /// E5: a lost or stolen device. Its entries after the last one known here no longer count,
    /// on every device that learns about the revocation.
    pub fn revoke_device(&mut self, words: &str, device: PubKey) -> Result<()> {
        let person = person_from(words)?;
        if self.person()? != Some(person.public()) {
            return Err(Error::Invalid("Die Wörter gehören nicht zu diesem Gerät"));
        }
        if device == self.device() {
            return Err(Error::Invalid(
                "Dieses Gerät kann sich nicht selbst widerrufen",
            ));
        }
        let last = self
            .tip(&device)?
            .ok_or(Error::Invalid("Dieses Gerät ist hier nicht bekannt"))?;
        self.append(person.revoke(device, last.seq))?;
        let me = self.device();
        let tx = self.conn.transaction()?;
        rebuild_projection(&tx, &me)?;
        tx.commit()?;
        Ok(())
    }

    fn trust(&self) -> Result<Option<Trust>> {
        let Some(person) = self.person()? else {
            return Ok(None);
        };
        let chains = all_chains(&self.conn)?;
        Ok(Some(Trust::of(person, chains.iter().map(Vec::as_slice))))
    }

    /// Whether `device` belongs to the same person as this one and is not revoked.
    pub fn trusts(&self, device: &PubKey) -> Result<bool> {
        Ok(self.trust()?.is_some_and(|t| {
            t.devices
                .get(device)
                .is_some_and(|d| d.revoked_after.is_none())
        }))
    }

    /// The devices of this person known here.
    pub fn devices(&self) -> Result<Vec<DeviceInfo>> {
        let Some(trust) = self.trust()? else {
            return Ok(Vec::new());
        };
        trust
            .devices
            .into_values()
            .map(|d| {
                let tip = self.tip(&d.key)?;
                let last_ms = match &tip {
                    Some(_) => chain_of(&self.conn, &d.key)?.last().map(|e| e.head.ts_ms),
                    None => None,
                };
                Ok(DeviceInfo {
                    this_device: d.key == self.device(),
                    key: d.key,
                    name: d.name,
                    revoked_after: d.revoked_after,
                    entries: tip.map_or(0, |t| t.seq + 1),
                    last_ms,
                })
            })
            .collect()
    }

    /// The last entry of every chain known here.
    pub fn heads(&self) -> Result<Vec<ChainHead>> {
        let mut stmt = self.conn.prepare(
            "SELECT author, seq, hash FROM ledger AS l
             WHERE seq = (SELECT MAX(seq) FROM ledger WHERE author = l.author)
             ORDER BY author",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        rows.map(|r| {
            let (author, seq, hash) = r?;
            let bad = || Error::Invalid("ungültiger Schlüssel in der Datenbank");
            Ok(ChainHead {
                author: PubKey(author.try_into().map_err(|_| bad())?),
                seq: seq as u64,
                hash: Hash(hash.try_into().map_err(|_| bad())?),
            })
        })
        .collect()
    }

    pub fn tip(&self, author: &PubKey) -> Result<Option<ChainHead>> {
        Ok(self.heads()?.into_iter().find(|h| h.author == *author))
    }

    /// The entries of `author`'s chain after `after` (all if `None`), ready to send. Content of
    /// nodes that are not exportable stays here (E3/E5); the receiver sees why from the policy.
    pub fn chain_since(&self, author: &PubKey, after: Option<u64>) -> Result<Vec<LedgerEntry>> {
        let home = node_ids(
            &self.conn,
            "SELECT node_id FROM policies WHERE exportable = 0",
        )?;
        let mut chain = chain_of(&self.conn, author)?;
        chain.retain(|e| after.is_none_or(|a| e.head.seq > a));
        for e in &mut chain {
            if content_node(e).is_some_and(|n| home.contains(&n)) {
                e.content = None;
            }
        }
        Ok(chain)
    }

    /// F5: takes in entries of another device's chain. They must verify, attach to what is
    /// known, and come from a device of the same person; entries after a revocation are left
    /// out. A second, validly signed entry at a known position is a fork: kept as evidence
    /// and refused. Returns how many entries were new.
    pub fn receive(&mut self, entries: Vec<LedgerEntry>) -> Result<usize> {
        let Some(first) = entries.first() else {
            return Ok(0);
        };
        let author = first.head.author;
        if author == self.device() {
            return Err(Error::BadChain(
                "die eigene Kette schreibt nur dieses Gerät".into(),
            ));
        }
        if entries.iter().any(|e| e.head.author != author) {
            return Err(Error::BadChain("Einträge aus mehreren Ketten".into()));
        }
        let me = self.device();
        let person = self.person()?.ok_or(Error::Untrusted)?;
        let tx = self.conn.transaction()?;
        let known = chain_of(&tx, &author)?;
        let mut new: Vec<LedgerEntry> = Vec::new();
        for e in entries {
            let seq = e.head.seq as usize;
            if let Some(old) = known.get(seq) {
                if old.hash == e.hash {
                    continue;
                }
                let mut alternative = known[..seq].to_vec();
                alternative.push(e.clone());
                if verify_chain(&alternative).is_ok() {
                    tx.execute(
                        "INSERT OR IGNORE INTO forks (author, seq, hash, entry) VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![
                            author.0.as_slice(),
                            seq as i64,
                            e.hash.0.as_slice(),
                            postcard::to_allocvec(&e)?
                        ],
                    )?;
                    tx.commit()?;
                    return Err(Error::Fork {
                        author,
                        seq: seq as u64,
                    });
                }
                return Err(Error::BadChain(format!("Eintrag {seq} passt nicht")));
            }
            if seq != known.len() + new.len() {
                return Err(Error::BadChain(format!(
                    "Eintrag {} erwartet, {seq} bekommen",
                    known.len() + new.len()
                )));
            }
            new.push(e);
        }
        if new.is_empty() {
            return Ok(0);
        }
        let mut full = known;
        full.extend(new.iter().cloned());
        verify_chain(&full).map_err(|e| Error::BadChain(e.to_string()))?;

        let mut chains = all_chains(&tx)?;
        chains.retain(|c| c[0].head.author != author);
        chains.push(full);
        let trust = Trust::of(person, chains.iter().map(Vec::as_slice));
        let accepted: Vec<LedgerEntry> = new
            .into_iter()
            .take_while(|e| trust.accepts(&author, e.head.seq))
            .collect();
        if accepted.is_empty() {
            return Err(Error::Untrusted);
        }
        for e in &accepted {
            insert_entry(&tx, e)?;
        }
        rebuild_projection(&tx, &me)?;
        tx.commit()?;
        // Content removed by a deletion must not survive in the write-ahead log.
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(accepted.len())
    }
}
