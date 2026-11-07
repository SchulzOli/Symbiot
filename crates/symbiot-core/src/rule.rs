//! What is sensitive (E4).
//!
//! A rule is a name and a few keywords, e.g. „Gesundheit“: `arzt`, `befund`, `diagnose`. The AI
//! may suggest one; it becomes a rule only when the human accepts it, as an entry in the chain.
//! From then on it is applied deterministically, the same on every device: a node whose title
//! or text contains one of the keywords is sensitive. Marking by hand always wins over rules.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::bytes::{PubKey, RuleId};
use crate::node::Node;

/// Keywords shorter than this would match far too much.
pub const MIN_KEYWORD: usize = 3;
/// More keywords than this make a rule hard to read and check.
pub const MAX_KEYWORDS: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: RuleId,
    /// What the rule is about, e.g. „Gesundheit“.
    pub label: String,
    /// Lower case, at least [`MIN_KEYWORD`] characters each.
    pub keywords: Vec<String>,
    /// The model that suggested it, if it came from the AI.
    pub model: Option<String>,
    pub author: PubKey,
    pub created_ms: u64,
}

impl Rule {
    pub fn matches(&self, node: &Node) -> bool {
        matches_text(&self.keywords, &node.title, &node.body)
    }
}

/// Whether title or body contains one of the keywords (case does not matter; parts of
/// words count, so „arzt“ also finds „Arzttermin“).
pub fn matches_text(keywords: &[String], title: &str, body: &str) -> bool {
    let mut text = title.to_lowercase();
    text.push('\n');
    text.push_str(&body.to_lowercase());
    keywords.iter().any(|k| text.contains(k.as_str()))
}

/// Lower case, trimmed, long enough, no duplicates, at most [`MAX_KEYWORDS`].
pub fn clean_keywords<S: AsRef<str>>(raw: impl IntoIterator<Item = S>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in raw {
        let k = k.as_ref().trim().to_lowercase();
        if k.chars().count() >= MIN_KEYWORD && !out.contains(&k) && out.len() < MAX_KEYWORDS {
            out.push(k.to_string());
        }
    }
    out
}

/// The effective sensitivity of a node: marking by hand wins, otherwise any rule.
pub fn is_sensitive<'a>(
    node: &Node,
    by_hand: Option<bool>,
    rules: impl IntoIterator<Item = &'a Rule>,
) -> bool {
    by_hand.unwrap_or_else(|| rules.into_iter().any(|r| r.matches(node)))
}
