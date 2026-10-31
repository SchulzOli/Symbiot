//! Symbiot store — SQLite on every platform (desktop, mobile, server).
//!
//! The ledger table is the truth; the nodes table is a projection of it,
//! updated in the same transaction as every append. Each entry is stored as its
//! signed head plus content in a separate column, so content can be removed (E2).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use symbiot_core::{
    ApplyError, Change, Edge, EdgeId, EdgeKind, Hash, LedgerEntry, Lookup, Node, NodeId, NodeKind,
    Op, Policy, Proposal, PubKey, Sig, Signer,
};

/// `PRAGMA user_version` of the current schema. 0 with an `entry` column is the v1 format.
const SCHEMA_VERSION: i64 = 2;

pub use symbiot_core::VerifyReport;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Datenbank: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Kodierung: {0}")]
    Encoding(#[from] postcard::Error),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("Knoten {0} existiert nicht")]
    UnknownNode(NodeId),
    #[error("Verbindung {0} existiert nicht")]
    UnknownEdge(EdgeId),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Store {
    conn: Connection,
    signer: Signer,
    set_aside: Option<PathBuf>,
}

/// Reads the device's signing seed, creating it on first start. Back this file up: it is the device identity.
pub fn load_or_create_seed(path: &Path) -> io::Result<[u8; 32]> {
    if let Ok(bytes) = std::fs::read(path) {
        return bytes.try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "device key must be 32 bytes")
        });
    }
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(io::Error::other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, seed)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(seed)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Store {
    /// Opens (or creates) `<dir>/symbiot.db` with the device key `<dir>/device.key`.
    pub fn open_dir(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|_| Error::Invalid("Datenordner nicht anlegbar"))?;
        let seed = load_or_create_seed(&dir.join("device.key"))
            .map_err(|_| Error::Invalid("Geräteschlüssel nicht lesbar"))?;
        Self::open(&dir.join("symbiot.db"), Signer::from_seed(seed))
    }

    /// Opens `db`. A database in the old v1 format holds only test data (no migration, decided
    /// 2026-10-31): it is renamed to `<name>.v1.db` and a fresh v2 chain starts.
    pub fn open(db: &Path, signer: Signer) -> Result<Self> {
        let mut conn = Connection::open(db)?;
        let mut set_aside = None;
        if is_v1(&conn)? {
            drop(conn);
            let old = db.with_extension("v1.db");
            std::fs::rename(db, &old)
                .map_err(|_| Error::Invalid("alte Datenbank (v1) nicht beiseitelegbar"))?;
            for suffix in ["-wal", "-shm"] {
                let mut side = db.as_os_str().to_owned();
                side.push(suffix);
                let _ = std::fs::remove_file(side);
            }
            set_aside = Some(old);
            conn = Connection::open(db)?;
        }
        let mut store = Self::init(conn, signer)?;
        store.set_aside = set_aside;
        Ok(store)
    }

    pub fn open_in_memory(signer: Signer) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, signer)
    }

    /// Where an old v1 database was moved when this store was opened, if it was.
    pub fn set_aside_v1(&self) -> Option<&Path> {
        self.set_aside.as_deref()
    }

    fn init(conn: Connection, signer: Signer) -> Result<Self> {
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::Invalid("Datenbank stammt von einer neueren Version"));
        }
        // secure_delete: removed content is overwritten, not left in free pages (E2).
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA secure_delete = ON;
             CREATE TABLE IF NOT EXISTS ledger (
                 author  BLOB NOT NULL,
                 seq     INTEGER NOT NULL,
                 hash    BLOB NOT NULL UNIQUE,
                 sig     BLOB NOT NULL,
                 head    BLOB NOT NULL,
                 content BLOB,
                 PRIMARY KEY (author, seq)
             );
             CREATE TABLE IF NOT EXISTS nodes (
                 id         BLOB PRIMARY KEY,
                 kind       TEXT NOT NULL,
                 updated_ms INTEGER NOT NULL,
                 node       BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS nodes_kind ON nodes (kind, updated_ms);
             CREATE TABLE IF NOT EXISTS edges (
                 id         BLOB PRIMARY KEY,
                 from_id    BLOB NOT NULL,
                 to_id      BLOB NOT NULL,
                 kind       TEXT NOT NULL,
                 created_ms INTEGER NOT NULL,
                 edge       BLOB NOT NULL,
                 UNIQUE (from_id, to_id, kind)
             );
             CREATE TABLE IF NOT EXISTS sensitive (
                 node_id BLOB PRIMARY KEY
             );
             CREATE TABLE IF NOT EXISTS policies (
                 node_id    BLOB PRIMARY KEY,
                 deletable  INTEGER NOT NULL,
                 exportable INTEGER NOT NULL
             );",
        )?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Store {
            conn,
            signer,
            set_aside: None,
        })
    }

    /// This device's identity.
    pub fn device(&self) -> PubKey {
        self.signer.public()
    }

    pub fn create_node(&mut self, kind: NodeKind, title: &str, body: &str) -> Result<Node> {
        self.create_node_with(kind, title, body, Policy::default())
    }

    /// Creates a node with its policy (E3). A policy other than the default is written in the
    /// same transaction, so the node never exists without it — not even for a moment.
    pub fn create_node_with(
        &mut self,
        kind: NodeKind,
        title: &str,
        body: &str,
        policy: Policy,
    ) -> Result<Node> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("Titel darf nicht leer sein"));
        }
        let tx = self.conn.transaction()?;
        let create = Op::CreateNode {
            kind,
            title: title.into(),
            body: body.into(),
        };
        let Change::Node(node) = append_in(&tx, &self.signer, create)? else {
            unreachable!("CreateNode yields a node")
        };
        if policy != Policy::default() {
            let set = Op::SetPolicy {
                node: node.id,
                deletable: policy.deletable,
                exportable: policy.exportable,
            };
            append_in(&tx, &self.signer, set)?;
        }
        tx.commit()?;
        Ok(node)
    }

    /// Editing appends a new entry; earlier versions stay in the ledger.
    pub fn update_node(&mut self, id: NodeId, title: &str, body: &str) -> Result<Node> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("Titel darf nicht leer sein"));
        }
        match self.append(Op::UpdateNode {
            node: id,
            title: title.into(),
            body: body.into(),
        })? {
            Change::Node(n) => Ok(n),
            _ => unreachable!("UpdateNode yields a node"),
        }
    }

    /// Connects two nodes, e.g. „Beleg stützt Ziel“.
    pub fn link(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) -> Result<Edge> {
        match self.append(Op::Link { from, to, kind })? {
            Change::Edge(e) => Ok(e),
            _ => unreachable!("Link yields an edge"),
        }
    }

    /// Removes a connection.
    pub fn unlink(&mut self, edge: EdgeId) -> Result<()> {
        self.append(Op::Unlink { edge }).map(|_| ())
    }

    /// E2: writes a deletion marker. If the node is deletable, the content of every entry
    /// that shaped it is removed here and now. Returns how many entries lost their content.
    pub fn delete_node(&mut self, id: NodeId) -> Result<usize> {
        match self.append(Op::DeleteNode { node: id })? {
            Change::Deleted { prune, .. } => Ok(prune.len()),
            _ => unreachable!("DeleteNode yields a deletion"),
        }
    }

    /// E3: sets whether a node is deletable and exportable. Every change is a new entry.
    pub fn set_policy(&mut self, id: NodeId, policy: Policy) -> Result<()> {
        self.append(Op::SetPolicy {
            node: id,
            deletable: policy.deletable,
            exportable: policy.exportable,
        })
        .map(|_| ())
    }

    pub fn policy(&self, id: NodeId) -> Result<Policy> {
        self.node(id)?.ok_or(Error::UnknownNode(id))?;
        read_policy(&self.conn, &id)
    }

    /// Nodes that must stay on the own devices (E3) — also away from a hosted AI.
    pub fn not_exportable_nodes(&self) -> Result<Vec<NodeId>> {
        node_ids(
            &self.conn,
            "SELECT node_id FROM policies WHERE exportable = 0",
        )
    }

    /// F4: the human accepted an AI proposal. Signs the decision with the full proposal and
    /// creates the new node plus its edge „derives from“ the goal.
    pub fn accept_proposal(&mut self, proposal: Proposal) -> Result<(Node, Edge)> {
        match self.append(Op::AcceptProposal { proposal })? {
            Change::Accepted { node, edge } => Ok((node, edge)),
            _ => unreachable!("AcceptProposal yields a node and an edge"),
        }
    }

    /// F4: the human rejected an AI proposal. Only the decision is recorded; the graph stays.
    pub fn reject_proposal(&mut self, proposal: Proposal) -> Result<()> {
        self.append(Op::RejectProposal { proposal }).map(|_| ())
    }

    /// Marks a node as sensitive (E4): it never leaves the home network.
    pub fn set_sensitive(&mut self, id: NodeId, sensitive: bool) -> Result<()> {
        self.append(Op::MarkSensitive {
            node: id,
            sensitive,
        })
        .map(|_| ())
    }

    /// All nodes currently marked as sensitive.
    pub fn sensitive_nodes(&self) -> Result<Vec<NodeId>> {
        node_ids(&self.conn, "SELECT node_id FROM sensitive")
    }

    /// What must never reach an external model: sensitive (E4) or not exportable (E3).
    pub fn home_only_nodes(&self) -> Result<Vec<NodeId>> {
        node_ids(
            &self.conn,
            "SELECT node_id FROM sensitive
             UNION SELECT node_id FROM policies WHERE exportable = 0",
        )
    }

    fn append(&mut self, op: Op) -> Result<Change> {
        let tx = self.conn.transaction()?;
        let change = append_in(&tx, &self.signer, op)?;
        tx.commit()?;
        if matches!(&change, Change::Deleted { prune, .. } if !prune.is_empty()) {
            // Removed content must not survive in the write-ahead log either.
            self.conn
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        }
        Ok(change)
    }
}

