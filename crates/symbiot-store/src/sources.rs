//! #44: which raw data is used — per source, as decisions in the chain.
//!
//! `source_grant` holds the newest decision per source (the latest entry time wins, as in a
//! full replay). What a source does not use stays in the `evidence` table but is left out of
//! every query by [`USED`]. A revocation removes the content of the raw data loaded until then
//! (E2): from the ledger where a whole batch belongs to the source, from the projection always.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, OptionalExtension, params};
use symbiot_core::source::Grant;
use symbiot_core::{Change, Hash, Op};

use crate::{Result, Store};

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS source_grant (
        source     TEXT PRIMARY KEY,
        ts         INTEGER NOT NULL,
        entry      BLOB NOT NULL,
        active     INTEGER NOT NULL,
        exclude    TEXT NOT NULL,
        keep_days  INTEGER NOT NULL,
        revoked_ms INTEGER
    );";

/// The condition an evidence row `e` must meet to be used: its source is not paused or
/// revoked, its chat or repository is not left out, and it is recent enough.
pub(crate) const USED: &str = "NOT EXISTS (
    SELECT 1 FROM source_grant AS g WHERE g.source = e.origin AND (
        g.active = 0
        OR (g.keep_days > 0
            AND e.at_ms < CAST(strftime('%s', 'now') AS INTEGER) * 1000 - g.keep_days * 86400000)
        OR e.scope IN (SELECT value FROM json_each(g.exclude))))";

/// A source as the person manages it: what it holds, what is used, and the decision.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SourceInfo {
    /// The origin of its raw data, e.g. `whatsapp`.
    pub source: String,
    /// How it is used; the default when nobody decided.
    pub grant: Grant,
    /// When the decision was made; `None` if nobody decided yet.
    pub decided_ms: Option<u64>,
    /// When it was last revoked. Raw data loaded until then is gone.
    pub revoked_ms: Option<u64>,
    /// Items with content.
    pub items: usize,
    /// Of those, the ones used.
    pub used: usize,
    /// Time range of the items.
    pub first_ms: Option<u64>,
    pub last_ms: Option<u64>,
    /// When the newest batch was loaded.
    pub loaded_ms: Option<u64>,
    /// The adapters (devices) that loaded it, by name.
    pub adapters: Vec<String>,
    /// Chats or repositories with their number of items, the largest first.
    pub scopes: Vec<Scope>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Scope {
    pub name: String,
    pub items: usize,
    /// Left out by the decision.
    pub excluded: bool,
}

impl Store {
    /// Decides how a source is used. Takes effect at once, on every device after the sync.
    pub fn grant_source(&mut self, source: &str, grant: Grant) -> Result<()> {
        self.append(Op::GrantSource {
            source: source.to_string(),
            grant,
        })?;
        Ok(())
    }

    /// Revokes a source: not used any more, and its raw data is removed (E2). Returns how many
    /// items lost their content here.
    pub fn revoke_source(&mut self, source: &str) -> Result<usize> {
        let before = self.evidence_count()?.0;
        self.append(Op::RevokeSource {
            source: source.to_string(),
        })?;
        // Removed content must not survive in the write-ahead log or as files.
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        self.prune_files()?;
        Ok(before - self.evidence_count()?.0)
    }

