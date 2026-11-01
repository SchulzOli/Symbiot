//! The person behind the devices (E5).
//!
//! A person has one key, shown to the human as recovery words. It confirms every device
//! („this device is mine“) and can revoke a lost one. The person key is never stored on a
//! device: it is derived from the words when needed and forgotten afterwards. Which devices
//! belong to a person follows deterministically from the chains alone.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bytes::{PubKey, Sig};
use crate::ledger::{LedgerEntry, Op};

const PERSON_DOMAIN: &[u8] = b"symbiot/person/v1";
const CERT_DOMAIN: &[u8] = b"symbiot/device-cert/v1";
const REVOKE_DOMAIN: &[u8] = b"symbiot/device-revoke/v1";

/// The person key, derived from 32 bytes of entropy (the recovery words).
pub struct Person {
    key: SigningKey,
}

impl Person {
    pub fn from_entropy(entropy: [u8; 32]) -> Self {
        let mut h = Sha256::new();
        h.update(PERSON_DOMAIN);
        h.update(entropy);
        Person {
            key: SigningKey::from_bytes(&h.finalize().into()),
        }
    }

    pub fn public(&self) -> PubKey {
        PubKey(self.key.verifying_key().to_bytes())
    }

    /// „This device is mine.“ The entry goes into the device's own chain.
    pub fn certify(&self, device: PubKey, name: &str) -> Op {
        let sig = Sig(self.key.sign(&cert_message(&device, name)).to_bytes());
        Op::CertifyDevice {
            device,
            name: name.into(),
            person: self.public(),
            sig,
        }
    }

    /// A lost or stolen device: its entries after `last_seq` are no longer accepted.
    pub fn revoke(&self, device: PubKey, last_seq: u64) -> Op {
        let sig = Sig(self.key.sign(&revoke_message(&device, last_seq)).to_bytes());
        Op::RevokeDevice {
            device,
            last_seq,
            person: self.public(),
            sig,
        }
    }
}

fn cert_message(device: &PubKey, name: &str) -> Vec<u8> {
    let mut m = Vec::from(CERT_DOMAIN);
    m.extend_from_slice(&device.0);
    m.extend_from_slice(name.as_bytes());
    m
}

fn revoke_message(device: &PubKey, last_seq: u64) -> Vec<u8> {
    let mut m = Vec::from(REVOKE_DOMAIN);
    m.extend_from_slice(&device.0);
    m.extend_from_slice(&last_seq.to_le_bytes());
    m
}

fn signed_by(person: &PubKey, message: &[u8], sig: &Sig) -> bool {
    VerifyingKey::from_bytes(&person.0)
        .and_then(|k| k.verify_strict(message, &Signature::from_bytes(&sig.0)))
        .is_ok()
}

/// A device of the person, as the chains say.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub key: PubKey,
    pub name: String,
    /// Entries after this `seq` are not accepted (the device was revoked).
    pub revoked_after: Option<u64>,
}

/// Which devices belong to one person. Computed from the chains alone, the same on every device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trust {
    pub person: Option<PubKey>,
    pub devices: BTreeMap<PubKey, Device>,
}

