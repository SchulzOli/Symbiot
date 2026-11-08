//! Persons (E14, M8, #31): who the raw data is about, across sources.
//!
//! A person is a group of identifiers (`mailto:…`, `tel:…`, `telegram:…`, `name:…`). Rules join
//! what is the same identifier in another spelling ([`symbiot_core::identity::normalize`]). The
//! human joins the rest: [`Store::merge_identities`] writes a decision into the chain,
//! [`Store::split_identity`] and [`Store::keep_apart`] undo or prevent it. Persons are computed
//! from the raw data and those decisions; nothing here is stored that could not be rebuilt.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::params;
use symbiot_core::identity::{Decision, name_key, normalize};
use symbiot_core::{Change, Op};

use crate::{Result, Store, append_in};

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS identity_decision (
        n        INTEGER PRIMARY KEY AUTOINCREMENT,
        entry    BLOB NOT NULL UNIQUE,
        decision BLOB NOT NULL
    );";

/// One identifier of a person, with how often it occurs in the raw data.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Identifier {
    pub id: String,
    /// Names the sources show next to it, most frequent first.
    pub names: Vec<String>,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Person {
    /// The smallest identifier of the group. Changes when groups change; use any identifier
    /// of the person to address it.
    pub id: String,
    pub name: String,
    pub identifiers: Vec<Identifier>,
    /// Items of raw data that name one of the identifiers.
    pub count: usize,
    pub first_ms: u64,
    pub last_ms: u64,
    /// Origins of those items, e.g. `whatsapp`, `mail`.
    pub origins: Vec<String>,
}

/// Two persons that may be one. Only a suggestion: nothing is written until the human merges.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Suggestion {
    pub a: Person,
    pub b: Person,
    pub reason: String,
}

/// What the raw data says about one identifier.
#[derive(Default)]
struct Seen {
    names: HashMap<String, usize>,
    count: usize,
    first_ms: u64,
    last_ms: u64,
    origins: BTreeSet<String>,
}

/// Identifier groups after all decisions, plus the pairs kept apart.
#[derive(Default)]
struct Groups {
    of: HashMap<String, usize>,
    members: Vec<BTreeSet<String>>,
    names: Vec<String>,
    apart: BTreeSet<(String, String)>,
}

impl Groups {
    fn group(&mut self, id: &str) -> usize {
        if let Some(&g) = self.of.get(id) {
            return g;
        }
        self.members.push(BTreeSet::from([id.to_string()]));
        self.names.push(String::new());
        let g = self.members.len() - 1;
        self.of.insert(id.to_string(), g);
        g
    }

    fn take_out(&mut self, id: &str) {
        let g = self.group(id);
        if self.members[g].len() > 1 {
            self.members[g].remove(id);
            self.of.remove(id);
            self.group(id);
        }
    }

    fn apply(&mut self, d: &Decision) {
        match d {
            Decision::Merge { ids, name } => {
                let ids: Vec<String> = ids.iter().map(|i| normalize(i)).collect();
                let target = self.group(&ids[0]);
                for id in &ids[1..] {
                    let g = self.group(id);
                    if g == target {
                        continue;
                    }
                    let moved = std::mem::take(&mut self.members[g]);
                    for m in &moved {
                        self.of.insert(m.clone(), target);
                    }
                    self.members[target].extend(moved);
                    if self.names[target].is_empty() {
                        self.names[target] = std::mem::take(&mut self.names[g]);
                    }
                }
                if !name.trim().is_empty() {
                    self.names[target] = name.trim().to_string();
                }
            }
            Decision::Split { id } => self.take_out(&normalize(id)),
            Decision::KeepApart { a, b } => {
                let (a, b) = (normalize(a), normalize(b));
                if self.group(&a) == self.group(&b) {
                    self.take_out(&b);
                }
                self.apart.insert((a.clone(), b.clone()));
                self.apart.insert((b, a));
            }
        }
    }
}

impl Store {
    fn decisions(&self) -> Result<Vec<Decision>> {
        let mut stmt = self
            .conn
            .prepare("SELECT decision FROM identity_decision ORDER BY n")?;
        let rows = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        rows.map(|b| Ok(postcard::from_bytes(&b?)?)).collect()
    }