    /// Every source with raw data or a decision, the largest first.
    pub fn sources(&self) -> Result<Vec<SourceInfo>> {
        let mut out: BTreeMap<String, SourceInfo> = BTreeMap::new();
        let mut stmt = self.conn.prepare(&format!(
            "SELECT e.origin, COUNT(*), SUM({USED}), MIN(e.at_ms), MAX(e.at_ms), MAX(e.loaded_ms)
             FROM evidence AS e WHERE e.item IS NOT NULL AND e.origin IS NOT NULL
             GROUP BY e.origin"
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? as usize,
                r.get::<_, i64>(2)? as usize,
                r.get::<_, Option<i64>>(3)?.map(|t| t as u64),
                r.get::<_, Option<i64>>(4)?.map(|t| t as u64),
                r.get::<_, Option<i64>>(5)?.map(|t| t as u64),
            ))
        })?;
        for row in rows {
            let (source, items, used, first_ms, last_ms, loaded_ms) = row?;
            let mut info = SourceInfo::new(source.clone());
            info.items = items;
            info.used = used;
            info.first_ms = first_ms;
            info.last_ms = last_ms;
            info.loaded_ms = loaded_ms;
            out.insert(source, info);
        }
        for (source, decision) in read_grants(&self.conn)? {
            let info = out
                .entry(source.clone())
                .or_insert_with(|| SourceInfo::new(source));
            info.grant = decision.grant;
            info.decided_ms = Some(decision.ts);
            info.revoked_ms = decision.revoked_ms;
        }

        let names: HashMap<Vec<u8>, String> = self
            .devices()?
            .into_iter()
            .map(|d| (d.key.0.to_vec(), d.name))
            .collect();
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT origin, author FROM evidence
             WHERE item IS NOT NULL AND origin IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (source, author) = row?;
            if let Some(info) = out.get_mut(&source) {
                let name = names.get(&author).cloned().unwrap_or_else(|| {
                    let hex: String = author.iter().take(4).map(|b| format!("{b:02x}")).collect();
                    format!("Adapter {hex}")
                });
                if !info.adapters.contains(&name) {
                    info.adapters.push(name);
                }
            }
        }

        let mut stmt = self.conn.prepare(
            "SELECT origin, scope, COUNT(*) FROM evidence
             WHERE item IS NOT NULL AND origin IS NOT NULL AND scope IS NOT NULL
             GROUP BY origin, scope ORDER BY COUNT(*) DESC, scope",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as usize,
            ))
        })?;
        for row in rows {
            let (source, name, items) = row?;
            if let Some(info) = out.get_mut(&source) {
                info.scopes.push(Scope {
                    excluded: info.grant.exclude.contains(&name),
                    name,
                    items,
                });
            }
        }
        // Left out, but no item (left): still listed, so it can be let in again.
        for info in out.values_mut() {
            for name in &info.grant.exclude {
                if !info.scopes.iter().any(|s| &s.name == name) {
                    info.scopes.push(Scope {
                        name: name.clone(),
                        items: 0,
                        excluded: true,
                    });
                }
            }
        }

        let mut list: Vec<SourceInfo> = out.into_values().collect();
        list.sort_by(|a, b| b.items.cmp(&a.items).then_with(|| a.source.cmp(&b.source)));
        Ok(list)
    }
}

impl SourceInfo {
    fn new(source: String) -> Self {
        SourceInfo {
            source,
            grant: Grant::default(),
            decided_ms: None,
            revoked_ms: None,
            items: 0,
            used: 0,
            first_ms: None,
            last_ms: None,
            loaded_ms: None,
            adapters: Vec::new(),
            scopes: Vec::new(),
        }
    }
}

struct Decided {
    ts: u64,
    grant: Grant,
    revoked_ms: Option<u64>,
}

fn read_grants(conn: &Connection) -> Result<Vec<(String, Decided)>> {
    let mut stmt = conn
        .prepare("SELECT source, ts, active, exclude, keep_days, revoked_ms FROM source_grant")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u64,
            r.get::<_, bool>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)? as u32,
            r.get::<_, Option<i64>>(5)?.map(|t| t as u64),
        ))
    })?;
    rows.map(|row| {
        let (source, ts, active, exclude, keep_days, revoked_ms) = row?;
        Ok((
            source,
            Decided {
                ts,
                grant: Grant {
                    active,
                    exclude: serde_json::from_str(&exclude).unwrap_or_default(),
                    keep_days,
                },
                revoked_ms,
            },
        ))
    })
    .collect()
}

