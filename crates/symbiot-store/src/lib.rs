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
    ApplyError, Change, Edge, EdgeId, EdgeKind, Hash, Head, LedgerEntry, Lookup, Node, NodeId,
    NodeKind, Op, Policy, Proposal, PubKey, Rule, RuleId, Sig, Signer, Trust, replay,
};

/// `PRAGMA user_version` of the current schema. 0 with an `entry` column is the v1 format.
/// 3: nodes carry `concurrent` (M4); the projection is rebuilt from the ledger on upgrade.
/// 4: rules for what is sensitive and marks by hand (E4); `sensitive` holds the result.
/// 5: raw data (E14) in `evidence`.
/// 6: `evidence` knows its origin, `evidence_fts` indexes the words (#30); rebuilt from the ledger.
/// 7: `evidence_file` lists the files raw data references (#28); rebuilt from the ledger.
/// 8: `evidence_actor` (identifiers per item) and `identity_decision` (#31); rebuilt.
const SCHEMA_VERSION: i64 = 8;

mod evidence;
mod files;
mod persons;
mod sync;

pub use evidence::{EvidenceQuery, Origin, Recorded, StoredEvidence};
pub use files::{FileInfo, MAX_FILE_BYTES};
pub use persons::{Identifier, Person, Suggestion};

pub use symbiot_core::VerifyReport;
pub use sync::{ChainHead, DeviceInfo, new_words};

