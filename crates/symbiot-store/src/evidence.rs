//! Raw data (E14): the evidence table, filled from `RecordEvidence` entries of every chain.
//!
//! Adapters are devices of their own (E7): an adapter opens its own store with its own key and
//! calls [`Store::record_evidence`]; its chain reaches other devices through the normal sync.
//! The table is a projection like the graph tables. A row whose content was removed keeps its
//! key, so the same item is not loaded again.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use symbiot_core::evidence::{self, Evidence};
use symbiot_core::{Change, Hash, LedgerEntry, Op, PubKey, kind};

use crate::{Error, Known, Result, Store, all_chains, append_in};

/// What [`Store::record_evidence`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Recorded {
    /// New items, now in the chain.
    pub added: usize,
    /// Items that were already known (loaded before, deleted before, or twice in the input).
    pub known: usize,
    /// Batches written to the chain.
    pub entries: usize,
}

/// Which evidence to read. Every field narrows the result; `Default` reads everything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvidenceQuery {
    /// Vocabulary term, e.g. `schema:Message`.
    pub kind: Option<String>,
    /// From this time on (inclusive, ms since the epoch).
    pub from_ms: Option<u64>,
    /// Before this time (exclusive).
    pub to_ms: Option<u64>,
    pub limit: Option<usize>,
}

/// One item as stored, with where it came from.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StoredEvidence {
    pub key: Hash,
    /// The batch entry that holds it.
    pub entry: Hash,
    /// The adapter device that loaded it.
    pub author: PubKey,
    pub evidence: Evidence,
}

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS evidence (
        key    BLOB PRIMARY KEY,
        entry  BLOB NOT NULL,
        part   INTEGER NOT NULL,
        author BLOB NOT NULL,
        kind   TEXT,
        at_ms  INTEGER,
        item   BLOB
    );
    CREATE INDEX IF NOT EXISTS evidence_time ON evidence (at_ms);
    CREATE INDEX IF NOT EXISTS evidence_kind ON evidence (kind, at_ms);";

impl Store {
    /// Loads raw data into this device's chain, in batches (E13). Items whose key is already
    /// known are left out, so a second import of the same data adds nothing. All or nothing:
    /// if one item breaks the contract, nothing is written.
    pub fn record_evidence(&mut self, items: Vec<Evidence>) -> Result<Recorded> {
        let tx = self.conn.transaction()?;
        let mut seen = HashSet::new();
        let mut fresh = Vec::new();
        let mut out = Recorded::default();
        for item in items {
            item.check().map_err(|reason| Error::BadEvidence {
                id: item.id.clone(),
                reason: reason.to_string(),
            })?;
            let key = item.key();
            if !seen.insert(key) || is_known(&tx, &key)? {
                out.known += 1;
                continue;
            }
            fresh.push(item);
        }
        out.added = fresh.len();
        for batch in evidence::batches(fresh) {
            append_in(&tx, &self.signer, Op::RecordEvidence { items: batch })?;
            out.entries += 1;
        }
        tx.commit()?;
        Ok(out)
    }

    /// Evidence that still has its content, newest first.
    pub fn evidence(&self, q: &EvidenceQuery) -> Result<Vec<StoredEvidence>> {
        let mut stmt = self.conn.prepare(
            "SELECT key, entry, author, item FROM evidence
             WHERE item IS NOT NULL
               AND (?1 IS NULL OR kind = ?1)
               AND (?2 IS NULL OR at_ms >= ?2)
               AND (?3 IS NULL OR at_ms < ?3)
             ORDER BY at_ms DESC, key
             LIMIT ?4",
        )?;
        let limit = q.limit.map_or(-1, |l| l as i64);
        let rows = stmt.query_map(
            params![
                q.kind,
                q.from_ms.map(|t| t as i64),
                q.to_ms.map(|t| t as i64),
                limit
            ],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, Vec<u8>>(3)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (key, entry, author, item) = row?;
            Ok(StoredEvidence {
                key: Hash(fixed(&key)?),
                entry: Hash(fixed(&entry)?),
                author: PubKey(fixed(&author)?),
                evidence: postcard::from_bytes(&item)?,
            })
        })
        .collect()
    }

    /// Number of items with content, and of keys whose content was removed.
    pub fn evidence_count(&self) -> Result<(usize, usize)> {
        Ok(self.conn.query_row(
            "SELECT COUNT(item), COUNT(*) - COUNT(item) FROM evidence",
            [],
            |r| Ok((r.get::<_, i64>(0)? as usize, r.get::<_, i64>(1)? as usize)),
        )?)
    }
}

