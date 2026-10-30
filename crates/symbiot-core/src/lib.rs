//! Symbiot core — the shared kernel.
//!
//! `no_std` + `alloc` so the same logic runs on a microcontroller, a phone,
//! a desktop and the server. Every device writes its own append-only,
//! signed hash chain; state is a projection of all known chains.
#![cfg_attr(not(test), no_std)]

/// Version of the ledger/sync wire format. Bumped on any breaking change,
/// because hashes must be identical on every platform.
pub const PROTOCOL_VERSION: u16 = 1;

/// Crate version, so every app can report which kernel it runs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_starts_at_one() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }
}

pub const   CI_RED_TEST:u8=1;
