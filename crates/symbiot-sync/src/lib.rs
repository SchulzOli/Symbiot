//! Symbiot sync (M4, F5): devices exchange chains with heimserver.
//!
//! Every request is signed with the device key, so heimserver knows which device asks. Entries carry
//! their own signatures; heimserver takes in a chain only from a device of its own person (E5), and so
//! does every device when it pulls. Syncing twice changes nothing.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use symbiot_core::{LedgerEntry, PubKey, Sig, verify_message};
use symbiot_store::{ChainHead, Store};

pub const HEADER_DEVICE: &str = "x-symbiot-device";
pub const HEADER_TIME: &str = "x-symbiot-time";
pub const HEADER_SIG: &str = "x-symbiot-sig";
/// How far the time of a request may be off the server's clock.
pub const MAX_SKEW_MS: u64 = 5 * 60 * 1000;

/// What a device signs for one request: method, path with query, time and the body's hash.
pub fn request_message(method: &str, path: &str, ts_ms: u64, body: &[u8]) -> Vec<u8> {
    format!(
        "symbiot/sync-auth/v1\n{method}\n{path}\n{ts_ms}\n{}",
        hex::encode(Sha256::digest(body))
    )
    .into_bytes()
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("Anmeldung fehlt")]
    Missing,
    #[error("Die Uhrzeit des Geräts weicht mehr als 5 Minuten ab")]
    Skew,
    #[error("Signatur der Anfrage ungültig")]
    BadSignature,
}

/// Server side: which device signed this request.
pub fn verify_request(
    device: Option<&str>,
    ts_ms: Option<&str>,
    sig: Option<&str>,
    method: &str,
    path: &str,
    body: &[u8],
    now_ms: u64,
) -> Result<PubKey, AuthError> {
    let (Some(device), Some(ts), Some(sig)) = (device, ts_ms, sig) else {
        return Err(AuthError::Missing);
    };
    let device = PubKey::from_hex(device).ok_or(AuthError::Missing)?;
    let sig = Sig::from_hex(sig).ok_or(AuthError::Missing)?;
    let ts: u64 = ts.parse().map_err(|_| AuthError::Missing)?;
    if ts.abs_diff(now_ms) > MAX_SKEW_MS {
        return Err(AuthError::Skew);
    }
    if !verify_message(&device, &request_message(method, path, ts, body), &sig) {
        return Err(AuthError::BadSignature);
    }
    Ok(device)
}

/// Answer to a push.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pushed {
    pub received: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("heimserver nicht erreichbar: {0}")]
    Http(String),
    /// heimserver answered with an error, e.g. „Gerät gehört nicht zu diesem Personen-Schlüssel“.
    #[error("heimserver: {0}")]
    Remote(String),
    #[error(transparent)]
    Store(#[from] symbiot_store::Error),
}

/// What one round of syncing moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub sent: usize,
    pub received: usize,
}

/// heimserver, as seen from a device.
pub struct Remote {
    base: String,
    agent: ureq::Agent,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock(store: &Mutex<Store>) -> MutexGuard<'_, Store> {
    store.lock().unwrap_or_else(|p| p.into_inner())
}