fn fixed<const N: usize>(b: &[u8]) -> Result<[u8; N]> {
    b.try_into()
        .map_err(|_| Error::Invalid("Schlüssel in der Datenbank hat die falsche Länge"))
}

fn is_known(conn: &Connection, key: &Hash) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM evidence WHERE key = ?1",
            [key.0.as_slice()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Writes the rows of a `Change::Evidence`. The first chain to load an item keeps it.
pub(crate) fn project_evidence(tx: &Connection, change: &Change) -> Result<()> {
    let Change::Evidence {
        entry,
        author,
        keys,
        items,
    } = change
    else {
        return Ok(());
    };
    for (part, key) in keys.iter().enumerate() {
        let item = items.get(part);
        tx.execute(
            "INSERT OR IGNORE INTO evidence (key, entry, part, author, kind, at_ms, item)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                key.0.as_slice(),
                entry.0.as_slice(),
                part as i64,
                author.0.as_slice(),
                item.map(|i| i.kind.as_str()),
                item.map(|i| i.at_ms as i64),
                item.map(postcard::to_allocvec).transpose()?
            ],
        )?;
    }
    Ok(())
}

/// Projects one entry from another chain, if it is a raw-data batch. A batch that breaks the
/// contract is skipped, as the graph replay skips entries that do not apply.
pub(crate) fn receive_evidence(tx: &Connection, e: &LedgerEntry) -> Result<()> {
    if e.head.kind != kind::EVIDENCE_BATCH {
        return Ok(());
    }
    match Change::of(e, &Known::default()) {
        Ok(change) => project_evidence(tx, &change),
        Err(_) => Ok(()),
    }
}