/// What the core rules need to know, loaded from SQLite before an append.
#[derive(Default)]
struct Known {
    nodes: HashMap<NodeId, Node>,
    edge: Option<Edge>,
    duplicate: bool,
    policy: Policy,
}

impl Lookup for Known {
    fn node(&self, id: &NodeId) -> Option<Node> {
        self.nodes.get(id).cloned()
    }

    fn edge(&self, id: &EdgeId) -> Option<Edge> {
        self.edge.clone().filter(|e| e.id == *id)
    }

    fn has_edge(&self, _: &NodeId, _: &NodeId, _: EdgeKind) -> bool {
        self.duplicate
    }

    fn policy(&self, _: &NodeId) -> Policy {
        self.policy
    }
}

/// Signs `op` onto this device's chain and updates the projection, inside `tx`.
fn append_in(tx: &Connection, signer: &Signer, op: Op) -> Result<Change> {
    let author = signer.public();
    let tip: Option<LedgerEntry> = tx
        .query_row(
            "SELECT head, hash, sig, content FROM ledger
             WHERE author = ?1 ORDER BY seq DESC LIMIT 1",
            [author.0.as_slice()],
            raw_entry,
        )
        .optional()?
        .map(decode_entry)
        .transpose()?;

    // Load what the core rules need, then let the core decide (same rules on every platform).
    let mut known = Known::default();
    let referenced: Vec<NodeId> = match &op {
        Op::CreateNode { .. } | Op::Unlink { .. } => vec![],
        Op::UpdateNode { node, .. }
        | Op::MarkSensitive { node, .. }
        | Op::DeleteNode { node }
        | Op::SetPolicy { node, .. } => vec![*node],
        Op::Link { from, to, .. } => vec![*from, *to],
        Op::AcceptProposal { proposal } | Op::RejectProposal { proposal } => {
            std::iter::once(proposal.goal)
                .chain(proposal.sources.iter().copied())
                .collect()
        }
    };
    for id in referenced {
        if let Some(n) = read_node(tx, &id)? {
            known.nodes.insert(id, n);
        }
    }
    match &op {
        Op::Link { from, to, kind } => known.duplicate = edge_exists(tx, from, to, *kind)?,
        Op::Unlink { edge } => known.edge = read_edge(tx, edge)?,
        Op::DeleteNode { node } => known.policy = read_policy(tx, node)?,
        _ => {}
    }

    let entry = signer.append(tip.as_ref(), now_ms(), op);
    let change = Change::of(&entry, &known).map_err(|e| match e {
        ApplyError::UnknownNode(id) => Error::UnknownNode(id),
        ApplyError::UnknownEdge(id) => Error::UnknownEdge(id),
        ApplyError::SelfLink => {
            Error::Invalid("Ein Knoten kann nicht mit sich selbst verbunden werden")
        }
        ApplyError::DuplicateEdge => Error::Invalid("Diese Verbindung gibt es schon"),
        ApplyError::NoSources => Error::Invalid("Ein Vorschlag ohne Quellen wird verworfen"),
        ApplyError::EmptyTitle => Error::Invalid("Titel darf nicht leer sein"),
        ApplyError::Undecodable(_) => Error::Invalid("Eintrag nicht lesbar"),
    })?;

    tx.execute(
        "INSERT INTO ledger (author, seq, hash, sig, head, content)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            author.0.as_slice(),
            entry.head.seq as i64,
            entry.hash.0.as_slice(),
            entry.sig.0.as_slice(),
            postcard::to_allocvec(&entry.head)?,
            entry
                .content
                .as_ref()
                .map(postcard::to_allocvec)
                .transpose()?
        ],
    )?;
    project(tx, &change)?;
    Ok(change)
}