impl Remote {
    /// `base` like `https://heimserver.example.ts.net:8449`.
    pub fn new(base: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        Remote {
            base: base.trim_end_matches('/').into(),
            agent,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// One signed request; the store is locked only to sign.
    fn call(
        &self,
        store: &Mutex<Store>,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, Error> {
        let ts = now_ms();
        let bytes = body.unwrap_or_default();
        let (device, sig) = {
            let s = lock(store);
            let sig = s.sign_message(&request_message(method, path, ts, &bytes));
            (s.device(), sig)
        };
        let url = format!("{}{path}", self.base);
        let headers = [
            (HEADER_DEVICE, device.to_hex()),
            (HEADER_TIME, ts.to_string()),
            (HEADER_SIG, sig.to_hex()),
        ];
        let result = if method == "POST" {
            let mut req = self
                .agent
                .post(&url)
                .header("content-type", "application/json");
            for (k, v) in &headers {
                req = req.header(*k, v);
            }
            req.send(&bytes[..])
        } else {
            let mut req = self.agent.get(&url);
            for (k, v) in &headers {
                req = req.header(*k, v);
            }
            req.call()
        };
        let mut res = result.map_err(|e| Error::Http(e.to_string()))?;
        let status = res.status();
        let body = res
            .body_mut()
            .with_config()
            .limit(256 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| Error::Http(e.to_string()))?;
        if status.is_success() {
            return Ok(body);
        }
        #[derive(Deserialize)]
        struct ErrBody {
            error: String,
        }
        Err(Error::Remote(
            serde_json::from_slice::<ErrBody>(&body)
                .map(|b| b.error)
                .unwrap_or_else(|_| format!("Fehler {status}")),
        ))
    }

    fn json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
        serde_json::from_slice(bytes).map_err(|e| Error::Http(format!("unlesbare Antwort: {e}")))
    }

    pub fn heads(&self, store: &Mutex<Store>) -> Result<Vec<ChainHead>, Error> {
        Self::json(&self.call(store, "GET", "/api/sync/heads", None)?)
    }

    pub fn pull(
        &self,
        store: &Mutex<Store>,
        author: &PubKey,
        after: Option<u64>,
    ) -> Result<Vec<LedgerEntry>, Error> {
        let path = match after {
            Some(a) => format!("/api/sync/chains/{author}?after={a}"),
            None => format!("/api/sync/chains/{author}"),
        };
        Self::json(&self.call(store, "GET", &path, None)?)
    }

    pub fn push(&self, store: &Mutex<Store>, entries: &[LedgerEntry]) -> Result<Pushed, Error> {
        let Some(first) = entries.first() else {
            return Ok(Pushed { received: 0 });
        };
        let path = format!("/api/sync/chains/{}", first.head.author);
        let body = serde_json::to_vec(entries).map_err(|e| Error::Http(e.to_string()))?;
        Self::json(&self.call(store, "POST", &path, Some(body))?)
    }
}

/// One round: send what heimserver does not have of this device's chain, then fetch every chain heimserver
/// knows further than this device. The own chain goes first, so a newly set up device
/// introduces itself with its certificate before it asks for anything.
pub fn sync(store: &Mutex<Store>, remote: &Remote) -> Result<Report, Error> {
    let me = lock(store).device();
    let mut report = Report::default();

    let theirs = remote.heads(store)?;
    let known_there = theirs.iter().find(|h| h.author == me).map(|h| h.seq);
    let mine = lock(store).chain_since(&me, known_there)?;
    if !mine.is_empty() {
        report.sent = remote.push(store, &mine)?.received;
    }

    for head in theirs.iter().filter(|h| h.author != me) {
        let here = lock(store).tip(&head.author)?.map(|t| t.seq);
        if here.is_some_and(|s| s >= head.seq) {
            continue;
        }
        let entries = remote.pull(store, &head.author, here)?;
        report.received += lock(store).receive(entries)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use symbiot_core::Signer;

    #[test]
    fn requests_are_bound_to_method_path_time_and_body() {
        let s = Signer::from_seed([1; 32]);
        let msg = request_message("POST", "/api/sync/chains/ab", 1_000, b"[]");
        let sig = s.sign_message(&msg).to_hex();
        let dev = s.public().to_hex();
        let check = |method, path, ts: &str, body: &[u8], now| {
            verify_request(Some(&dev), Some(ts), Some(&sig), method, path, body, now)
        };
        assert_eq!(
            check("POST", "/api/sync/chains/ab", "1000", b"[]", 2_000),
            Ok(s.public())
        );
        let bad = Err(AuthError::BadSignature);
        assert_eq!(
            check("GET", "/api/sync/chains/ab", "1000", b"[]", 2_000),
            bad
        );
        assert_eq!(check("POST", "/api/sync/heads", "1000", b"[]", 2_000), bad);
        assert_eq!(
            check("POST", "/api/sync/chains/ab", "1001", b"[]", 2_000),
            bad
        );
        assert_eq!(
            check("POST", "/api/sync/chains/ab", "1000", b"[1]", 2_000),
            bad
        );
        assert_eq!(
            check(
                "POST",
                "/api/sync/chains/ab",
                "1000",
                b"[]",
                1_000 + MAX_SKEW_MS + 1
            ),
            Err(AuthError::Skew)
        );
        assert_eq!(
            verify_request(None, None, None, "GET", "/", b"", 0),
            Err(AuthError::Missing)
        );
    }
}
