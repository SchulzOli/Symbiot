//! Merkle tree hash as in RFC 9162 (Certificate Transparency v2).
//!
//! An entry's content is a list of parts; the signed head carries the root over them.
//! Parts can then be removed one by one and the rest still proves against the root.

use sha2::{Digest, Sha256};

use crate::bytes::{Hash, Salt};

/// `sha256(0x00 ‖ salt ‖ data)` — the leaf of one content part.
pub fn leaf(salt: &Salt, data: &[u8]) -> Hash {
    let mut h = Sha256::new();
    h.update([0x00]);
    h.update(salt.0);
    h.update(data);
    Hash(h.finalize().into())
}

/// `MTH` over leaf hashes: empty → `sha256("")`, one leaf → the leaf,
/// otherwise split at the largest power of two below `n`.
pub fn root(leaves: &[Hash]) -> Hash {
    match leaves {
        [] => Hash(Sha256::digest([]).into()),
        [one] => *one,
        _ => {
            let k = split(leaves.len());
            let mut h = Sha256::new();
            h.update([0x01]);
            h.update(root(&leaves[..k]).0);
            h.update(root(&leaves[k..]).0);
            Hash(h.finalize().into())
        }
    }
}

/// Largest power of two strictly below `n` (n ≥ 2).
fn split(n: usize) -> usize {
    1 << (usize::BITS - 1 - (n - 1).leading_zeros())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_leaf(data: &[u8]) -> Hash {
        let mut h = Sha256::new();
        h.update([0x00]);
        h.update(data);
        Hash(h.finalize().into())
    }

    #[test]
    fn split_is_largest_power_of_two_below() {
        assert_eq!(split(2), 1);
        assert_eq!(split(3), 2);
        assert_eq!(split(4), 2);
        assert_eq!(split(5), 4);
        assert_eq!(split(8), 4);
        assert_eq!(split(9), 8);
    }

    /// The test vectors of RFC 6962 / RFC 9162 implementations (Certificate Transparency).
    #[test]
    fn matches_certificate_transparency_vectors() {
        let data: [&[u8]; 8] = [
            b"",
            b"\x00",
            b"\x10",
            b"\x20\x21",
            b"\x30\x31",
            b"\x40\x41\x42\x43",
            b"\x50\x51\x52\x53\x54\x55\x56\x57",
            b"\x60\x61\x62\x63\x64\x65\x66\x67\x68\x69\x6a\x6b\x6c\x6d\x6e\x6f",
        ];
        let roots = [
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
            "fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
            "aeb6bcfe274b70a14fb067a5e5578264db0fa9b51af5e0ba159158f329e06e77",
            "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "4e3bbb1f7b478dcfe71fb631631519a3bca12c9aefca1612bfce4c13a86264d4",
            "76e67dadbcdf1e10e1b74ddc608abd2f98dfb16fbce75277b5232a127f2087ef",
            "ddb89be403809e325750d3d263cd78929c2942b7942a34b77e122c9594a74c8c",
            "5dc9da79a70659a9ad559cb701ded9a2ab9d823aad2f4960cfe370eff4604328",
        ];
        let leaves: alloc::vec::Vec<Hash> = data.iter().map(|d| plain_leaf(d)).collect();
        for (n, expected) in roots.iter().enumerate() {
            assert_eq!(root(&leaves[..=n]).to_hex(), *expected, "{} leaves", n + 1);
        }
        assert_eq!(
            root(&[]).to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
