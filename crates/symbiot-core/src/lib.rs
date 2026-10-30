//! Symbiot core — the shared kernel.
//!
//! `no_std` + `alloc` so the same logic runs on a microcontroller, a phone,
//! a desktop and the server. Every device writes its own append-only,
//! signed hash chain; state is a projection of all known chains.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod bytes;
mod ledger;
mod node;

pub use bytes::{Hash, NodeId, PubKey, Sig};
pub use ledger::{LedgerEntry, Op, Signer, VerifyError, VerifyReport, build_chain, verify_chain};
pub use node::{ApplyError, Node, NodeKind, State};

/// Version of the ledger/sync wire format. Bumped on any breaking change,
/// because hashes must be identical on every platform.
pub const PROTOCOL_VERSION: u16 = 1;

/// Crate version, so every app can report which kernel it runs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
