//! Symbiot store — SQLite on every platform (desktop, mobile, server).
//!
//! The ledger table is the truth; the nodes table is a projection of it,
//! updated in the same transaction as every append.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use symbiot_core::{
    ApplyError, Change, Edge, EdgeKind, LedgerEntry, Node, NodeId, NodeKind, Op, Proposal, PubKey,
    Signer,
};

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
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Store {
    conn: Connection,
    signer: Signer,
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

    pub fn open(db: &Path, signer: Signer) -> Result<Self> {
        Self::init(Connection::open(db)?, signer)
    }

    pub fn open_in_memory(signer: Signer) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, signer)
    }

    fn init(conn: Connection, signer: Signer) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS ledger (
                 author BLOB NOT NULL,
                 seq    INTEGER NOT NULL,
                 hash   BLOB NOT NULL UNIQUE,
                 entry  BLOB NOT NULL,
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
             );",
        )?;
        Ok(Store { conn, signer })
    }

    /// This device's identity.
    pub fn device(&self) -> PubKey {
        self.signer.public()
    }

    pub fn create_node(&mut self, kind: NodeKind, title: &str, body: &str) -> Result<Node> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("Titel darf nicht leer sein"));
        }
        match self.append(Op::CreateNode {
            kind,
            title: title.into(),
            body: body.into(),
        })? {
            Change::Node(n) => Ok(n),
            _ => unreachable!("CreateNode yields a node"),
        }
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
        let mut stmt = self.conn.prepare("SELECT node_id FROM sensitive")?;
        let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        rows.map(|b| {
            let b = b?;
            let bytes: [u8; 16] = b
                .try_into()
                .map_err(|_| Error::Invalid("ungültige Knoten-ID in der Datenbank"))?;
            Ok(NodeId(bytes))
        })
        .collect()
    }

    fn append(&mut self, op: Op) -> Result<Change> {
        let author = self.device();
        let tx = self.conn.transaction()?;

        let tip: Option<LedgerEntry> = tx
            .query_row(
                "SELECT entry FROM ledger WHERE author = ?1 ORDER BY seq DESC LIMIT 1",
                [author.0.as_slice()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|b| postcard::from_bytes(&b))
            .transpose()?;

        // Load what the core rules need, then let the core decide (same rules on every platform).
        let referenced: Vec<NodeId> = match &op {
            Op::CreateNode { .. } => vec![],
            Op::UpdateNode { node, .. } | Op::MarkSensitive { node, .. } => vec![*node],
            Op::Link { from, to, .. } => vec![*from, *to],
            Op::AcceptProposal { proposal } | Op::RejectProposal { proposal } => {
                std::iter::once(proposal.goal)
                    .chain(proposal.sources.iter().copied())
                    .collect()
            }
        };
        let mut known = HashMap::new();
        for id in referenced {
            if let Some(n) = read_node(&tx, &id)? {
                known.insert(id, n);
            }
        }
        let duplicate = match &op {
            Op::Link { from, to, kind } => edge_exists(&tx, from, to, *kind)?,
            _ => false,
        };

        let entry = self.signer.append(tip.as_ref(), now_ms(), op);
        let change =
            Change::of(&entry, |id| known.get(id).cloned(), |_, _, _| duplicate).map_err(|e| {
                match e {
                    ApplyError::UnknownNode(id) => Error::UnknownNode(id),
                    ApplyError::SelfLink => {
                        Error::Invalid("Ein Knoten kann nicht mit sich selbst verbunden werden")
                    }
                    ApplyError::DuplicateEdge => Error::Invalid("Diese Verbindung gibt es schon"),
                    ApplyError::NoSources => {
                        Error::Invalid("Ein Vorschlag ohne Quellen wird verworfen")
                    }
                    ApplyError::EmptyTitle => Error::Invalid("Titel darf nicht leer sein"),
                }
            })?;

        tx.execute(
            "INSERT INTO ledger (author, seq, hash, entry) VALUES (?1, ?2, ?3, ?4)",
            params![
                author.0.as_slice(),
                entry.seq as i64,
                entry.hash.0.as_slice(),
                postcard::to_allocvec(&entry)?
            ],
        )?;
        match &change {
            Change::Node(node) => write_node(&tx, node)?,
            Change::Edge(edge) => write_edge(&tx, edge)?,
            Change::Accepted { node, edge } => {
                write_node(&tx, node)?;
                write_edge(&tx, edge)?
            }
            Change::Rejected => 0,
            Change::Sensitivity {
                node,
                sensitive: true,
            } => tx.execute(
                "INSERT OR IGNORE INTO sensitive (node_id) VALUES (?1)",
                [node.0.as_slice()],
            )?,
            Change::Sensitivity {
                node,
                sensitive: false,
            } => tx.execute(
                "DELETE FROM sensitive WHERE node_id = ?1",
                [node.0.as_slice()],
            )?,
        };
        tx.commit()?;
        Ok(change)
    }

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
            .prepare("SELECT entry FROM ledger WHERE hash = ?1")?;
        node.history
            .iter()
            .map(|h| {
                let bytes: Vec<u8> = stmt.query_row([h.0.as_slice()], |r| r.get(0))?;
                Ok(postcard::from_bytes(&bytes)?)
            })
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
        let mut stmt = self
            .conn
            .prepare("SELECT entry FROM ledger WHERE author = ?1 ORDER BY seq")?;
        let rows = stmt.query_map([self.device().0.as_slice()], |r| r.get::<_, Vec<u8>>(0))?;
        rows.map(|b| Ok(postcard::from_bytes(&b?)?)).collect()
    }

    pub fn verify(&self) -> Result<VerifyReport> {
        Ok(VerifyReport::of(self.device(), &self.entries()?))
    }
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

        // Someone edits the stored entry behind the app's back.
        let mut e = s.entries().unwrap().remove(0);
        if let Op::CreateNode { title, .. } = &mut e.op {
            *title = "Nicht laufen".into();
        }
        s.conn
            .execute(
                "UPDATE ledger SET entry = ?1 WHERE seq = 0",
                [postcard::to_allocvec(&e).unwrap()],
            )
            .unwrap();

        let r = s.verify().unwrap();
        assert!(!r.ok);
        assert!(r.error.unwrap().contains("Eintrag 0"));
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
        assert!(matches!(&h[0].op, Op::CreateNode { title, .. } if title == "Laufen"));
        assert!(matches!(&h[1].op, Op::UpdateNode { title, .. } if title == "Laufen 3x/Woche"));

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
            &s.entries().unwrap()[2].op,
            Op::RejectProposal { proposal: p } if p.title == "Laufschuhe kaufen"
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
        assert!(matches!(&h[0].op, Op::AcceptProposal { proposal: p } if p.sources.len() == 2));

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
}
