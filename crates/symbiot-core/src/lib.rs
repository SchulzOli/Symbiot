//! Symbiot core — the shared kernel.
//!
//! `no_std` + `alloc` so the same logic runs on a microcontroller, a phone,
//! a desktop and the server. Every device writes its own append-only,
//! signed hash chain; state is a projection of all known chains.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod action;
mod bytes;
mod edge;
pub mod evidence;
pub mod identity;
pub mod kind;
mod ledger;
pub mod merkle;
mod node;
mod person;
mod proposal;
pub mod rule;
pub mod source;

pub use bytes::{Blob, EdgeId, Hash, NodeId, PubKey, RuleId, Salt, Sig};
pub use edge::{Edge, EdgeKind};
pub use evidence::{Actor, Evidence, FileRef};
pub use ledger::{
    ContentRef, Decode, Head, LedgerEntry, Op, Part, Signer, VerifyError, VerifyReport,
    build_chain, verify_chain, verify_message,
};
pub use node::{ApplyError, Change, Lookup, Node, NodeKind, Policy, Replay, State, replay};
pub use person::{Device, Person, Trust};
pub use proposal::Proposal;
pub use rule::Rule;

/// Version of the ledger/sync wire format. Bumped on any breaking change,
/// because hashes must be identical on every platform.
/// v2: signed head + removable, salted content (E2, E13).
pub const PROTOCOL_VERSION: u16 = 2;

/// Crate version, so every app can report which kernel it runs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
