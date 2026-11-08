//! Fixed ids for entry kinds. They go into the signed head, so they never change and are
//! never reused. Unlike an enum index, a new kind can be added anywhere, and a device that
//! does not know a kind can still verify the entry and pass it on.

pub const CREATE_NODE: u16 = 1;
pub const UPDATE_NODE: u16 = 2;
pub const LINK: u16 = 3;
pub const ACCEPT_PROPOSAL: u16 = 4;
pub const REJECT_PROPOSAL: u16 = 5;
pub const MARK_SENSITIVE: u16 = 6;
pub const DELETE_NODE: u16 = 7;
pub const UNLINK: u16 = 8;
pub const SET_POLICY: u16 = 9;
pub const ADD_SENSITIVITY_RULE: u16 = 10;
pub const REMOVE_SENSITIVITY_RULE: u16 = 11;

// Reserved for what comes with adapters, sync and sensors (E5, E7, E13).
pub const SET_RETENTION: u16 = 16;
pub const GRANT: u16 = 17;
pub const REVOKE: u16 = 18;
pub const CERTIFY_DEVICE: u16 = 19;
pub const REVOKE_DEVICE: u16 = 20;
pub const CHECKPOINT: u16 = 21;
pub const WITNESS: u16 = 22;
pub const PRUNED: u16 = 23;
pub const EVIDENCE_BATCH: u16 = 24;
pub const SENSOR_BATCH: u16 = 25;
pub const ROLLUP: u16 = 26;

// E14, M8: the human's decisions about which identifiers are one person (#31).
pub const DECIDE_IDENTITY: u16 = 27;

// E16: the human's decision on a suggestion of the AI (#42).
pub const DECIDE_SUGGESTION: u16 = 28;
