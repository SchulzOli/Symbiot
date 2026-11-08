//! #44: which raw data is used. The person decides per source (the origin of the raw data,
//! e.g. `whatsapp`, `mail`, `git`): use it or pause it, leave out single chats or repositories,
//! only the last N days — or revoke it, which removes its raw data on every device (E2).
//!
//! These are decisions in the chain like every other, so every device of the person applies
//! the same ones. Adapters keep loading; what is not used is simply not shown, searched or
//! given to the AI.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// How a source may be used. The default is „everything“: a source without a decision is used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// `false`: paused — kept, but not used.
    pub active: bool,
    /// Chats, repositories … of this source that are left out (the item's `chat` or `repo`).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Only the last N days are used; 0 means all.
    #[serde(default)]
    pub keep_days: u32,
}

impl Default for Grant {
    fn default() -> Self {
        Grant {
            active: true,
            exclude: Vec::new(),
            keep_days: 0,
        }
    }
}

/// The longest name of a source, a chat or a repository in a decision.
pub const MAX_NAME: usize = 200;
/// The most chats or repositories one decision leaves out.
pub const MAX_EXCLUDE: usize = 1000;

/// A source name is the origin of the raw data: lower-case letters, digits, `-` and `_`.
pub fn valid_source(source: &str) -> bool {
    !source.is_empty()
        && source.len() <= MAX_NAME
        && source
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

impl Grant {
    pub fn valid(&self) -> bool {
        self.exclude.len() <= MAX_EXCLUDE
            && self
                .exclude
                .iter()
                .all(|s| !s.trim().is_empty() && s.len() <= MAX_NAME)
    }
}

/// The context of an item within its source, which [`Grant::exclude`] refers to.
pub fn scope_of(e: &crate::Evidence) -> Option<&str> {
    ["chat", "repo"]
        .iter()
        .find_map(|k| e.fields.iter().find(|(f, _)| f == k))
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_source("whatsapp"));
        assert!(valid_source("my-mail_2"));
        assert!(!valid_source(""));
        assert!(!valid_source("Mail"));
        assert!(!valid_source("a:b"));
        let g = Grant {
            exclude: alloc::vec![" ".into()],
            ..Grant::default()
        };
        assert!(!g.valid());
        assert!(Grant::default().valid());
    }
}
