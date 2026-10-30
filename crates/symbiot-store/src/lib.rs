//! Symbiot store — SQLite on every platform (desktop, mobile, server).
//!
//! The ledger table is the truth; the nodes table is a projection of it,
//! updated in the same transaction as every append.

use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use symbiot_core::{LedgerEntry, Node, NodeId, NodeKind, Op, PubKey, Signer};

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
             CREATE INDEX IF NOT EXISTS nodes_kind ON nodes (kind, updated_ms);",
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
        self.append(Op::CreateNode {
            kind,
            title: title.into(),
            body: body.into(),
        })
    }

    /// Editing appends a new entry; earlier versions stay in the ledger.
    pub fn update_node(&mut self, id: NodeId, title: &str, body: &str) -> Result<Node> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("Titel darf nicht leer sein"));
        }
        self.append(Op::UpdateNode {
            node: id,
            title: title.into(),
            body: body.into(),
        })
    }

    fn append(&mut self, op: Op) -> Result<Node> {
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

        let entry = self.signer.append(tip.as_ref(), now_ms(), op);
        let id = Node::target(&entry);
        let current = read_node(&tx, &id)?;
        let node = Node::apply(current, &entry).map_err(|_| Error::UnknownNode(id))?;

        tx.execute(
            "INSERT INTO ledger (author, seq, hash, entry) VALUES (?1, ?2, ?3, ?4)",
            params![
                author.0.as_slice(),
                entry.seq as i64,
                entry.hash.0.as_slice(),
                postcard::to_allocvec(&entry)?
            ],
        )?;
        tx.execute(
            "INSERT INTO nodes (id, kind, updated_ms, node) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (id) DO UPDATE SET updated_ms = excluded.updated_ms, node = excluded.node",
            params![
                id.0.as_slice(),
                node.kind.as_str(),
                node.updated_ms as i64,
                postcard::to_allocvec(&node)?
            ],
        )?;
        tx.commit()?;
        Ok(node)
    }

    pub fn node(&self, id: NodeId) -> Result<Option<Node>> {
        read_node(&self.conn, &id)
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