/// Applies a change to the projection tables — the SQLite twin of `State::apply`.
fn project(tx: &Connection, change: &Change) -> Result<()> {
    match change {
        Change::Node(node) => {
            write_node(tx, node)?;
        }
        Change::Edge(edge) => {
            write_edge(tx, edge)?;
        }
        Change::Accepted { node, edge } => {
            write_node(tx, node)?;
            write_edge(tx, edge)?;
        }
        Change::Rejected => {}
        Change::Sensitivity {
            node,
            sensitive: true,
        } => {
            tx.execute(
                "INSERT OR IGNORE INTO sensitive (node_id) VALUES (?1)",
                [node.0.as_slice()],
            )?;
        }
        Change::Sensitivity {
            node,
            sensitive: false,
        } => {
            tx.execute(
                "DELETE FROM sensitive WHERE node_id = ?1",
                [node.0.as_slice()],
            )?;
        }
        Change::Deleted { node, prune } => {
            let id = node.0.as_slice();
            tx.execute("DELETE FROM nodes WHERE id = ?1", [id])?;
            tx.execute("DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1", [id])?;
            tx.execute("DELETE FROM sensitive WHERE node_id = ?1", [id])?;
            tx.execute("DELETE FROM policies WHERE node_id = ?1", [id])?;
            // E2: the head stays (hash, signature), title and body go — with secure_delete.
            for h in prune {
                tx.execute(
                    "UPDATE ledger SET content = NULL WHERE hash = ?1",
                    [h.0.as_slice()],
                )?;
            }
        }
        Change::Unlinked(edge) => {
            tx.execute("DELETE FROM edges WHERE id = ?1", [edge.0.as_slice()])?;
        }
        Change::Policy { node, policy } if *policy == Policy::default() => {
            tx.execute(
                "DELETE FROM policies WHERE node_id = ?1",
                [node.0.as_slice()],
            )?;
        }
        Change::Policy { node, policy } => {
            tx.execute(
                "INSERT OR REPLACE INTO policies (node_id, deletable, exportable)
                 VALUES (?1, ?2, ?3)",
                params![node.0.as_slice(), policy.deletable, policy.exportable],
            )?;
        }
    }
    Ok(())
}

