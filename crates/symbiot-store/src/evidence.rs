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

use crate::sources::USED;
use crate::{Error, Known, Result, Store, append_in};

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
    /// Origin, the part of the id before the first `:`, e.g. `telegram`, `mail`, `git`.
    pub origin: Option<String>,
    /// Words to search for in text, names and fields. Every word must occur; a word also
    /// matches as the start of a longer one (`wand` finds „Wandern“).
    pub text: Option<String>,
    pub limit: Option<usize>,
    /// Skip this many results (for „load more“).
    pub offset: Option<usize>,
    /// #31: only items that name one of these identifiers (e.g. all of one person).
    pub actors: Option<Vec<String>>,
}

/// How many items one origin has.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Origin {
    pub origin: String,
    pub count: usize,
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

/// `evidence_fts` is contentless: the words are indexed, the text itself lives only in `item`.
pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS evidence (
        key    BLOB PRIMARY KEY,
        entry  BLOB NOT NULL,
        part   INTEGER NOT NULL,
        author BLOB NOT NULL,
        origin TEXT,
        kind   TEXT,
        at_ms  INTEGER,
        item   BLOB,
        -- #44: the chat or repository, and when the batch was loaded.
        scope     TEXT,
        loaded_ms INTEGER
    );
    CREATE INDEX IF NOT EXISTS evidence_time ON evidence (at_ms);
    CREATE INDEX IF NOT EXISTS evidence_kind ON evidence (kind, at_ms);
    CREATE INDEX IF NOT EXISTS evidence_origin ON evidence (origin, at_ms);
    CREATE TABLE IF NOT EXISTS evidence_actor (
        key   BLOB NOT NULL,
        actor TEXT NOT NULL,
        role  TEXT NOT NULL,
        name  TEXT NOT NULL,
        PRIMARY KEY (key, actor, role)
    );
    CREATE INDEX IF NOT EXISTS evidence_actor_id ON evidence_actor (actor);
    CREATE TABLE IF NOT EXISTS evidence_file (
        hash BLOB NOT NULL,
        key  BLOB NOT NULL,
        name TEXT NOT NULL,
        mime TEXT NOT NULL,
        len  INTEGER NOT NULL,
        PRIMARY KEY (hash, key)
    );
    CREATE VIRTUAL TABLE IF NOT EXISTS evidence_fts USING fts5 (
        text, meta,
        content = '', contentless_delete = 1,
        tokenize = 'unicode61 remove_diacritics 2'
    );";

