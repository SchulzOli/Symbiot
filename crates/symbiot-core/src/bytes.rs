//! Fixed-size byte newtypes. Hex strings in JSON, raw bytes in postcard —
//! so hashes are computed over compact, deterministic bytes everywhere.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! byte_newtype {
    ($(#[$doc:meta])* $name:ident, $len:expr) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub [u8; $len]);

        impl $name {
            pub const LEN: usize = $len;

            pub fn to_hex(&self) -> String {
                hex::encode(self.0)
            }

            pub fn from_hex(s: &str) -> Option<Self> {
                let mut out = [0u8; $len];
                hex::decode_to_slice(s, &mut out).ok()?;
                Some(Self(out))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.to_hex())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.to_hex())
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                if s.is_human_readable() {
                    s.serialize_str(&self.to_hex())
                } else {
                    s.serialize_bytes(&self.0)
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = $name;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        write!(f, "{} bytes or {} hex chars", $len, $len * 2)
                    }

                    fn visit_str<E: de::Error>(self, v: &str) -> Result<$name, E> {
                        $name::from_hex(v).ok_or_else(|| E::invalid_value(de::Unexpected::Str(v), &self))
                    }

                    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<$name, E> {
                        let arr: [u8; $len] =
                            v.try_into().map_err(|_| E::invalid_length(v.len(), &self))?;
                        Ok($name(arr))
                    }
                }
                if d.is_human_readable() {
                    d.deserialize_str(V)
                } else {
                    d.deserialize_bytes(V)
                }
            }
        }
    };
}

byte_newtype!(
    /// SHA-256 of an entry's signed head (or of content, for Merkle roots).
    Hash, 32
);
byte_newtype!(
    /// Ed25519 public key — the identity of a device (PC, phone, sensor, server).
    PubKey, 32
);
byte_newtype!(
    /// Ed25519 signature over an entry hash.
    Sig, 64
);
byte_newtype!(
    /// Stable node id: the first 16 bytes of the hash of the entry that created it.
    NodeId, 16
);

byte_newtype!(
    /// Stable edge id: the first 16 bytes of the hash of the entry that created it.
    EdgeId, 16
);

byte_newtype!(
    /// Stable rule id (E4): the first 16 bytes of the hash of the entry that added it.
    RuleId, 16
);

byte_newtype!(
    /// Per-part salt: without it, short removed content could be confirmed by guessing.
    Salt, 16
);

/// Variable-length bytes. Hex in JSON, raw bytes in postcard — like the fixed-size types.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Blob(pub Vec<u8>);

impl fmt::Debug for Blob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Blob({})", hex::encode(&self.0))
    }
}

impl Serialize for Blob {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.serialize_str(&hex::encode(&self.0))
        } else {
            s.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Blob {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Blob;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bytes or hex chars")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Blob, E> {
                hex::decode(v)
                    .map(Blob)
                    .map_err(|_| E::invalid_value(de::Unexpected::Str(v), &self))
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Blob, E> {
                Ok(Blob(v.to_vec()))
            }
        }
        if d.is_human_readable() {
            d.deserialize_str(V)
        } else {
            d.deserialize_bytes(V)
        }
    }
}

impl Hash {
    /// `prev` of the first entry in every chain.
    pub const ZERO: Hash = Hash([0; 32]);

    fn prefix16(&self) -> [u8; 16] {
        let mut id = [0u8; 16];
        id.copy_from_slice(&self.0[..16]);
        id
    }
}

impl NodeId {
    pub fn from_entry_hash(h: &Hash) -> Self {
        NodeId(h.prefix16())
    }
}

impl EdgeId {
    pub fn from_entry_hash(h: &Hash) -> Self {
        EdgeId(h.prefix16())
    }
}

impl RuleId {
    pub fn from_entry_hash(h: &Hash) -> Self {
        RuleId(h.prefix16())
    }
}