/// Fills the evidence table from the ledger, e.g. after an upgrade.
pub(crate) fn rebuild_evidence(tx: &Connection) -> Result<()> {
    tx.execute("DELETE FROM evidence", [])?;
    for chain in all_chains(tx)? {
        for e in &chain {
            receive_evidence(tx, e)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use symbiot_core::evidence::{Actor, role};
    use symbiot_core::{NodeKind, Signer};

    use super::*;

    fn store(seed: u8) -> Store {
        Store::open_in_memory(Signer::from_seed([seed; 32])).unwrap()
    }

    fn message(n: u32, at_ms: u64) -> Evidence {
        Evidence {
            id: format!("telegram:chat1/{n}"),
            kind: "schema:Message".into(),
            at_ms,
            actors: vec![Actor {
                id: "telegram:anna".into(),
                role: role::FROM.into(),
            }],
            text: format!("Nachricht {n}"),
            fields: vec![("chat".into(), "Familie".into())],
            files: vec![],
        }
    }

    #[test]
    fn record_query_and_import_again() {
        let mut s = store(1);
        let items: Vec<Evidence> = (1..=600).map(|n| message(n, 1_000 + n as u64)).collect();
        let r = s.record_evidence(items.clone()).unwrap();
        assert_eq!((r.added, r.known), (600, 0));
        assert!(
            r.entries >= 3,
            "600 items need at least three batches of 256"
        );
        assert_eq!(s.evidence_count().unwrap(), (600, 0));
        assert!(s.verify().unwrap().ok);
        assert!(
            s.nodes(None).unwrap().is_empty(),
            "raw data is not the graph"
        );

        let newest = s
            .evidence(&EvidenceQuery {
                limit: Some(2),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(newest[0].evidence, items[599]);
        assert_eq!(newest[1].evidence, items[598]);
        assert_eq!(newest[0].author, s.device());

        let window = s
            .evidence(&EvidenceQuery {
                from_ms: Some(1_010),
                to_ms: Some(1_020),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(window.len(), 10);
        let other = EvidenceQuery {
            kind: Some("schema:EmailMessage".into()),
            ..Default::default()
        };
        assert!(s.evidence(&other).unwrap().is_empty());

        // The same export again, plus one new message and one twice in the input.
        let mut again = items;
        again.push(message(601, 5_000));
        again.push(message(601, 5_000));
        let entries = s.entries().unwrap().len();
        let r = s.record_evidence(again).unwrap();
        assert_eq!((r.added, r.known, r.entries), (1, 601, 1));
        assert_eq!(s.entries().unwrap().len(), entries + 1);
    }

    #[test]
    fn broken_item_writes_nothing() {
        let mut s = store(1);
        let mut bad = message(2, 10);
        bad.id = "ohne-herkunft".into();
        let err = s.record_evidence(vec![message(1, 10), bad]).unwrap_err();
        assert!(matches!(err, Error::BadEvidence { ref id, .. } if id == "ohne-herkunft"));
        assert_eq!(s.evidence_count().unwrap(), (0, 0));
        assert!(s.entries().unwrap().is_empty());
    }

    #[test]
    fn removed_content_keeps_the_key() {
        let mut s = store(1);
        s.record_evidence(vec![message(1, 10), message(2, 11)])
            .unwrap();
        // E2: the content of the batch goes, the head stays.
        s.conn
            .execute("UPDATE ledger SET content = NULL", [])
            .unwrap();
        let tx = s.conn.transaction().unwrap();
        rebuild_evidence(&tx).unwrap();
        tx.commit().unwrap();
        assert_eq!(s.evidence_count().unwrap(), (0, 2));
        assert!(s.evidence(&EvidenceQuery::default()).unwrap().is_empty());
        assert!(s.verify().unwrap().ok);

        let r = s.record_evidence(vec![message(1, 10)]).unwrap();
        assert_eq!(
            (r.added, r.known),
            (0, 1),
            "deleted raw data does not come back"
        );
    }

    #[test]
    fn adapter_chain_arrives_through_sync() {
        let words = crate::new_words().unwrap();
        let mut adapter = store(2);
        adapter.certify_self(&words, "telegram-import").unwrap();
        adapter
            .record_evidence(vec![message(1, 10), message(2, 11)])
            .unwrap();
        let mut home = store(3);
        home.certify_self(&words, "heimserver").unwrap();
        home.create_node(NodeKind::Goal, "Familie öfter sehen", "")
            .unwrap();

        let n = home.receive(adapter.entries().unwrap()).unwrap();
        assert_eq!(n, adapter.entries().unwrap().len());
        let got = home.evidence(&EvidenceQuery::default()).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].author, adapter.device());
        assert_eq!(home.nodes(None).unwrap().len(), 1, "the graph is unchanged");

        // The home server loading the same message itself adds nothing.
        let r = home.record_evidence(vec![message(1, 10)]).unwrap();
        assert_eq!(r.added, 0);
    }

    #[test]
    fn upgrade_from_schema_4_fills_the_table() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("symbiot.db");
        {
            let mut s = Store::open(&db, Signer::from_seed([1; 32])).unwrap();
            s.record_evidence(vec![message(1, 10)]).unwrap();
            s.conn
                .execute_batch("DROP TABLE evidence; PRAGMA user_version = 4;")
                .unwrap();
        }
        let s = Store::open(&db, Signer::from_seed([1; 32])).unwrap();
        assert_eq!(s.evidence_count().unwrap(), (1, 0));
    }
}