    fn seen(&self) -> Result<HashMap<String, Seen>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.actor, a.name, e.at_ms, e.origin FROM evidence_actor AS a
             JOIN evidence AS e ON e.key = a.key
             WHERE e.item IS NOT NULL",
        )?;
        let mut seen: HashMap<String, Seen> = HashMap::new();
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        for row in rows {
            let (actor, name, at, origin) = row?;
            let s = seen.entry(actor).or_default();
            s.count += 1;
            s.first_ms = if s.first_ms == 0 {
                at
            } else {
                s.first_ms.min(at)
            };
            s.last_ms = s.last_ms.max(at);
            if !name.trim().is_empty() {
                *s.names.entry(name.trim().to_string()).or_default() += 1;
            }
            s.origins.extend(origin);
        }
        Ok(seen)
    }

    fn groups(&self, seen: &HashMap<String, Seen>) -> Result<Groups> {
        let mut g = Groups::default();
        let mut ids: Vec<&String> = seen.keys().collect();
        ids.sort();
        for id in ids {
            g.group(id);
        }
        for d in self.decisions()? {
            g.apply(&d);
        }
        Ok(g)
    }

    /// Every person the raw data names, the most frequent first.
    pub fn persons(&self) -> Result<Vec<Person>> {
        let seen = self.seen()?;
        let groups = self.groups(&seen)?;
        let mut out: Vec<Person> = groups
            .members
            .iter()
            .zip(&groups.names)
            .filter(|(m, _)| m.iter().any(|id| seen.contains_key(id)))
            .map(|(m, name)| person(m, name, &seen))
            .collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    /// The person an identifier belongs to.
    pub fn person_of(&self, id: &str) -> Result<Option<Person>> {
        let id = normalize(id);
        Ok(self
            .persons()?
            .into_iter()
            .find(|p| p.identifiers.iter().any(|i| i.id == id)))
    }

    /// Persons that may be one: the same name, or the same first name as a full name, under
    /// different identifiers. Pairs kept apart are not suggested again.
    pub fn person_suggestions(&self) -> Result<Vec<Suggestion>> {
        let seen = self.seen()?;
        let groups = self.groups(&seen)?;
        let persons = self.persons()?;
        let apart = |a: &Person, b: &Person| {
            a.identifiers.iter().any(|x| {
                b.identifiers
                    .iter()
                    .any(|y| groups.apart.contains(&(x.id.clone(), y.id.clone())))
            })
        };
        let mut out = Vec::new();
        for (i, a) in persons.iter().enumerate() {
            for b in &persons[i + 1..] {
                let (ka, kb) = (name_key(&a.name), name_key(&b.name));
                if ka.is_empty() || kb.is_empty() || apart(a, b) {
                    continue;
                }
                let reason = if ka == kb {
                    "gleicher Name"
                } else if first_name_of(&ka, &kb) || first_name_of(&kb, &ka) {
                    "gleicher Vorname"
                } else {
                    continue;
                };
                out.push(Suggestion {
                    a: a.clone(),
                    b: b.clone(),
                    reason: reason.into(),
                });
            }
        }
        // Same names first, then the persons with the most raw data.
        out.sort_by_key(|s| {
            (
                s.reason != "gleicher Name",
                usize::MAX - (s.a.count + s.b.count),
            )
        });
        out.truncate(50);
        Ok(out)
    }

    /// The human decides: these identifiers are one person. `name` is shown if not empty.
    pub fn merge_identities(&mut self, ids: &[String], name: &str) -> Result<()> {
        let ids: Vec<String> = ids.iter().map(|i| normalize(i)).collect();
        self.decide(Decision::Merge {
            ids,
            name: name.trim().into(),
        })
    }

    /// The human decides: this identifier is a person of its own again.
    pub fn split_identity(&mut self, id: &str) -> Result<()> {
        self.decide(Decision::Split { id: normalize(id) })
    }

    /// The human decides: these two are not the same person.
    pub fn keep_apart(&mut self, a: &str, b: &str) -> Result<()> {
        self.decide(Decision::KeepApart {
            a: normalize(a),
            b: normalize(b),
        })
    }

    fn decide(&mut self, decision: Decision) -> Result<()> {
        let tx = self.conn.transaction()?;
        append_in(&tx, &self.signer, Op::DecideIdentity { decision })?;
        tx.commit()?;
        Ok(())
    }
}

