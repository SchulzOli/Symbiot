//! M4: the person key (E5) and exchanging chains with the other devices (F5).
//!
//! A device belongs to a person once its own chain holds a certificate signed with the person
//! key. The key is derived from 24 recovery words when needed and never stored. Chains of other
//! devices are only taken in if they verify, attach to what is already known, and belong to the
//! same person as this device.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use symbiot_core::{Hash, LedgerEntry, Person, PubKey, Trust, kind, verify_after};

use crate::{
    Error, Result, Store, decode_entry, device_entries, insert_entry, node_ids, raw_entry,
    rebuild_projection,
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

impl Store {
    /// The person this device belongs to, if it was set up.
    pub fn person(&self) -> Result<Option<PubKey>> {
        let me = self.device();
        let mut own = device_entries(&self.conn)?;
        own.retain(|e| e.head.author == me);
        Ok(Trust::person_of(&own))
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
        // Only certificates and revocations: trust never reads what the devices wrote (H3).
        let entries = device_entries(&self.conn)?;
        Ok(Some(Trust::of(person, [entries.as_slice()])))
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
                let last_ms: Option<i64> = self
                    .conn
                    .query_row(
                        "SELECT ts_ms FROM chain_tip WHERE author = ?1",
                        [d.key.0.as_slice()],
                        |r| r.get(0),
                    )
                    .optional()?;
                let last_ms = last_ms.map(|t| t as u64);
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
        let mut stmt = self
            .conn
            .prepare("SELECT author, seq, hash FROM chain_tip ORDER BY author")?;
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
        Ok(read_tip(&self.conn, author)?.map(|(seq, hash)| ChainHead {
            author: *author,
            seq,
            hash,
        }))
    }

    /// The entries of `author`'s chain after `after` (all if `None`), ready to send. Content of
    /// nodes that are not exportable stays here (E3/E5); the receiver sees why from the policy.
    pub fn chain_since(&self, author: &PubKey, after: Option<u64>) -> Result<Vec<LedgerEntry>> {
        self.chain_page(author, after, usize::MAX)
    }

    /// Like [`Store::chain_since`], but at most `limit` entries: a long chain is fetched in
    /// pages, as it is sent in pages (H3).
    pub fn chain_page(
        &self,
        author: &PubKey,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<LedgerEntry>> {
        let home = node_ids(
            &self.conn,
            "SELECT node_id FROM policies WHERE exportable = 0",
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT head, hash, sig, content FROM ledger
             WHERE author = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![
                author.0.as_slice(),
                after.map_or(-1, |a| a as i64),
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            raw_entry,
        )?;
        let mut chain: Vec<LedgerEntry> = rows.map(|r| decode_entry(r?)).collect::<Result<_>>()?;
        for e in &mut chain {
            // One rule for every kind of entry (H2): whatever speaks about a node that is not
            // exportable keeps its content here — its versions, proposals, suggestions.
            if e.about().iter().any(|n| home.contains(n)) {
                e.content = None;
            }
        }
        Ok(chain)
    }

    /// F5: takes in entries of another device's chain. They must verify, attach to what is
    /// known, and come from a device of the same person; entries after a revocation are left
    /// out. A second, validly signed entry at a known position is a fork: kept as evidence
    /// and refused. Returns how many entries were new.
    ///
    /// H3: the cost depends on the new entries, not on what is stored. They are verified
    /// against the tip of the chain (the rest was verified when it came in), trust reads only
    /// certificates and revocations, and raw data never touches the graph: a batch goes
    /// straight into its table, and only entries of the graph rebuild the projection.
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
        let tip = read_tip(&tx, &author)?;
        let next = tip.map_or(0, |(seq, _)| seq + 1);
        let mut new: Vec<LedgerEntry> = Vec::new();
        for e in entries {
            let seq = e.head.seq;
            if seq < next {
                if hash_at(&tx, &author, seq)? == Some(e.hash) {
                    continue;
                }
                // Another entry at a known position. If it is validly signed onto what comes
                // before it, the device key wrote two histories.
                let before = match seq {
                    0 => None,
                    _ => hash_at(&tx, &author, seq - 1)?.map(|h| (seq - 1, h)),
                };
                let attaches = seq == 0 || before.is_some();
                if attaches && verify_after(&author, before, std::slice::from_ref(&e)).is_ok() {
                    tx.execute(
                        "INSERT OR IGNORE INTO forks (author, seq, hash, entry) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            author.0.as_slice(),
                            seq as i64,
                            e.hash.0.as_slice(),
                            postcard::to_allocvec(&e)?
                        ],
                    )?;
                    tx.commit()?;
                    return Err(Error::Fork { author, seq });
                }
                return Err(Error::BadChain(format!("Eintrag {seq} passt nicht")));
            }
            let expected = next + new.len() as u64;
            if seq != expected {
                return Err(Error::BadChain(format!(
                    "Eintrag {expected} erwartet, {seq} bekommen"
                )));
            }
            new.push(e);
        }
        if new.is_empty() {
            return Ok(0);
        }
        verify_after(&author, tip, &new).map_err(|e| Error::BadChain(e.to_string()))?;

        // Who counts: the certificates and revocations known here, plus those that just came.
        let is_device =
            |e: &LedgerEntry| matches!(e.head.kind, kind::CERTIFY_DEVICE | kind::REVOKE_DEVICE);
        let mut devices = device_entries(&tx)?;
        devices.extend(new.iter().filter(|e| is_device(e)).cloned());
        let trust = Trust::of(person, [devices.as_slice()]);
        let accepted: Vec<LedgerEntry> = new
            .into_iter()
            .take_while(|e| trust.accepts(&author, e.head.seq))
            .collect();
        if accepted.is_empty() {
            return Err(Error::Untrusted);
        }
        let mut graph = false;
        for e in &accepted {
            insert_entry(&tx, e)?;
            if e.head.kind == kind::EVIDENCE_BATCH {
                crate::evidence::receive_evidence(&tx, e)?;
            } else {
                graph = true;
            }
        }
        // Raw data of a source revoked meanwhile must not stay (E2).
        let revoked = crate::sources::any_revoked(&tx)?;
        if graph {
            rebuild_projection(&tx, &me)?;
        } else if revoked {
            crate::sources::prune_revoked(&tx)?;
        }
        tx.commit()?;
        if graph || revoked {
            // Content removed by a deletion must not survive in the write-ahead log.
            self.conn
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        }
        Ok(accepted.len())
    }
}

/// Where `author`'s chain stands here: its last entry as `(seq, hash)`.
fn read_tip(conn: &Connection, author: &PubKey) -> Result<Option<(u64, Hash)>> {
    conn.query_row(
        "SELECT seq, hash FROM chain_tip WHERE author = ?1",
        [author.0.as_slice()],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
    )
    .optional()?
    .map(|(seq, hash)| Ok((seq as u64, as_hash(hash)?)))
    .transpose()
}

/// The hash of entry `seq` of `author`'s chain, if it is stored here.
fn hash_at(conn: &Connection, author: &PubKey, seq: u64) -> Result<Option<Hash>> {
    conn.query_row(
        "SELECT hash FROM ledger WHERE author = ?1 AND seq = ?2",
        params![author.0.as_slice(), seq as i64],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()?
    .map(as_hash)
    .transpose()
}

fn as_hash(bytes: Vec<u8>) -> Result<Hash> {
    Ok(Hash(bytes.try_into().map_err(|_| {
        Error::Invalid("ungültiger Hash in der Datenbank")
    })?))
}