/// Drops the raw-data tables of an older schema; they are rebuilt from the ledger.
pub(crate) const DROP: &str = "DROP TABLE IF EXISTS evidence; DROP TABLE IF EXISTS evidence_fts; DROP TABLE IF EXISTS evidence_file; DROP TABLE IF EXISTS evidence_actor;";

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
        let mut stmt = self.conn.prepare(&format!(
            "SELECT key, entry, author, item FROM evidence AS e
             WHERE item IS NOT NULL AND {USED}
               AND (?1 IS NULL OR kind = ?1)
               AND (?2 IS NULL OR at_ms >= ?2)
               AND (?3 IS NULL OR at_ms < ?3)
               AND (?4 IS NULL OR origin = ?4)
               AND (?5 IS NULL OR rowid IN
                    (SELECT rowid FROM evidence_fts WHERE evidence_fts MATCH ?5))
               AND (?8 IS NULL OR key IN
                    (SELECT key FROM evidence_actor
                     WHERE actor IN (SELECT value FROM json_each(?8))))
             ORDER BY at_ms DESC, key
             LIMIT ?6 OFFSET ?7"
        ))?;
        let search = q.text.as_deref().and_then(match_query);
        if q.text.is_some() && search.is_none() {
            return Ok(Vec::new());
        }
        let limit = q.limit.map_or(-1, |l| l as i64);
        let rows = stmt.query_map(
            params![
                q.kind,
                q.from_ms.map(|t| t as i64),
                q.to_ms.map(|t| t as i64),
                q.origin,
                search,
                limit,
                q.offset.unwrap_or(0) as i64,
                q.actors.as_deref().map(json_list)
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

    /// Origins with content that is used (#44), the largest first.
    pub fn evidence_origins(&self) -> Result<Vec<Origin>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT origin, COUNT(*) FROM evidence AS e
             WHERE item IS NOT NULL AND origin IS NOT NULL AND {USED}
             GROUP BY origin ORDER BY COUNT(*) DESC, origin"
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok(Origin {
                origin: r.get(0)?,
                count: r.get::<_, i64>(1)? as usize,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
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

/// Identifiers (normalised by rule) as a JSON array for `json_each`.
fn json_list(ids: &[String]) -> String {
    let mut out = String::from("[");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        for c in symbiot_core::identity::normalize(id).chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

/// The user's words as an FTS5 query: each word quoted (no operators can slip in) and
/// matched as a prefix. `None` if no word is left.
fn match_query(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// What the search finds besides the text: names and ids of the people, and field values.
fn meta_of(e: &Evidence) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for a in &e.actors {
        parts.push(&a.name);
        parts.push(&a.id);
    }
    for (_, v) in &e.fields {
        parts.push(v);
    }
    parts.join(" ")
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
        ts_ms,
        keys,
        items,
    } = change
    else {
        return Ok(());
    };
    for (part, key) in keys.iter().enumerate() {
        let item = items.get(part);
        let added = tx.execute(
            "INSERT OR IGNORE INTO evidence
                 (key, entry, part, author, origin, kind, at_ms, item, scope, loaded_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                key.0.as_slice(),
                entry.0.as_slice(),
                part as i64,
                author.0.as_slice(),
                item.and_then(|i| i.id.split_once(':')).map(|(o, _)| o),
                item.map(|i| i.kind.as_str()),
                item.map(|i| i.at_ms as i64),
                item.map(postcard::to_allocvec).transpose()?,
                item.and_then(symbiot_core::source::scope_of),
                *ts_ms as i64
            ],
        )?;
        if let (1, Some(item)) = (added, item) {
            tx.execute(
                "INSERT INTO evidence_fts (rowid, text, meta) VALUES (?1, ?2, ?3)",
                params![tx.last_insert_rowid(), item.text, meta_of(item)],
            )?;
            // #31: identifiers by rule, so the same person in another spelling is one.
            for a in &item.actors {
                tx.execute(
                    "INSERT OR IGNORE INTO evidence_actor (key, actor, role, name)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        key.0.as_slice(),
                        symbiot_core::identity::normalize(&a.id),
                        a.role,
                        a.name
                    ],
                )?;
            }
            for f in &item.files {
                tx.execute(
                    "INSERT OR IGNORE INTO evidence_file (hash, key, name, mime, len)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        f.hash.0.as_slice(),
                        key.0.as_slice(),
                        f.name,
                        f.mime,
                        f.len as i64
                    ],
                )?;
            }
        }
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
    tx.execute_batch("DELETE FROM evidence; DELETE FROM evidence_fts; DELETE FROM evidence_file; DELETE FROM evidence_actor;")?;
    // One batch at a time: the raw data of a life does not fit into memory at once.
    let mut stmt = tx.prepare(
        "SELECT head, hash, sig, content FROM ledger WHERE kind = ?1 ORDER BY author, seq",
    )?;
    let mut rows = stmt.query([kind::EVIDENCE_BATCH as i64])?;
    while let Some(row) = rows.next()? {
        receive_evidence(tx, &crate::decode_entry(crate::raw_entry(row)?)?)?;
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
                name: "Anna".into(),
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

    fn mail(n: u32, at_ms: u64, subject: &str, text: &str) -> Evidence {
        Evidence {
            id: format!("mail:{n}@example.org"),
            kind: "schema:EmailMessage".into(),
            at_ms,
            actors: vec![Actor {
                id: "mailto:ben@example.org".into(),
                role: role::FROM.into(),
                name: "Ben Müller".into(),
            }],
            text: text.into(),
            fields: vec![("subject".into(), subject.into())],
            files: vec![],
        }
    }

    #[test]
    fn search_origin_and_paging() {
        let mut s = store(1);
        let mut items: Vec<Evidence> = (1..=5).map(|n| message(n, 100 + n as u64)).collect();
        items.push(mail(1, 50, "Wandern am Samstag", "Hast du Lust?"));
        items.push(mail(2, 60, "Rechnung", "Anbei die Rechnung für März."));
        s.record_evidence(items).unwrap();

        let find = |s: &Store, text: &str| {
            s.evidence(&EvidenceQuery {
                text: Some(text.into()),
                ..Default::default()
            })
            .unwrap()
            .into_iter()
            .map(|e| e.evidence.id)
            .collect::<Vec<_>>()
        };
        assert_eq!(
            find(&s, "wand"),
            ["mail:1@example.org"],
            "prefix of a field"
        );
        assert_eq!(
            find(&s, "muller rechnung"),
            ["mail:2@example.org"],
            "all words, no accents"
        );
        assert_eq!(find(&s, "Nachricht 3"), ["telegram:chat1/3"]);
        assert!(find(&s, "gibtsnicht").is_empty());
        assert!(find(&s, "   ").is_empty());
        assert_eq!(find(&s, "\"AND OR*").len(), 0, "operators are only words");

        let origins = s.evidence_origins().unwrap();
        assert_eq!(
            origins,
            [
                Origin {
                    origin: "telegram".into(),
                    count: 5
                },
                Origin {
                    origin: "mail".into(),
                    count: 2
                }
            ]
        );
        let page = |offset| {
            s.evidence(&EvidenceQuery {
                origin: Some("telegram".into()),
                limit: Some(2),
                offset: Some(offset),
                ..Default::default()
            })
            .unwrap()
            .into_iter()
            .map(|e| e.evidence.at_ms)
            .collect::<Vec<_>>()
        };
        assert_eq!(page(0), [105, 104]);
        assert_eq!(page(2), [103, 102]);
        assert_eq!(page(4), [101]);

        // A rebuild gives the same index.
        let tx = s.conn.transaction().unwrap();
        rebuild_evidence(&tx).unwrap();
        tx.commit().unwrap();
        assert_eq!(find(&s, "wand"), ["mail:1@example.org"]);
    }

    #[test]
    fn upgrade_from_schema_5_adds_origin_and_index() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("symbiot.db");
        {
            let mut s = Store::open(&db, Signer::from_seed([1; 32])).unwrap();
            s.record_evidence(vec![mail(1, 10, "Wandern", "")]).unwrap();
            // The table as schema 5 had it.
            s.conn
                .execute_batch(
                    "DROP TABLE evidence; DROP TABLE evidence_fts;
                     CREATE TABLE evidence (key BLOB PRIMARY KEY, entry BLOB NOT NULL,
                       part INTEGER NOT NULL, author BLOB NOT NULL, kind TEXT, at_ms INTEGER,
                       item BLOB);
                     PRAGMA user_version = 5;",
                )
                .unwrap();
        }
        let s = Store::open(&db, Signer::from_seed([1; 32])).unwrap();
        let q = EvidenceQuery {
            text: Some("wandern".into()),
            origin: Some("mail".into()),
            ..Default::default()
        };
        assert_eq!(s.evidence(&q).unwrap().len(), 1);
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