/// `short` is one word and the first word of `long`.
fn first_name_of(short: &str, long: &str) -> bool {
    !short.contains(' ')
        && long
            .split_once(' ')
            .is_some_and(|(first, _)| first == short)
}

fn person(members: &BTreeSet<String>, decided: &str, seen: &HashMap<String, Seen>) -> Person {
    let empty = Seen::default();
    let mut identifiers: Vec<Identifier> = members
        .iter()
        .map(|id| {
            let s = seen.get(id).unwrap_or(&empty);
            let mut names: Vec<(&String, &usize)> = s.names.iter().collect();
            names.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            Identifier {
                id: id.clone(),
                names: names.into_iter().map(|(n, _)| n.clone()).collect(),
                count: s.count,
            }
        })
        .collect();
    identifiers.sort_by(|a, b| b.count.cmp(&a.count).then(a.id.cmp(&b.id)));
    let mut name_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for id in members {
        if let Some(s) = seen.get(id) {
            for (n, c) in &s.names {
                *name_counts.entry(n).or_default() += c;
            }
        }
    }
    let name = if !decided.is_empty() {
        decided.to_string()
    } else if let Some((n, _)) = name_counts.iter().max_by_key(|(n, c)| (**c, n.len())) {
        n.to_string()
    } else {
        let first = &identifiers[0].id;
        first
            .split_once(':')
            .map_or(first.as_str(), |(_, v)| v)
            .to_string()
    };
    let stats: Vec<&Seen> = members.iter().filter_map(|id| seen.get(id)).collect();
    Person {
        id: members.iter().next().cloned().unwrap_or_default(),
        name,
        count: stats.iter().map(|s| s.count).sum(),
        first_ms: stats
            .iter()
            .map(|s| s.first_ms)
            .filter(|&t| t > 0)
            .min()
            .unwrap_or(0),
        last_ms: stats.iter().map(|s| s.last_ms).max().unwrap_or(0),
        origins: stats
            .iter()
            .flat_map(|s| s.origins.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        identifiers,
    }
}

/// Writes a `Change::Identity` into the projection.
pub(crate) fn project_identity(tx: &rusqlite::Connection, change: &Change) -> Result<()> {
    if let Change::Identity { entry, decision } = change {
        tx.execute(
            "INSERT OR IGNORE INTO identity_decision (entry, decision) VALUES (?1, ?2)",
            params![entry.0.as_slice(), postcard::to_allocvec(decision)?],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use symbiot_core::Signer;
    use symbiot_core::evidence::{Actor, Evidence, role};

    use super::*;
    use crate::EvidenceQuery;

    fn said(id: &str, at: u64, who: &str, name: &str) -> Evidence {
        Evidence {
            id: id.into(),
            kind: "schema:Message".into(),
            at_ms: at,
            actors: vec![Actor {
                id: who.into(),
                role: symbiot_core::evidence::role::FROM.into(),
                name: name.into(),
            }],
            text: format!("{id} von {name}"),
            fields: vec![],
            files: vec![],
        }
    }

    fn store(seed: u8) -> Store {
        let mut s = Store::open_in_memory(Signer::from_seed([seed; 32])).unwrap();
        s.record_evidence(vec![
            said("whatsapp:c/1", 10, "name:Anna", "Anna"),
            said("whatsapp:c/2", 11, "name:Anna", "Anna"),
            said("telegram:1/1", 20, "telegram:user1", "Anna"),
            said("mail:1", 30, "mailto:Anna@Example.org", "Anna Beispiel"),
            said("mail:2", 31, "mailto:anna@example.org", "Anna Beispiel"),
            said("mail:3", 32, "mailto:ben@example.org", "Ben"),
        ])
        .unwrap();
        s
    }

    fn names(s: &Store) -> Vec<(String, usize, usize)> {
        s.persons()
            .unwrap()
            .into_iter()
            .map(|p| (p.name, p.identifiers.len(), p.count))
            .collect()
    }

    #[test]
    fn rules_join_spellings_and_suggest_the_rest() {
        let s = store(1);
        assert_eq!(
            names(&s),
            [
                ("Anna".into(), 1, 2),
                ("Anna Beispiel".into(), 1, 2),
                ("Anna".into(), 1, 1),
                ("Ben".into(), 1, 1),
            ],
            "the two spellings of the mail address are one identifier by rule"
        );
        let sug = s.person_suggestions().unwrap();
        let pairs: Vec<(String, String, String)> = sug
            .iter()
            .map(|x| (x.a.id.clone(), x.b.id.clone(), x.reason.clone()))
            .collect();
        assert_eq!(pairs[0].2, "gleicher Name");
        assert!(pairs.iter().any(|p| p.2 == "gleicher Vorname"));
        assert!(
            !pairs
                .iter()
                .any(|p| p.0.contains("ben") || p.1.contains("ben"))
        );
    }

    #[test]
    fn merge_split_and_keep_apart() {
        let mut s = store(1);
        s.merge_identities(
            &[
                "name:Anna".into(),
                "telegram:user1".into(),
                "MAILTO:anna@example.org".into(),
            ],
            "Anna B.",
        )
        .unwrap();
        let anna = s.person_of("telegram:user1").unwrap().unwrap();
        assert_eq!(
            (anna.name.as_str(), anna.identifiers.len(), anna.count),
            ("Anna B.", 3, 5)
        );
        assert_eq!(anna.origins, ["mail", "telegram", "whatsapp"]);
        assert_eq!((anna.first_ms, anna.last_ms), (10, 31));

        // „Was habe ich mit wem gemacht“: everything of one person, newest first.
        let ids: Vec<String> = anna.identifiers.iter().map(|i| i.id.clone()).collect();
        let items = s
            .evidence(&EvidenceQuery {
                actors: Some(ids),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(items.len(), 5);
        assert!(
            s.person_suggestions().unwrap().is_empty(),
            "nothing left to join"
        );

        s.split_identity("telegram:user1").unwrap();
        assert_eq!(
            s.person_of("name:Anna").unwrap().unwrap().identifiers.len(),
            2
        );
        assert!(
            !s.person_suggestions().unwrap().is_empty(),
            "suggested again"
        );
        s.keep_apart("telegram:user1", "name:Anna").unwrap();
        assert!(s.person_suggestions().unwrap().is_empty());

        assert!(s.merge_identities(&["name:Anna".into()], "").is_err());
        assert!(s.verify().unwrap().ok);
        assert!(
            s.nodes(None).unwrap().is_empty(),
            "persons are not graph nodes"
        );
    }

    #[test]
    fn decisions_travel_with_the_chain() {
        let words = crate::new_words().unwrap();
        let mut pc = store(2);
        pc.certify_self(&words, "PC").unwrap();
        pc.merge_identities(&["name:Anna".into(), "telegram:user1".into()], "")
            .unwrap();
        let mut home = Store::open_in_memory(Signer::from_seed([3; 32])).unwrap();
        home.certify_self(&words, "Heimserver").unwrap();
        home.receive(pc.entries().unwrap()).unwrap();
        let anna = home.person_of("name:Anna").unwrap().unwrap();
        assert_eq!(anna.identifiers.len(), 2);
        assert_eq!(home.persons().unwrap().len(), pc.persons().unwrap().len());
    }

    #[test]
    fn roles_and_unused_import() {
        // Every role names a person, not only senders.
        let mut s = Store::open_in_memory(Signer::from_seed([4; 32])).unwrap();
        let mut e = said("mail:9", 5, "mailto:a@b.de", "A");
        e.actors.push(Actor {
            id: "mailto:c@d.de".into(),
            role: role::TO.into(),
            name: String::new(),
        });
        s.record_evidence(vec![e]).unwrap();
        let p = s.persons().unwrap();
        assert_eq!(p.len(), 2);
        assert!(p.iter().any(|x| x.name == "c@d.de"), "no name: the address");
    }
}