impl Store {
    pub fn node(&self, id: NodeId) -> Result<Option<Node>> {
        read_node(&self.conn, &id)
    }

    /// All connections, oldest first.
    pub fn edges(&self) -> Result<Vec<Edge>> {
        let mut stmt = self
            .conn
            .prepare("SELECT edge FROM edges ORDER BY created_ms")?;
        let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        rows.map(|b| Ok(postcard::from_bytes(&b?)?)).collect()
    }

    /// The ledger entries that shaped a node, oldest first — every version is still there.
    pub fn history(&self, id: NodeId) -> Result<Vec<LedgerEntry>> {
        let node = self.node(id)?.ok_or(Error::UnknownNode(id))?;
        let mut stmt = self
            .conn
            .prepare("SELECT head, hash, sig, content FROM ledger WHERE hash = ?1")?;
        node.history
            .iter()
            .map(|h| decode_entry(stmt.query_row([h.0.as_slice()], raw_entry)?))
            .collect()
    }

    /// Newest first, optionally filtered by kind.
    pub fn nodes(&self, kind: Option<NodeKind>) -> Result<Vec<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT node FROM nodes WHERE ?1 IS NULL OR kind = ?1 ORDER BY updated_ms DESC",
        )?;
        let rows = stmt.query_map([kind.map(NodeKind::as_str)], |r| r.get::<_, Vec<u8>>(0))?;
        rows.map(|b| Ok(postcard::from_bytes(&b?)?)).collect()
    }

    /// This device's chain, oldest first.
    pub fn entries(&self) -> Result<Vec<LedgerEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT head, hash, sig, content FROM ledger WHERE author = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map([self.device().0.as_slice()], raw_entry)?;
        rows.map(|r| decode_entry(r?)).collect()
    }

    pub fn verify(&self) -> Result<VerifyReport> {
        Ok(VerifyReport::of(self.device(), &self.entries()?))
    }
}

