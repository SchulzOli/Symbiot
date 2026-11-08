//! Persons from raw data (E14, M8, #31).
//!
//! The raw data names people as its sources do: `mailto:a@b.de`, `tel:+49…`, `telegram:user1`,
//! `name:Anna`. Which of these identifiers belong to the same person is decided in two ways:
//!
//! - **Rules** (here, the same on every device): an identifier is normalised (`mailto:` in lower
//!   case, `tel:` with country code), and a few forms are known to be one person
//!   (GitHub's no-reply address is the GitHub account).
//! - **The human**: everything else. Suggestions are made outside the chain; only the human's
//!   decision is signed — [`Decision::Merge`], [`Decision::Split`], [`Decision::KeepApart`].
//!   Each can be undone by the next decision, nothing is overwritten.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// A human decision about identifiers. The identifiers are the content of the entry, so they
/// can be removed (E2) like any other personal data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// These identifiers are one person, optionally with the name to show.
    Merge { ids: Vec<String>, name: String },
    /// This identifier is a person of its own again.
    Split { id: String },
    /// These two identifiers are not the same person: do not suggest them again.
    KeepApart { a: String, b: String },
}

/// Why a decision is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    /// A merge needs two or more different identifiers; keeping apart needs two different ones.
    TooFew,
    /// An identifier without scheme (`scheme:value`).
    Identifier,
}

impl Decision {
    pub fn check(&self) -> Result<(), Invalid> {
        let ok = |id: &String| prefixed(id);
        match self {
            Decision::Merge { ids, .. } => {
                if !ids.iter().all(ok) {
                    return Err(Invalid::Identifier);
                }
                let mut unique: Vec<&String> = ids.iter().collect();
                unique.sort();
                unique.dedup();
                if unique.len() < 2 {
                    return Err(Invalid::TooFew);
                }
            }
            Decision::Split { id } if !ok(id) => return Err(Invalid::Identifier),
            Decision::Split { .. } => {}
            Decision::KeepApart { a, b } => {
                if !ok(a) || !ok(b) {
                    return Err(Invalid::Identifier);
                }
                if normalize(a) == normalize(b) {
                    return Err(Invalid::TooFew);
                }
            }
        }
        Ok(())
    }
}

fn prefixed(s: &str) -> bool {
    matches!(s.split_once(':'), Some((a, b)) if !a.is_empty() && !b.is_empty())
}

/// The canonical form of an identifier: what counts as „the same identifier“ by rule.
pub fn normalize(id: &str) -> String {
    let Some((scheme, value)) = id.split_once(':') else {
        return id.trim().to_string();
    };
    let scheme = scheme.trim().to_ascii_lowercase();
    let value = value.trim();
    match scheme.as_str() {
        "mailto" => {
            let addr = value.to_lowercase();
            // GitHub's no-reply address is the account itself.
            if let Some(user) = addr
                .strip_suffix("@users.noreply.github.com")
                .map(|l| l.split_once('+').map_or(l, |(_, u)| u))
            {
                return alloc::format!("github:{user}");
            }
            alloc::format!("mailto:{addr}")
        }
        "tel" => {
            let digits: String = value.chars().filter(char::is_ascii_digit).collect();
            let full = if value.trim_start().starts_with('+') {
                digits
            } else if let Some(rest) = digits.strip_prefix("00") {
                rest.into()
            } else {
                // A national number: without its country it stays as it is.
                return alloc::format!("tel:{digits}");
            };
            alloc::format!("tel:+{full}")
        }
        "github" | "telegram" | "steam" | "whatsapp" => {
            alloc::format!("{scheme}:{}", value.to_lowercase())
        }
        "name" => {
            let words: Vec<&str> = value.split_whitespace().collect();
            alloc::format!("name:{}", words.join(" "))
        }
        _ => alloc::format!("{scheme}:{value}"),
    }
}