/// Writes a `Change::Source`. The newer decision wins (time, then entry hash), so the order
/// in which entries arrive does not matter; the last revocation is kept apart.
pub(crate) fn project_source(tx: &Connection, change: &Change) -> Result<()> {
    let Change::Source {
        entry,
        ts_ms,
        source,
        grant,
    } = change
    else {
        return Ok(());
    };
    let revoked = grant.is_none().then_some(*ts_ms as i64);
    // Revoked: not used until a new grant, with nothing left out.
    let grant = grant.clone().unwrap_or(Grant {
        active: false,
        exclude: Vec::new(),
        keep_days: 0,
    });
    let old: Option<(i64, Vec<u8>)> = tx
        .query_row(
            "SELECT ts, entry FROM source_grant WHERE source = ?1",
            [source],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let newer = old
        .as_ref()
        .is_none_or(|(ts, h)| (*ts_ms as i64, entry.0.as_slice()) > (*ts, h.as_slice()));
    if newer {
        tx.execute(
            "INSERT INTO source_grant (source, ts, entry, active, exclude, keep_days, revoked_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (source) DO UPDATE SET ts = ?2, entry = ?3, active = ?4, exclude = ?5,
                 keep_days = ?6, revoked_ms = MAX(COALESCE(revoked_ms, ?7), COALESCE(?7, revoked_ms))",
            params![
                source,
                *ts_ms as i64,
                entry.0.as_slice(),
                grant.active,
                serde_json::to_string(&grant.exclude)?,
                grant.keep_days as i64,
                revoked
            ],
        )?;
    } else if let Some(r) = revoked {
        tx.execute(
            "UPDATE source_grant SET revoked_ms = MAX(COALESCE(revoked_ms, ?2), ?2)
             WHERE source = ?1",
            params![source, r],
        )?;
    }
    if revoked.is_some() {
        prune_revoked(tx)?;
    }
    Ok(())
}

/// Whether any source was ever revoked — only then is there raw data to remove.
pub(crate) fn any_revoked(conn: &Connection) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM source_grant WHERE revoked_ms IS NOT NULL LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// E2 for revoked sources: raw data loaded up to the revocation loses its content — in the
/// projection always, in the ledger where the whole batch is covered. Returns the items.
pub(crate) fn prune_revoked(tx: &Connection) -> Result<usize> {
    const COVERED: &str = "EXISTS (SELECT 1 FROM source_grant AS g WHERE g.source = e.origin
        AND g.revoked_ms IS NOT NULL AND e.loaded_ms <= g.revoked_ms)";
    let mut stmt = tx.prepare(&format!(
        "SELECT e.rowid, e.key, e.entry FROM evidence AS e
         WHERE e.item IS NOT NULL AND {COVERED}"
    ))?;
    let rows: Vec<(i64, Vec<u8>, Vec<u8>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut entries: Vec<Vec<u8>> = Vec::new();
    for (rowid, key, entry) in &rows {
        tx.execute("DELETE FROM evidence_fts WHERE rowid = ?1", [rowid])?;
        tx.execute("DELETE FROM evidence_actor WHERE key = ?1", [key])?;
        tx.execute("DELETE FROM evidence_file WHERE key = ?1", [key])?;
        if !entries.contains(entry) {
            entries.push(entry.clone());
        }
    }
    tx.execute(
        &format!("UPDATE evidence AS e SET item = NULL WHERE {COVERED}"),
        [],
    )?;
    for entry in entries {
        // Every item of the batch is gone: its content can go from the ledger, too.
        let left: i64 = tx.query_row(
            "SELECT COUNT(*) FROM evidence WHERE entry = ?1 AND item IS NOT NULL",
            [&entry],
            |r| r.get(0),
        )?;
        if left == 0 {
            tx.execute("UPDATE ledger SET content = NULL WHERE hash = ?1", [&entry])?;
        }
    }
    Ok(rows.len())
}

/// The decisions of a replay, written again (after `DELETE FROM source_grant`).
pub(crate) fn project_all(
    tx: &Connection,
    all: &[(Hash, u64, String, Option<Grant>)],
) -> Result<()> {
    for (entry, ts_ms, source, grant) in all {
        project_source(
            tx,
            &Change::Source {
                entry: *entry,
                ts_ms: *ts_ms,
                source: source.clone(),
                grant: grant.clone(),
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use symbiot_core::evidence::{Actor, role};
    use symbiot_core::{Evidence, Signer};

    use super::*;
    use crate::EvidenceQuery;

    fn store(seed: u8) -> Store {
        Store::open_in_memory(Signer::from_seed([seed; 32])).unwrap()
    }

    fn message(n: u32, chat: &str, at_ms: u64) -> Evidence {
        Evidence {
            id: format!("telegram:{chat}/{n}"),
            kind: "schema:Message".into(),
            at_ms,
            actors: vec![Actor {
                id: "telegram:anna".into(),
                role: role::FROM.into(),
                name: "Anna".into(),
            }],
            text: format!("Nachricht {n}"),
            fields: vec![("chat".into(), chat.into())],
            files: vec![],
        }
    }

    fn commit(n: u32) -> Evidence {
        Evidence {
            id: format!("git:{n:040x}"),
            kind: "schema:SoftwareSourceCode".into(),
            at_ms: 5,
            actors: vec![],
            text: format!("Commit {n}"),
            fields: vec![("repo".into(), "symbiot".into())],
            files: vec![],
        }
    }

    fn shown(s: &Store) -> usize {
        s.evidence(&EvidenceQuery::default()).unwrap().len()
    }

    #[test]
    fn pause_leave_out_and_let_in_again() {
        let mut s = store(1);
        s.record_evidence(vec![
            message(1, "Familie", 1),
            message(2, "Familie", 2),
            message(3, "Arbeit", 3),
            commit(1),
        ])
        .unwrap();
        let all = s.sources().unwrap();
        assert_eq!(all[0].source, "telegram");
        assert_eq!((all[0].items, all[0].used), (3, 3));
        assert_eq!(all[0].scopes[0].name, "Familie");
        assert_eq!(all[0].decided_ms, None);
        assert_eq!(all[1].scopes[0].name, "symbiot");

        s.grant_source(
            "telegram",
            Grant {
                exclude: vec!["Arbeit".into()],
                ..Grant::default()
            },
        )
        .unwrap();
        assert_eq!(shown(&s), 3);
        let t = &s.sources().unwrap()[0];
        assert_eq!((t.items, t.used), (3, 2));
        assert!(t.scopes.iter().any(|x| x.name == "Arbeit" && x.excluded));
        assert_eq!(
            s.persons().unwrap()[0].count,
            2,
            "persons see only what is used"
        );

        s.grant_source(
            "telegram",
            Grant {
                active: false,
                ..Grant::default()
            },
        )
        .unwrap();
        assert_eq!(shown(&s), 1, "only the commit is left");
        assert_eq!(s.evidence_origins().unwrap().len(), 1);
        assert!(s.persons().unwrap().is_empty());

        s.grant_source("telegram", Grant::default()).unwrap();
        assert_eq!(shown(&s), 4, "pausing keeps the data");
        assert!(s.verify().unwrap().ok);
    }

    #[test]
    fn only_recent_days() {
        let mut s = store(1);
        let now = crate::now_ms();
        let day = 86_400_000;
        s.record_evidence(vec![
            message(1, "Familie", now - 40 * day),
            message(2, "Familie", now - day),
        ])
        .unwrap();
        s.grant_source(
            "telegram",
            Grant {
                keep_days: 30,
                ..Grant::default()
            },
        )
        .unwrap();
        assert_eq!(shown(&s), 1);
    }

    #[test]
    fn revoke_removes_raw_data_everywhere_and_new_data_can_come() {
        let words = crate::new_words().unwrap();
        let mut adapter = store(2);
        adapter.certify_self(&words, "telegram-import").unwrap();
        adapter
            .record_evidence(vec![message(1, "Familie", 1), message(2, "Familie", 2)])
            .unwrap();
        let mut home = store(3);
        home.certify_self(&words, "Heimserver").unwrap();
        home.record_evidence(vec![commit(1)]).unwrap();
        home.receive(adapter.entries().unwrap()).unwrap();
        assert_eq!(shown(&home), 3);
        assert_eq!(home.sources().unwrap()[0].adapters, vec!["telegram-import"]);

        assert_eq!(home.revoke_source("telegram").unwrap(), 2);
        assert_eq!(shown(&home), 1);
        let t = home
            .sources()
            .unwrap()
            .into_iter()
            .find(|s| s.source == "telegram")
            .unwrap();
        assert!(t.revoked_ms.is_some());
        assert!(!t.grant.active);
        assert_eq!(t.items, 0);
        // The adapter's batch lost its content in the ledger, and the chain still verifies.
        let theirs = home.chain_since(&adapter.device(), None).unwrap();
        assert!(theirs.iter().any(|e| e.is_pruned()));
        assert!(home.verify_all().unwrap().iter().all(|r| r.ok));

        // The adapter learns of it through the sync and removes its copy, too.
        let mut adapter_side = adapter;
        adapter_side.receive(home.entries().unwrap()).unwrap();
        assert_eq!(
            shown(&adapter_side),
            1,
            "only the commit from the home server"
        );
        assert_eq!(
            adapter_side
                .record_evidence(vec![message(1, "Familie", 1)])
                .unwrap()
                .added,
            0,
            "revoked raw data is not loaded again"
        );

        // A new grant lets new data in; what was revoked stays gone.
        home.grant_source("telegram", Grant::default()).unwrap();
        adapter_side.receive(home.entries().unwrap()).unwrap();
        adapter_side
            .record_evidence(vec![message(3, "Familie", 3)])
            .unwrap();
        home.receive(adapter_side.entries().unwrap()).unwrap();
        assert_eq!(shown(&home), 2);

        // A full rebuild gives the same.
        let tx = home.conn.unchecked_transaction().unwrap();
        crate::rebuild_projection(&tx, &home.device()).unwrap();
        crate::evidence::rebuild_evidence(&tx).unwrap();
        prune_revoked(&tx).unwrap();
        tx.commit().unwrap();
        assert_eq!(shown(&home), 2);
    }

    #[test]
    fn bad_names_are_refused() {
        let mut s = store(1);
        assert!(s.grant_source("Tele gram", Grant::default()).is_err());
        assert!(
            s.grant_source(
                "telegram",
                Grant {
                    exclude: vec![String::new()],
                    ..Grant::default()
                }
            )
            .is_err()
        );
        assert!(s.revoke_source("").is_err());
    }
}
