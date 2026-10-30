//! Fixed-size byte newtypes. Hex strings in JSON, raw bytes in postcard —
//! so hashes are computed over compact, deterministic bytes everywhere.

use alloc::string::String;
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
    /// SHA-256 of an entry's signed content.
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

impl Hash {
    /// `prev` of the first entry in every chain.
    pub const ZERO: Hash = Hash([0; 32]);
}

impl NodeId {
    pub fn from_entry_hash(h: &Hash) -> Self {
        let mut id = [0u8; 16];
        id.copy_from_slice(&h.0[..16]);
        NodeId(id)
    }
}
