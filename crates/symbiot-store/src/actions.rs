//! E16: carrying out actions — the same for a click of the human and an accepted suggestion
//! of the AI.

use std::collections::HashMap;

use symbiot_core::action::{Action, Suggestion, Target};
use symbiot_core::{NodeId, Op, Policy};

use crate::{Error, Result, Store};

/// One action that was carried out.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Executed {
    /// Its index in the suggestion.
    pub index: usize,
    /// The node it created, if it created one.
    pub node: Option<NodeId>,
}

/// What became of an accepted suggestion: the actions carried out, and the error that
/// stopped the rest, if any.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Outcome {
    pub executed: Vec<Executed>,
    pub error: Option<String>,
}

impl Store {
    /// Carries out one action. `new` maps `$n` to the nodes created by earlier actions.
    pub fn execute(
        &mut self,
        action: &Action,
        new: &HashMap<u16, NodeId>,
    ) -> Result<Option<NodeId>> {
        let node = |t: &Target| -> Result<NodeId> {
            match t {
                Target::Node(id) => Ok(*id),
                Target::New(n) => new.get(n).copied().ok_or(Error::Invalid(
                    "Verweis auf einen Knoten, der nicht angelegt wurde",
                )),
            }
        };
        match action {
            Action::CreateNode { kind, title, body } => {
                return Ok(Some(self.create_node(*kind, title, body)?.id));
            }
            Action::UpdateNode {
                node: t,
                title,
                body,
            } => {
                self.update_node(node(t)?, title, body)?;
            }
            Action::Link { from, to, kind } => {
                self.link(node(from)?, node(to)?, *kind)?;
            }
            Action::Unlink { edge } => self.unlink(*edge)?,
            Action::DeleteNode { node: t } => {
                self.delete_node(node(t)?)?;
            }
            Action::SetSensitive { node: t, sensitive } => {
                self.set_sensitive(node(t)?, *sensitive)?
            }
            Action::SetPolicy {
                node: t,
                deletable,
                exportable,
            } => self.set_policy(
                node(t)?,
                Policy {
                    deletable: *deletable,
                    exportable: *exportable,
                },
            )?,
            Action::AddRule { label, keywords } => {
                self.add_rule(label, keywords, None)?;
            }
            Action::RemoveRule { rule } => self.remove_rule(*rule)?,
            Action::MergeIdentities { ids, name } => self.merge_identities(ids, name)?,
            Action::SplitIdentity { id } => self.split_identity(id)?,
            Action::KeepApart { a, b } => self.keep_apart(a, b)?,
        }
        Ok(None)
    }