/// A v1 database: no schema version yet, and the whole entry in one `entry` column.
fn is_v1(conn: &Connection) -> Result<bool> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let entry_column: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('ledger') WHERE name = 'entry'",
        [],
        |r| r.get(0),
    )?;
    Ok(version == 0 && entry_column > 0)
}

/// `head, hash, sig, content` as stored.
type RawEntry = (Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>);

fn raw_entry(r: &rusqlite::Row) -> rusqlite::Result<RawEntry> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

fn decode_entry((head, hash, sig, content): RawEntry) -> Result<LedgerEntry> {
    Ok(LedgerEntry {
        head: postcard::from_bytes(&head)?,
        hash: Hash(
            hash.try_into()
                .map_err(|_| Error::Invalid("ungültiger Hash in der Datenbank"))?,
        ),
        sig: Sig(sig
            .try_into()
            .map_err(|_| Error::Invalid("ungültige Signatur in der Datenbank"))?),
        content: content.map(|c| postcard::from_bytes(&c)).transpose()?,
    })
}

fn write_node(conn: &Connection, node: &Node) -> Result<usize> {
    Ok(conn.execute(
        "INSERT INTO nodes (id, kind, updated_ms, node) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (id) DO UPDATE SET updated_ms = excluded.updated_ms, node = excluded.node",
        params![
            node.id.0.as_slice(),
            node.kind.as_str(),
            node.updated_ms as i64,
            postcard::to_allocvec(node)?
        ],
    )?)
}

fn write_edge(conn: &Connection, edge: &Edge) -> Result<usize> {
    Ok(conn.execute(
        "INSERT INTO edges (id, from_id, to_id, kind, created_ms, edge)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            edge.id.0.as_slice(),
            edge.from.0.as_slice(),
            edge.to.0.as_slice(),
            edge.kind.as_str(),
            edge.created_ms as i64,
            postcard::to_allocvec(edge)?
        ],
    )?)
}