impl Trust {
    /// A device counts once its own chain holds a valid certificate of `person`. Revocations
    /// signed by `person` may stand in any chain; the earliest cut-off wins.
    pub fn of<'a>(person: PubKey, chains: impl IntoIterator<Item = &'a [LedgerEntry]>) -> Self {
        let mut devices: BTreeMap<PubKey, Device> = BTreeMap::new();
        let mut revoked: BTreeMap<PubKey, u64> = BTreeMap::new();
        for chain in chains {
            for e in chain {
                match e.op() {
                    Ok(Op::CertifyDevice {
                        device,
                        name,
                        person: p,
                        sig,
                    }) if p == person
                        && device == e.head.author
                        && signed_by(&p, &cert_message(&device, &name), &sig) =>
                    {
                        devices.entry(device).or_insert(Device {
                            key: device,
                            name,
                            revoked_after: None,
                        });
                    }
                    Ok(Op::RevokeDevice {
                        device,
                        last_seq,
                        person: p,
                        sig,
                    }) if p == person
                        && signed_by(&p, &revoke_message(&device, last_seq), &sig) =>
                    {
                        let cut = revoked.entry(device).or_insert(last_seq);
                        *cut = (*cut).min(last_seq);
                    }
                    _ => {}
                }
            }
        }
        for (device, cut) in revoked {
            if let Some(d) = devices.get_mut(&device) {
                d.revoked_after = Some(cut);
            }
        }
        Trust {
            person: Some(person),
            devices,
        }
    }

    /// Whether the entry `seq` of `author`'s chain counts.
    pub fn accepts(&self, author: &PubKey, seq: u64) -> bool {
        self.devices
            .get(author)
            .is_some_and(|d| d.revoked_after.is_none_or(|cut| seq <= cut))
    }

    /// The person a chain is certified for, if any — how a device finds out who it belongs to.
    pub fn person_of(chain: &[LedgerEntry]) -> Option<PubKey> {
        chain.iter().find_map(|e| match e.op() {
            Ok(Op::CertifyDevice {
                device,
                name,
                person,
                sig,
            }) if device == e.head.author
                && signed_by(&person, &cert_message(&device, &name), &sig) =>
            {
                Some(person)
            }
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{Signer, build_chain};
    use crate::node::NodeKind;

    fn create(title: &str) -> Op {
        Op::CreateNode {
            kind: NodeKind::Goal,
            title: title.into(),
            body: String::new(),
        }
    }

    #[test]
    fn a_certified_device_is_trusted() {
        let me = Person::from_entropy([1; 32]);
        let pc = Signer::from_seed([2; 32]);
        let chain = build_chain(
            &pc,
            [(1, create("Ziel")), (2, me.certify(pc.public(), "PC"))],
        );
        assert_eq!(Trust::person_of(&chain), Some(me.public()));
        let t = Trust::of(me.public(), [chain.as_slice()]);
        assert!(
            t.accepts(&pc.public(), 0),
            "entries before the certificate count too"
        );
        assert_eq!(t.devices[&pc.public()].name, "PC");
    }

    #[test]
    fn certificates_of_others_do_not_count() {
        let me = Person::from_entropy([1; 32]);
        let other = Person::from_entropy([9; 32]);
        let pc = Signer::from_seed([2; 32]);
        let phone = Signer::from_seed([3; 32]);

        // Signed by someone else.
        let foreign = build_chain(&pc, [(1, other.certify(pc.public(), "PC"))]);
        // A certificate for a different device, placed in this chain.
        let misplaced = build_chain(&phone, [(1, me.certify(pc.public(), "PC"))]);
        // A forged signature.
        let mut forged_op = me.certify(phone.public(), "Handy");
        if let Op::CertifyDevice { name, .. } = &mut forged_op {
            *name = "Fremd".into();
        }
        let forged = build_chain(&phone, [(1, forged_op)]);

        for chain in [&foreign, &misplaced, &forged] {
            let t = Trust::of(me.public(), [chain.as_slice()]);
            assert!(t.devices.is_empty(), "{chain:?}");
        }
        assert_eq!(Trust::person_of(&forged), None);
    }

    #[test]
    fn revocation_cuts_off_later_entries() {
        let me = Person::from_entropy([1; 32]);
        let pc = Signer::from_seed([2; 32]);
        let lost = Signer::from_seed([3; 32]);
        let lost_chain = build_chain(
            &lost,
            [
                (1, me.certify(lost.public(), "Handy")),
                (2, create("vor dem Verlust")),
                (3, create("vom Dieb")),
            ],
        );
        let pc_chain = build_chain(
            &pc,
            [
                (1, me.certify(pc.public(), "PC")),
                (4, me.revoke(lost.public(), 1)),
            ],
        );
        let t = Trust::of(me.public(), [lost_chain.as_slice(), pc_chain.as_slice()]);
        assert!(t.accepts(&lost.public(), 1));
        assert!(!t.accepts(&lost.public(), 2));
        assert!(t.accepts(&pc.public(), 1));
        assert!(!t.accepts(&Signer::from_seed([4; 32]).public(), 0));
    }
}