/// Why a node is sensitive or not (E4): marked by hand, which rules match.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sensitivity {
    pub sensitive: bool,
    /// Set by hand; wins over the rules.
    pub by_hand: Option<bool>,
    /// Labels of the rules that match.
    pub rules: Vec<String>,
}

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
    /// A chain from another device that does not verify or does not attach.
    #[error("Kette abgelehnt: {0}")]
    BadChain(String),
    /// The device does not belong to the person of this device (or is revoked).
    #[error("Gerät gehört nicht zu diesem Personen-Schlüssel")]
    Untrusted,
    /// Two validly signed entries at the same position: the device key was used twice.
    #[error("Verzweigung in der Kette von {author} bei Eintrag {seq}")]
    Fork { author: PubKey, seq: u64 },
    /// Raw data that breaks the contract (E14). Nothing of the import was written.
    #[error("Beleg {id}: {reason}")]
    BadEvidence { id: String, reason: String },
    /// The file store (#28): a file could not be read or written, or does not match its hash.
    #[error("Dateien: {0}")]
    Files(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Store {
    conn: Connection,
    signer: Signer,
    set_aside: Option<PathBuf>,
    /// Folder of the file store (#28); `None` for a store in memory.
    files: Option<PathBuf>,
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
        let mut store = Self::open(&dir.join("symbiot.db"), Signer::from_seed(seed))?;
        store.files = Some(dir.join("files"));
        Ok(store)
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
             );
             CREATE TABLE IF NOT EXISTS by_hand (
                 node_id   BLOB PRIMARY KEY,
                 sensitive INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS rules (
                 id   BLOB PRIMARY KEY,
                 rule BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS forks (
                 author BLOB NOT NULL,
                 seq    INTEGER NOT NULL,
                 hash   BLOB NOT NULL,
                 entry  BLOB NOT NULL,
                 PRIMARY KEY (author, seq, hash)
             );",
        )?;
        if version > 0 && version < 8 {
            conn.execute_batch(evidence::DROP)?;
        }
        conn.execute_batch(evidence::SCHEMA)?;
        conn.execute_batch(persons::SCHEMA)?;
        if version > 0 && version < SCHEMA_VERSION {
            // Only the projection changed shape; the ledger is the truth.
            let tx = conn.unchecked_transaction()?;
            rebuild_projection(&tx, &signer.public())?;
            evidence::rebuild_evidence(&tx)?;
            tx.commit()?;
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Store {
            conn,
            signer,
            set_aside: None,
            files: None,
        })
    }

    /// This device's identity.
    pub fn device(&self) -> PubKey {
        self.signer.public()
    }

    /// Signs a request to another device with this device's key (M4 sync).
    pub fn sign_message(&self, message: &[u8]) -> Sig {
        self.signer.sign_message(message)
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
        // The version being edited: if another device changed the node meanwhile, the
        // projection marks the result as concurrent (M4).
        let base = self.node(id)?.and_then(|n| n.history.last().copied());
        match self.append(Op::UpdateNode {
            node: id,
            title: title.into(),
            body: body.into(),
            base,
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

    /// E4: from now on, nodes containing one of `keywords` are sensitive, on every device.
    /// `model` names the AI that suggested the rule, if any. Keywords are cleaned first.
    pub fn add_rule(
        &mut self,
        label: &str,
        keywords: &[String],
        model: Option<String>,
    ) -> Result<Rule> {
        match self.append(Op::AddSensitivityRule {
            label: label.trim().into(),
            keywords: symbiot_core::rule::clean_keywords(keywords),
            model,
        })? {
            Change::Rule(r) => Ok(r),
            _ => unreachable!("AddSensitivityRule yields a rule"),
        }
    }

    pub fn remove_rule(&mut self, id: RuleId) -> Result<()> {
        self.append(Op::RemoveSensitivityRule { rule: id })
            .map(|_| ())
    }

    /// The rules in force, oldest first.
    pub fn rules(&self) -> Result<Vec<Rule>> {
        let mut rules = read_rules(&self.conn)?;
        rules.sort_by_key(|r| r.created_ms);
        Ok(rules)
    }

    /// Why a node is sensitive or not: by hand, by which rules (E4).
    pub fn sensitivity(&self, id: NodeId) -> Result<Sensitivity> {
        let node = self.node(id)?.ok_or(Error::UnknownNode(id))?;
        let by_hand = read_by_hand(&self.conn, &id)?;
        let rules: Vec<String> = self
            .rules()?
            .into_iter()
            .filter(|r| r.matches(&node))
            .map(|r| r.label)
            .collect();
        Ok(Sensitivity {
            sensitive: by_hand.unwrap_or(!rules.is_empty()),
            by_hand,
            rules,
        })
    }

    /// All nodes currently sensitive — by hand or by a rule.
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
    rule: Option<Rule>,
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

    fn rule(&self, id: &RuleId) -> Option<Rule> {
        self.rule.clone().filter(|r| r.id == *id)
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
        Op::CreateNode { .. }
        | Op::Unlink { .. }
        | Op::CertifyDevice { .. }
        | Op::RevokeDevice { .. }
        | Op::AddSensitivityRule { .. }
        | Op::RemoveSensitivityRule { .. }
        | Op::RecordEvidence { .. }
        | Op::DecideIdentity { .. } => vec![],
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
        Op::RemoveSensitivityRule { rule } => {
            known.rule = read_rules(tx)?.into_iter().find(|r| r.id == *rule)
        }
        _ => {}
    }

    let entry = signer.append(tip.as_ref(), next_ts(tx)?, op);
    let change = Change::of(&entry, &known).map_err(|e| match e {
        ApplyError::UnknownNode(id) => Error::UnknownNode(id),
        ApplyError::UnknownEdge(id) => Error::UnknownEdge(id),
        ApplyError::SelfLink => {
            Error::Invalid("Ein Knoten kann nicht mit sich selbst verbunden werden")
        }
        ApplyError::DuplicateEdge => Error::Invalid("Diese Verbindung gibt es schon"),
        ApplyError::NoSources => Error::Invalid("Ein Vorschlag ohne Quellen wird verworfen"),
        ApplyError::EmptyTitle => Error::Invalid("Titel darf nicht leer sein"),
        ApplyError::UnknownRule(_) => Error::Invalid("Diese Regel gibt es nicht"),
        ApplyError::EmptyRule => Error::Invalid(
            "Eine Regel braucht einen Namen und Stichwörter mit mindestens drei Buchstaben",
        ),
        ApplyError::Undecodable(_) => Error::Invalid("Eintrag nicht lesbar"),
        ApplyError::BadEvidence => Error::Invalid("Rohdaten verletzen das Format (E14)"),
        ApplyError::BadIdentity => {
            Error::Invalid("Zusammenführen braucht zwei verschiedene Kennungen mit Schema")
        }
    })?;

    insert_entry(tx, &entry)?;
    project(tx, &change)?;
    Ok(change)
}

fn insert_entry(tx: &Connection, entry: &LedgerEntry) -> Result<()> {
    tx.execute(
        "INSERT INTO ledger (author, seq, hash, sig, head, content)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            entry.head.author.0.as_slice(),
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
    Ok(())
}

/// One device's chain as stored, oldest first.
fn chain_of(conn: &Connection, author: &PubKey) -> Result<Vec<LedgerEntry>> {
    let mut stmt =
        conn.prepare("SELECT head, hash, sig, content FROM ledger WHERE author = ?1 ORDER BY seq")?;
    let rows = stmt.query_map([author.0.as_slice()], raw_entry)?;
    rows.map(|r| decode_entry(r?)).collect()
}

/// Every chain in the ledger, each oldest first.
fn all_chains(conn: &Connection) -> Result<Vec<Vec<LedgerEntry>>> {
    let mut chains: Vec<Vec<LedgerEntry>> = Vec::new();
    let mut stmt =
        conn.prepare("SELECT head, hash, sig, content FROM ledger ORDER BY author, seq")?;
    let rows = stmt.query_map([], raw_entry)?;
    for row in rows {
        let e = decode_entry(row?)?;
        match chains.last_mut() {
            Some(c) if c[0].head.author == e.head.author => c.push(e),
            _ => chains.push(vec![e]),
        }
    }
    Ok(chains)
}

/// Applies a change to the projection tables — the SQLite twin of `State::apply`.
fn project(tx: &Connection, change: &Change) -> Result<()> {
    match change {
        Change::Node(node) => {
            write_node(tx, node)?;
            refresh_sensitive(tx, Some(node))?;
        }
        Change::Edge(edge) => {
            write_edge(tx, edge)?;
        }
        Change::Accepted { node, edge } => {
            write_node(tx, node)?;
            write_edge(tx, edge)?;
            refresh_sensitive(tx, Some(node))?;
        }
        Change::Rejected | Change::Device => {}
        Change::Evidence { .. } => evidence::project_evidence(tx, change)?,
        Change::Identity { .. } => persons::project_identity(tx, change)?,
        Change::Sensitivity { node, sensitive } => {
            tx.execute(
                "INSERT OR REPLACE INTO by_hand (node_id, sensitive) VALUES (?1, ?2)",
                params![node.0.as_slice(), sensitive],
            )?;
            if let Some(n) = read_node(tx, node)? {
                refresh_sensitive(tx, Some(&n))?;
            }
        }
        Change::Rule(rule) => {
            tx.execute(
                "INSERT INTO rules (id, rule) VALUES (?1, ?2)",
                params![rule.id.0.as_slice(), postcard::to_allocvec(rule)?],
            )?;
            refresh_sensitive(tx, None)?;
        }
        Change::RuleRemoved(id) => {
            tx.execute("DELETE FROM rules WHERE id = ?1", [id.0.as_slice()])?;
            refresh_sensitive(tx, None)?;
        }
        Change::Deleted { node, prune } => {
            let id = node.0.as_slice();
            tx.execute("DELETE FROM nodes WHERE id = ?1", [id])?;
            tx.execute("DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1", [id])?;
            tx.execute("DELETE FROM sensitive WHERE node_id = ?1", [id])?;
            tx.execute("DELETE FROM policies WHERE node_id = ?1", [id])?;
            tx.execute("DELETE FROM by_hand WHERE node_id = ?1", [id])?;
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

    /// Every chain known here, this device's first (M4).
    pub fn verify_all(&self) -> Result<Vec<VerifyReport>> {
        let me = self.device();
        let mut reports: Vec<VerifyReport> = all_chains(&self.conn)?
            .iter()
            .map(|c| VerifyReport::of(c[0].head.author, c))
            .collect();
        reports.sort_by_key(|r| r.device != me);
        Ok(reports)
    }
}

/// The time for a new entry: now, but never before anything already in the ledger. Then a new
/// local entry is also the newest in the merged order of all chains (see `symbiot_core::replay`),
/// even if another device's clock runs ahead — so the incremental projection stays the same as
/// a full rebuild.
fn next_ts(conn: &Connection) -> Result<u64> {
    // Every chain is written this way, so its tip holds its newest time.
    let mut stmt = conn.prepare(
        "SELECT head FROM ledger AS l
         WHERE seq = (SELECT MAX(seq) FROM ledger WHERE author = l.author)",
    )?;
    let tips = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
    let mut newest = None;
    for head in tips {
        let ts = postcard::from_bytes::<Head>(&head?)?.ts_ms;
        newest = newest.max(Some(ts));
    }
    Ok(now_ms().max(newest.map_or(0, |t| t + 1)))
}

/// Builds the projection tables again from every chain in the ledger.
/// Only this device's chain and those of devices of the same person (E5) count.
fn rebuild_projection(tx: &Connection, me: &PubKey) -> Result<()> {
    let mut chains = all_chains(tx)?;
    // Raw data (E14) does not touch the graph and has its own table: leave it out of the replay,
    // so a rebuild does not decode every item ever loaded.
    for c in &mut chains {
        c.retain(|e| e.head.kind != symbiot_core::kind::EVIDENCE_BATCH);
    }
    chains.retain(|c| !c.is_empty());
    let refs: Vec<&[LedgerEntry]> = chains.iter().map(Vec::as_slice).collect();
    let person = refs
        .iter()
        .find(|c| c[0].head.author == *me)
        .and_then(|c| Trust::person_of(c));
    let trust = person.map(|p| Trust::of(p, refs.iter().copied()));
    let mut r = replay(&refs, |author, seq| {
        author == me || trust.as_ref().is_some_and(|t| t.accepts(author, seq))
    });
    // E3/E5: a node that is not exportable lives only on the device that made it. Its head
    // arrives here (the chain must stay whole), its content does not — so it is not shown.
    let elsewhere: Vec<NodeId> = r
        .state
        .policies
        .iter()
        .filter(|(id, p)| !p.exportable && r.state.nodes.get(id).is_some_and(|n| n.author != *me))
        .map(|(id, _)| *id)
        .collect();
    for id in &elsewhere {
        r.state.nodes.remove(id);
        r.state.edges.retain(|_, e| e.from != *id && e.to != *id);
        r.state.sensitive.remove(id);
    }
    tx.execute_batch(
        "DELETE FROM nodes; DELETE FROM edges; DELETE FROM sensitive; DELETE FROM policies;
         DELETE FROM by_hand; DELETE FROM rules; DELETE FROM identity_decision;",
    )?;
    for (entry, decision) in &r.state.identity {
        persons::project_identity(
            tx,
            &Change::Identity {
                entry: *entry,
                decision: decision.clone(),
            },
        )?;
    }
    for (id, sensitive) in &r.state.by_hand {
        tx.execute(
            "INSERT INTO by_hand (node_id, sensitive) VALUES (?1, ?2)",
            params![id.0.as_slice(), sensitive],
        )?;
    }
    for rule in r.state.rules.values() {
        tx.execute(
            "INSERT INTO rules (id, rule) VALUES (?1, ?2)",
            params![rule.id.0.as_slice(), postcard::to_allocvec(rule)?],
        )?;
    }
    for node in r.state.nodes.values() {
        write_node(tx, node)?;
    }
    for edge in r.state.edges.values() {
        write_edge(tx, edge)?;
    }
    for id in &r.state.sensitive {
        tx.execute(
            "INSERT INTO sensitive (node_id) VALUES (?1)",
            [id.0.as_slice()],
        )?;
    }
    for (id, p) in &r.state.policies {
        project(
            tx,
            &Change::Policy {
                node: *id,
                policy: *p,
            },
        )?;
    }
    for h in &r.prune {
        tx.execute(
            "UPDATE ledger SET content = NULL WHERE hash = ?1",
            [h.0.as_slice()],
        )?;
    }
    Ok(())
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

fn read_rules(conn: &Connection) -> Result<Vec<Rule>> {
    let mut stmt = conn.prepare("SELECT rule FROM rules")?;
    let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
    rows.map(|b| Ok(postcard::from_bytes(&b?)?)).collect()
}

fn read_by_hand(conn: &Connection, id: &NodeId) -> Result<Option<bool>> {
    Ok(conn
        .query_row(
            "SELECT sensitive FROM by_hand WHERE node_id = ?1",
            [id.0.as_slice()],
            |r| r.get(0),
        )
        .optional()?)
}

/// E4: writes the effective sensitivity of one node, or of all nodes after a rule changed.
/// The SQLite twin of `State::refresh_sensitive`.
fn refresh_sensitive(tx: &Connection, only: Option<&Node>) -> Result<()> {
    let rules = read_rules(tx)?;
    let nodes: Vec<Node> = match only {
        Some(n) => vec![n.clone()],
        None => {
            let mut stmt = tx.prepare("SELECT node FROM nodes")?;
            let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
            rows.map(|b| Ok(postcard::from_bytes(&b?)?))
                .collect::<Result<Vec<Node>>>()?
        }
    };
    for node in &nodes {
        let by_hand = read_by_hand(tx, &node.id)?;
        if symbiot_core::rule::is_sensitive(node, by_hand, &rules) {
            tx.execute(
                "INSERT OR IGNORE INTO sensitive (node_id) VALUES (?1)",
                [node.id.0.as_slice()],
            )?;
        } else {
            tx.execute(
                "DELETE FROM sensitive WHERE node_id = ?1",
                [node.id.0.as_slice()],
            )?;
        }
    }
    Ok(())
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

    /// Opening an older v2 database rebuilds the projection from the ledger (schema 3, M4).
    #[test]
    fn upgrade_rebuilds_the_projection() {
        let dir = tempfile::tempdir().unwrap();
        let (goal, edge) = {
            let mut s = Store::open_dir(dir.path()).unwrap();
            let goal = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
            let proof = s.create_node(NodeKind::Evidence, "5 km", "").unwrap();
            let goal = s.update_node(goal.id, "Laufen 3x", "").unwrap();
            let edge = s.link(proof.id, goal.id, EdgeKind::Supports).unwrap();
            s.set_sensitive(proof.id, true).unwrap();
            s.conn
                .execute_batch("DELETE FROM nodes; DELETE FROM edges; PRAGMA user_version = 2;")
                .unwrap();
            (goal, edge)
        };
        let s = Store::open_dir(dir.path()).unwrap();
        assert_eq!(s.node(goal.id).unwrap(), Some(goal));
        assert_eq!(s.edges().unwrap(), vec![edge]);
        assert_eq!(s.sensitive_nodes().unwrap().len(), 1);
        assert!(s.verify().unwrap().ok);
    }

    #[test]
    fn entry_times_never_go_backwards() {
        let mut s = store();
        for i in 0..5 {
            s.create_node(NodeKind::Goal, &format!("Ziel {i}"), "")
                .unwrap();
        }
        let ts: Vec<u64> = s.entries().unwrap().iter().map(|e| e.head.ts_ms).collect();
        assert!(ts.windows(2).all(|w| w[0] < w[1]), "{ts:?}");
    }

    /// E5: the same words always give the same person; a device cannot be taken over.
    #[test]
    fn a_device_belongs_to_one_person() {
        let words = new_words().unwrap();
        assert_eq!(words.split_whitespace().count(), 24);
        let mut s = store();
        assert_eq!(s.person().unwrap(), None);
        assert!(s.devices().unwrap().is_empty());
        let person = s.certify_self(&words.to_uppercase(), "PC").unwrap();
        assert_eq!(
            s.certify_self(&words, "PC").unwrap(),
            person,
            "repeating is harmless"
        );
        assert_eq!(s.entries().unwrap().len(), 1);
        assert!(matches!(
            s.certify_self(&new_words().unwrap(), "PC"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            s.certify_self("eins zwei drei", "PC"),
            Err(Error::Invalid(_))
        ));
        let devices = s.devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert!(devices[0].this_device && devices[0].name == "PC");
        assert!(s.trusts(&s.device()).unwrap());
    }

    /// E4: a rule marks matching nodes in the store, by hand wins, and it survives a rebuild.
    #[test]
    fn rules_mark_sensitive_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let (doctor, run) = {
            let mut s = Store::open_dir(dir.path()).unwrap();
            let doctor = s
                .create_node(NodeKind::Reflection, "Arzttermin", "")
                .unwrap();
            let run = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
            let r = s
                .add_rule(
                    " Gesundheit ",
                    &["Arzt".into(), "Befund".into(), "ei".into()],
                    Some("qwen3:8b".into()),
                )
                .unwrap();
            assert_eq!(r.keywords, ["arzt", "befund"], "cleaned before signing");
            assert_eq!(s.sensitive_nodes().unwrap(), vec![doctor.id]);
            assert_eq!(s.home_only_nodes().unwrap(), vec![doctor.id]);
            let why = s.sensitivity(doctor.id).unwrap();
            assert!(why.sensitive && why.by_hand.is_none());
            assert_eq!(why.rules, ["Gesundheit"]);

            // An edit brings the run under the rule; by hand takes it out again.
            s.update_node(run.id, "Laufen, Arzt sagt ok", "").unwrap();
            assert_eq!(s.sensitive_nodes().unwrap().len(), 2);
            s.set_sensitive(run.id, false).unwrap();
            assert_eq!(s.sensitive_nodes().unwrap(), vec![doctor.id]);
            assert_eq!(s.sensitivity(run.id).unwrap().by_hand, Some(false));
            assert!(matches!(
                s.add_rule("Leer", &["ab".into()], None),
                Err(Error::Invalid(_))
            ));
            assert_eq!(s.rules().unwrap(), vec![r]);
            (doctor, run)
        };

        // Rebuilt from the ledger: the same result.
        let s = Store::open_dir(dir.path()).unwrap();
        s.conn
            .execute_batch("DELETE FROM sensitive; DELETE FROM rules; PRAGMA user_version = 3;")
            .unwrap();
        drop(s);
        let mut s = Store::open_dir(dir.path()).unwrap();
        assert_eq!(s.sensitive_nodes().unwrap(), vec![doctor.id]);
        let rule = s.rules().unwrap().remove(0);
        s.remove_rule(rule.id).unwrap();
        assert!(s.sensitive_nodes().unwrap().is_empty());
        assert_eq!(s.sensitivity(run.id).unwrap().by_hand, Some(false));
        assert!(s.verify().unwrap().ok);
    }
}