    /// The human decides on a suggestion: rejected, or accepted with the actions in `chosen`
    /// (indexes, in order). The chosen actions are carried out one by one; if one fails, the
    /// rest stay undone and the error is reported. The decision is signed last, naming the
    /// actions that were carried out, so the chain says exactly what happened.
    pub fn decide_suggestion(
        &mut self,
        suggestion: Suggestion,
        accept: bool,
        chosen: &[usize],
    ) -> Result<Outcome> {
        suggestion
            .check()
            .map_err(|e| Error::Suggestion(e.to_string()))?;
        let mut out = Outcome::default();
        if accept {
            if chosen.is_empty() {
                return Err(Error::Suggestion("Keine Aktion ausgewählt".into()));
            }
            if chosen.iter().any(|&i| i >= suggestion.actions.len()) {
                return Err(Error::Suggestion("Diese Aktion gibt es nicht".into()));
            }
            // A chosen action that builds on a new node needs the action that creates it.
            for &i in chosen {
                for t in suggestion.actions[i].targets() {
                    if let Target::New(n) = t
                        && !chosen.contains(&(n as usize))
                    {
                        return Err(Error::Suggestion(format!(
                            "Aktion {} braucht Aktion {}",
                            i + 1,
                            n + 1
                        )));
                    }
                }
            }
            let mut order = chosen.to_vec();
            order.sort_unstable();
            order.dedup();
            let mut new: HashMap<u16, NodeId> = HashMap::new();
            for i in order {
                match self.execute(&suggestion.actions[i], &new) {
                    Ok(node) => {
                        if let Some(id) = node {
                            new.insert(i as u16, id);
                        }
                        out.executed.push(Executed { index: i, node });
                    }
                    Err(e) => {
                        out.error = Some(format!("Aktion {}: {e}", i + 1));
                        break;
                    }
                }
            }
        }
        let accepted = accept && !out.executed.is_empty();
        self.append(Op::DecideSuggestion {
            accepted,
            chosen: out.executed.iter().map(|e| e.index as u32).collect(),
            suggestion,
        })?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use symbiot_core::{EdgeKind, NodeKind, Signer};

    use super::*;

    fn store() -> Store {
        Store::open_in_memory(Signer::from_seed([9; 32])).unwrap()
    }

    fn suggestion(actions: Vec<Action>) -> Suggestion {
        Suggestion {
            title: "Schritt".into(),
            reason: "Ein Schritt zum Ziel".into(),
            actions,
            sources: vec![],
            model: "mock".into(),
        }
    }

    #[test]
    fn accepted_actions_run_and_the_decision_is_signed() {
        let mut s = store();
        let goal = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        let sug = suggestion(vec![
            Action::CreateNode {
                kind: NodeKind::Goal,
                title: "Laufschuhe kaufen".into(),
                body: String::new(),
            },
            Action::Link {
                from: Target::New(0),
                to: Target::Node(goal.id),
                kind: EdgeKind::DerivesFrom,
            },
            Action::SetSensitive {
                node: Target::Node(goal.id),
                sensitive: true,
            },
        ]);
        let out = s.decide_suggestion(sug, true, &[0, 1]).unwrap();
        assert_eq!(out.error, None);
        assert_eq!(out.executed.len(), 2);
        let shoes = out.executed[0].node.unwrap();
        assert_eq!(s.nodes(None).unwrap().len(), 2);
        assert!(
            s.edges()
                .unwrap()
                .iter()
                .any(|e| e.from == shoes && e.to == goal.id)
        );
        assert!(
            s.sensitive_nodes().unwrap().is_empty(),
            "action 3 was not chosen"
        );

        let last = s.entries().unwrap().pop().unwrap();
        let Ok(Op::DecideSuggestion {
            accepted,
            chosen,
            suggestion,
        }) = last.op()
        else {
            panic!("the decision comes last");
        };
        assert!(accepted);
        assert_eq!(chosen, [0, 1]);
        assert_eq!(suggestion.model, "mock");
        assert!(s.verify().unwrap().ok);
    }

    #[test]
    fn rejected_writes_only_the_decision() {
        let mut s = store();
        let before = s.entries().unwrap().len();
        let sug = suggestion(vec![Action::CreateNode {
            kind: NodeKind::Goal,
            title: "X".into(),
            body: String::new(),
        }]);
        let out = s.decide_suggestion(sug, false, &[]).unwrap();
        assert!(out.executed.is_empty());
        assert!(s.nodes(None).unwrap().is_empty());
        assert_eq!(s.entries().unwrap().len(), before + 1);
    }

    #[test]
    fn stops_at_the_first_failure_and_says_so() {
        let mut s = store();
        let ghost = NodeId([1; NodeId::LEN]);
        let sug = suggestion(vec![
            Action::CreateNode {
                kind: NodeKind::Value,
                title: "Ruhe".into(),
                body: String::new(),
            },
            Action::DeleteNode {
                node: Target::Node(ghost),
            },
            Action::CreateNode {
                kind: NodeKind::Value,
                title: "Nie".into(),
                body: String::new(),
            },
        ]);
        let out = s.decide_suggestion(sug, true, &[0, 1, 2]).unwrap();
        assert_eq!(out.executed.len(), 1);
        assert!(out.error.unwrap().starts_with("Aktion 2:"));
        let Ok(Op::DecideSuggestion { chosen, .. }) = s.entries().unwrap().pop().unwrap().op()
        else {
            panic!()
        };
        assert_eq!(chosen, [0], "the chain names only what happened");
    }

    #[test]
    fn a_reference_needs_its_creator() {
        let mut s = store();
        let goal = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        let sug = suggestion(vec![
            Action::CreateNode {
                kind: NodeKind::Goal,
                title: "Neu".into(),
                body: String::new(),
            },
            Action::Link {
                from: Target::New(0),
                to: Target::Node(goal.id),
                kind: EdgeKind::Supports,
            },
        ]);
        let err = s.decide_suggestion(sug, true, &[1]).unwrap_err();
        assert!(
            err.to_string().contains("Aktion 2 braucht Aktion 1"),
            "{err}"
        );
        assert_eq!(s.nodes(None).unwrap().len(), 1, "nothing was done");
    }
}