fn read_node(conn: &Connection, id: &NodeId) -> Result<Option<Node>> {
    conn.query_row(
        "SELECT node FROM nodes WHERE id = ?1",
        [id.0.as_slice()],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()?
    .map(|b| postcard::from_bytes(&b))
    .transpose()
    .map_err(Error::from)
}

fn read_edge(conn: &Connection, id: &EdgeId) -> Result<Option<Edge>> {
    conn.query_row(
        "SELECT edge FROM edges WHERE id = ?1",
        [id.0.as_slice()],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .optional()?
    .map(|b| postcard::from_bytes(&b))
    .transpose()
    .map_err(Error::from)
}

/// Only nodes that differ from the default have a row.
fn read_policy(conn: &Connection, id: &NodeId) -> Result<Policy> {
    Ok(conn
        .query_row(
            "SELECT deletable, exportable FROM policies WHERE node_id = ?1",
            [id.0.as_slice()],
            |r| {
                Ok(Policy {
                    deletable: r.get(0)?,
                    exportable: r.get(1)?,
                })
            },
        )
        .optional()?
        .unwrap_or_default())
}

fn node_ids(conn: &Connection, sql: &str) -> Result<Vec<NodeId>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
    rows.map(|b| {
        let bytes: [u8; 16] = b?
            .try_into()
            .map_err(|_| Error::Invalid("ungültige Knoten-ID in der Datenbank"))?;
        Ok(NodeId(bytes))
    })
    .collect()
}

fn edge_exists(conn: &Connection, from: &NodeId, to: &NodeId, kind: EdgeKind) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM edges WHERE from_id = ?1 AND to_id = ?2 AND kind = ?3",
            params![from.0.as_slice(), to.0.as_slice(), kind.as_str()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory(Signer::from_seed([3; 32])).unwrap()
    }

    #[test]
    fn create_list_and_verify() {
        let mut s = store();
        s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        s.create_node(NodeKind::Evidence, "5 km gelaufen", "Strava")
            .unwrap();

        assert_eq!(s.nodes(None).unwrap().len(), 2);
        assert_eq!(s.nodes(Some(NodeKind::Goal)).unwrap().len(), 1);
        let r = s.verify().unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.entries, 2);
    }

    #[test]
    fn update_keeps_old_version_in_ledger() {
        let mut s = store();
        let n = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        let n2 = s.update_node(n.id, "Laufen 3x/Woche", "").unwrap();

        assert_eq!(n2.version, 2);
        assert_eq!(s.nodes(None).unwrap().len(), 1);
        assert_eq!(s.entries().unwrap().len(), 2);
        assert!(s.verify().unwrap().ok);
    }

    #[test]
    fn empty_title_and_unknown_node_are_rejected() {
        let mut s = store();
        assert!(matches!(
            s.create_node(NodeKind::Goal, "  ", ""),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            s.update_node(NodeId([1; 16]), "x", ""),
            Err(Error::UnknownNode(_))
        ));
        assert_eq!(
            s.entries().unwrap().len(),
            0,
            "failed ops must not touch the ledger"
        );
    }

    #[test]
    fn tampering_in_the_database_is_detected() {
        let mut s = store();
        s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        s.create_node(NodeKind::Goal, "Lesen", "").unwrap();

        // Someone edits the stored content behind the app's back.
        let e = s.entries().unwrap().remove(0);
        let mut parts = e.content.unwrap();
        parts[0].data.0 = postcard::to_allocvec(&("Nicht laufen", "")).unwrap();
        s.conn
            .execute(
                "UPDATE ledger SET content = ?1 WHERE seq = 0",
                [postcard::to_allocvec(&parts).unwrap()],
            )
            .unwrap();
        let r = s.verify().unwrap();
        assert!(!r.ok);
        assert!(r.error.unwrap().contains("Eintrag 0"));

        // Removing content is not tampering: the chain stays valid (E2).
        s.conn
            .execute("UPDATE ledger SET content = NULL WHERE seq = 0", [])
            .unwrap();
        let r = s.verify().unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.pruned, 1);

        // Changing the signed head is.
        let mut e = s.entries().unwrap().remove(1);
        e.head.ts_ms += 1;
        s.conn
            .execute(
                "UPDATE ledger SET head = ?1 WHERE seq = 1",
                [postcard::to_allocvec(&e.head).unwrap()],
            )
            .unwrap();
        let r = s.verify().unwrap();
        assert!(!r.ok);
        assert!(r.error.unwrap().contains("Eintrag 1"));
    }

    #[test]
    fn a_v1_database_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("symbiot.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE ledger (author BLOB, seq INTEGER, hash BLOB, entry BLOB);
                 INSERT INTO ledger VALUES (x'00', 0, x'00', x'00');",
            )
            .unwrap();
        }
        let device = {
            let first = Store::open_dir(dir.path()).unwrap();
            assert_eq!(
                first.set_aside_v1(),
                Some(dir.path().join("symbiot.v1.db").as_path())
            );
            first.device()
        };

        let mut s = Store::open_dir(dir.path()).unwrap();
        assert!(s.set_aside_v1().is_none(), "only once");
        assert!(dir.path().join("symbiot.v1.db").exists());
        assert_eq!(s.device(), device, "the device keeps its key");
        assert!(s.entries().unwrap().is_empty());
        s.create_node(NodeKind::Goal, "Neu", "").unwrap();
        assert!(s.verify().unwrap().ok);
    }

    #[test]
    fn link_and_history() {
        let mut s = store();
        let goal = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        let ev = s.create_node(NodeKind::Evidence, "5 km", "").unwrap();
        s.update_node(goal.id, "Laufen 3x/Woche", "").unwrap();

        let e = s.link(ev.id, goal.id, EdgeKind::Supports).unwrap();
        assert_eq!((e.from, e.to), (ev.id, goal.id));
        assert_eq!(s.edges().unwrap(), vec![e]);

        assert!(matches!(
            s.link(ev.id, goal.id, EdgeKind::Supports),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            s.link(goal.id, goal.id, EdgeKind::RelatesTo),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            s.link(goal.id, NodeId([7; 16]), EdgeKind::RelatesTo),
            Err(Error::UnknownNode(_))
        ));

        let h = s.history(goal.id).unwrap();
        assert_eq!(h.len(), 2);
        assert!(matches!(h[0].op(), Ok(Op::CreateNode { title, .. }) if title == "Laufen"));
        assert!(
            matches!(h[1].op(), Ok(Op::UpdateNode { title, .. }) if title == "Laufen 3x/Woche")
        );

        let r = s.verify().unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.entries, 4, "rejected links must not touch the ledger");
    }

    #[test]
    fn proposals_change_data_only_by_decision() {
        let mut s = store();
        let goal = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        let proof = s.create_node(NodeKind::Evidence, "5 km", "").unwrap();
        let proposal = Proposal {
            goal: goal.id,
            kind: NodeKind::Goal,
            title: "Laufschuhe kaufen".into(),
            body: "Alte Schuhe sind durch".into(),
            sources: vec![goal.id, proof.id],
            model: "qwen3:8b".into(),
        };

        // A proposal without sources is refused and leaves no trace.
        let mut no_sources = proposal.clone();
        no_sources.sources.clear();
        assert!(matches!(
            s.accept_proposal(no_sources),
            Err(Error::Invalid(_))
        ));
        assert_eq!(s.entries().unwrap().len(), 2);

        s.reject_proposal(proposal.clone()).unwrap();
        assert_eq!(s.nodes(None).unwrap().len(), 2, "rejecting adds no node");
        assert!(matches!(
            s.entries().unwrap()[2].op(),
            Ok(Op::RejectProposal { proposal: p }) if p.title == "Laufschuhe kaufen"
        ));

        let (node, edge) = s.accept_proposal(proposal).unwrap();
        assert_eq!(s.nodes(None).unwrap().len(), 3);
        assert_eq!(
            (edge.from, edge.to, edge.kind),
            (node.id, goal.id, EdgeKind::DerivesFrom)
        );
        assert_eq!(s.edges().unwrap(), vec![edge]);
        // The new node's provenance is the decision entry, which carries the sources.
        let h = s.history(node.id).unwrap();
        assert!(
            matches!(h[0].op(), Ok(Op::AcceptProposal { proposal: p }) if p.sources.len() == 2)
        );

        let r = s.verify().unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.entries, 4);
    }

    #[test]
    fn sensitivity_is_recorded_and_reversible() {
        let mut s = store();
        let n = s
            .create_node(NodeKind::Reflection, "Arzttermin", "")
            .unwrap();
        s.set_sensitive(n.id, true).unwrap();
        s.set_sensitive(n.id, true).unwrap();
        assert_eq!(s.sensitive_nodes().unwrap(), vec![n.id]);
        s.set_sensitive(n.id, false).unwrap();
        assert!(s.sensitive_nodes().unwrap().is_empty());
        assert!(matches!(
            s.set_sensitive(NodeId([1; 16]), true),
            Err(Error::UnknownNode(_))
        ));
        assert_eq!(s.entries().unwrap().len(), 4);
        assert_eq!(
            s.node(n.id).unwrap().unwrap(),
            n,
            "the node itself is unchanged"
        );
    }

    #[test]
    fn data_and_identity_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let device = {
            let mut s = Store::open_dir(dir.path()).unwrap();
            s.create_node(NodeKind::Reflection, "Heute ruhig", "")
                .unwrap();
            s.device()
        };
        let s = Store::open_dir(dir.path()).unwrap();
        assert_eq!(s.device(), device);
        assert_eq!(s.nodes(None).unwrap().len(), 1);
        assert!(s.verify().unwrap().ok);
    }

    /// E2/E3 acceptance: after deleting a deletable node its text is gone from the database
    /// file itself, the chain still verifies, and a node that is not deletable is only hidden.
    #[test]
    fn deleting_removes_the_content_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open_dir(dir.path()).unwrap();
        let goal = s
            .create_node(NodeKind::Goal, "Geheimes Ziel", "Streng vertraulich")
            .unwrap();
        s.update_node(goal.id, "Geheimes Ziel, Fassung 2", "Noch vertraulicher")
            .unwrap();
        let keep = Policy {
            deletable: false,
            exportable: false,
        };
        let proof = s
            .create_node_with(NodeKind::Evidence, "Bleibender Beleg", "", keep)
            .unwrap();
        assert_eq!(
            s.entries().unwrap().len(),
            4,
            "node and policy, one transaction"
        );
        assert_eq!(s.policy(proof.id).unwrap(), keep);
        assert_eq!(s.not_exportable_nodes().unwrap(), vec![proof.id]);
        s.link(proof.id, goal.id, EdgeKind::Supports).unwrap();

        assert_eq!(s.delete_node(goal.id).unwrap(), 2, "both versions");
        assert!(s.node(goal.id).unwrap().is_none());
        assert!(s.edges().unwrap().is_empty(), "its connections go with it");
        assert!(matches!(s.delete_node(goal.id), Err(Error::UnknownNode(_))));
        assert_eq!(
            s.delete_node(proof.id).unwrap(),
            0,
            "not deletable: hidden only"
        );
        assert!(s.nodes(None).unwrap().is_empty());

        let r = s.verify().unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.pruned, 2);
        drop(s);

        let mut raw = Vec::new();
        for f in ["symbiot.db", "symbiot.db-wal"] {
            if let Ok(bytes) = std::fs::read(dir.path().join(f)) {
                raw.extend(bytes);
            }
        }
        let contains = |t: &str| raw.windows(t.len()).any(|w| w == t.as_bytes());
        assert!(!contains("Geheimes Ziel"), "title still in the file");
        assert!(!contains("vertraulich"), "body still in the file");
        assert!(
            contains("Bleibender Beleg"),
            "a node that is not deletable keeps its content"
        );
    }

    #[test]
    fn unlink_and_policy_changes() {
        let mut s = store();
        let a = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        let b = s.create_node(NodeKind::Evidence, "5 km", "").unwrap();
        let e = s.link(b.id, a.id, EdgeKind::Supports).unwrap();
        s.unlink(e.id).unwrap();
        assert!(s.edges().unwrap().is_empty());
        assert!(matches!(s.unlink(e.id), Err(Error::UnknownEdge(_))));
        // The same connection can be made again after unlinking.
        s.link(b.id, a.id, EdgeKind::Supports).unwrap();

        let strict = Policy {
            deletable: false,
            exportable: true,
        };
        s.set_policy(a.id, strict).unwrap();
        assert_eq!(s.policy(a.id).unwrap(), strict);
        s.set_policy(a.id, Policy::default()).unwrap();
        assert_eq!(s.policy(a.id).unwrap(), Policy::default());
        assert!(matches!(
            s.set_policy(NodeId([1; 16]), strict),
            Err(Error::UnknownNode(_))
        ));
        assert!(s.verify().unwrap().ok);
    }
}