/// A name for comparing: lower case, without accents and punctuation, single spaces.
pub fn name_key(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            'ß' => 's',
            'á' | 'à' | 'â' => 'a',
            'é' | 'è' | 'ê' => 'e',
            'í' | 'ì' | 'î' => 'i',
            'ó' | 'ò' | 'ô' => 'o',
            'ú' | 'ù' | 'û' => 'u',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(c);
    }
    let words: Vec<&str> = out.split_whitespace().collect();
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_for_identifiers() {
        assert_eq!(
            normalize("mailto:Anna@Example.org"),
            "mailto:anna@example.org"
        );
        assert_eq!(
            normalize("MAILTO: anna@example.org "),
            "mailto:anna@example.org"
        );
        assert_eq!(normalize("tel:+49 151 2345-678"), "tel:+491512345678");
        assert_eq!(normalize("tel:0049 151 2345678"), "tel:+491512345678");
        assert_eq!(normalize("tel:0151 2345678"), "tel:01512345678");
        assert_eq!(
            normalize("mailto:12345+Octo@users.noreply.github.com"),
            "github:octo"
        );
        assert_eq!(normalize("github:Octo"), "github:octo");
        assert_eq!(normalize("name:  Anna   Beispiel "), "name:Anna Beispiel");
        assert_eq!(normalize("ohne-schema"), "ohne-schema");
    }

    #[test]
    fn names_compare_without_case_and_accents() {
        assert_eq!(name_key("  Jürgen-Müller "), "jurgen muller");
        assert_eq!(name_key("ANNA beispiel"), name_key("Anna Beispiel"));
    }

    #[test]
    fn decisions_are_checked() {
        let merge = |ids: &[&str]| Decision::Merge {
            ids: ids.iter().map(|s| s.to_string()).collect(),
            name: String::new(),
        };
        assert_eq!(merge(&["name:Anna", "mailto:a@b.de"]).check(), Ok(()));
        assert_eq!(merge(&["name:Anna"]).check(), Err(Invalid::TooFew));
        assert_eq!(
            merge(&["name:Anna", "name:Anna"]).check(),
            Err(Invalid::TooFew)
        );
        assert_eq!(
            merge(&["Anna", "name:Ben"]).check(),
            Err(Invalid::Identifier)
        );
        let apart = Decision::KeepApart {
            a: "mailto:A@b.de".into(),
            b: "mailto:a@b.de".into(),
        };
        assert_eq!(apart.check(), Err(Invalid::TooFew));
    }

    #[test]
    fn decision_in_the_chain() {
        use crate::ledger::{Op, Signer, verify_chain};
        use crate::node::{ApplyError, Change, State};
        let s = Signer::from_seed([4; 32]);
        let merge = Decision::Merge {
            ids: alloc::vec!["name:Anna".into(), "mailto:anna@example.org".into()],
            name: "Anna".into(),
        };
        let e = s.append(
            None,
            5,
            Op::DecideIdentity {
                decision: merge.clone(),
            },
        );
        assert_eq!(
            e.op(),
            Ok(Op::DecideIdentity {
                decision: merge.clone()
            })
        );
        let mut st = State::default();
        assert!(matches!(st.apply(&e), Ok(Change::Identity { .. })));
        assert_eq!(st.identity, [(e.hash, merge)]);
        assert!(st.nodes.is_empty());

        // E2: without its content the decision says nothing any more; the chain stays valid.
        let mut pruned = e.clone();
        pruned.content = None;
        assert_eq!(verify_chain(core::slice::from_ref(&pruned)), Ok(1));
        assert!(State::default().apply(&pruned).is_err());

        let bad = s.append(
            None,
            5,
            Op::DecideIdentity {
                decision: Decision::Split { id: "ohne".into() },
            },
        );
        assert_eq!(
            State::default().apply(&bad).err(),
            Some(ApplyError::BadIdentity)
        );
    }
}
