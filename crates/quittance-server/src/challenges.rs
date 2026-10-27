//! "Ask for proof": a verifier opens a challenge, the payer answers it with a
//! disclosure, and the verified result is kept for the verifier to read.
//!
//! Tickets live in memory and are written to one JSON file so a restart keeps
//! them. They hold the challenge and the verdict, never a viewing key.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use rand::{rand_core::UnwrapErr, rngs::SysRng, Rng};
use serde::{Deserialize, Serialize};
use zip311::{Challenge, Network};

pub const MAX_TICKETS: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub id: String,
    pub network: Network,
    pub msg: String,
    pub verifier: String,
    pub context: String,
    pub created: u64,
    pub expires: u64,
    pub answered: Option<u64>,
    /// The disclosure the payer sent, so the verifier can keep it as evidence.
    pub disclosure: Option<String>,
    pub verdict: Option<serde_json::Value>,
}

pub struct Store {
    tickets: Mutex<HashMap<String, Ticket>>,
    path: Option<PathBuf>,
}

impl Store {
    pub fn open(path: Option<PathBuf>) -> Self {
        let tickets = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Vec<Ticket>>(&b).ok())
            .map(|v| v.into_iter().map(|t| (t.id.clone(), t)).collect())
            .unwrap_or_default();
        Store { tickets: Mutex::new(tickets), path }
    }

    fn persist(&self, tickets: &HashMap<String, Ticket>) {
        if let Some(p) = &self.path {
            let list: Vec<&Ticket> = tickets.values().collect();
            if let Ok(bytes) = serde_json::to_vec(&list) {
                let tmp = p.with_extension("tmp");
                if std::fs::write(&tmp, bytes).is_ok() {
                    let _ = std::fs::rename(&tmp, p);
                }
            }
        }
    }

    pub fn create(&self, network: Network, verifier: &str, context: &str, ttl: u64, now: u64) -> Result<Ticket, String> {
        let mut rng = UnwrapErr(SysRng);
        let mut nonce = [0u8; 16];
        rng.fill_bytes(&mut nonce);
        let challenge = Challenge::new(verifier, context, nonce, now + ttl)?;
        let msg = String::from_utf8(challenge.to_msg()).expect("challenge text is UTF-8");
        let mut id_bytes = [0u8; 9];
        rng.fill_bytes(&mut id_bytes);
        let id = id_bytes.iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect::<String>();
        let ticket = Ticket {
            id: id.clone(),
            network,
            msg,
            verifier: challenge.verifier,
            context: challenge.context,
            created: now,
            expires: now + ttl,
            answered: None,
            disclosure: None,
            verdict: None,
        };
        let mut tickets = self.tickets.lock().unwrap();
        tickets.retain(|_, t| t.expires + 7 * 24 * 3600 > now);
        if tickets.len() >= MAX_TICKETS {
            return Err("too many open challenges; try again later".into());
        }
        tickets.insert(id, ticket.clone());
        self.persist(&tickets);
        Ok(ticket)
    }

    pub fn get(&self, id: &str) -> Option<Ticket> {
        self.tickets.lock().unwrap().get(id).cloned()
    }

    /// Records an answer. Once a ticket holds a valid answer it keeps it; until
    /// then the latest answer is shown, so the payer can see why one failed.
    pub fn answer(&self, id: &str, disclosure: String, verdict: serde_json::Value, now: u64) -> Option<Ticket> {
        let mut tickets = self.tickets.lock().unwrap();
        let t = tickets.get_mut(id)?;
        let already_valid = t.verdict.as_ref().and_then(|v| v.get("valid")).and_then(|v| v.as_bool()).unwrap_or(false);
        if !already_valid {
            t.answered = Some(now);
            t.disclosure = Some(disclosure);
            t.verdict = Some(verdict);
        }
        let out = t.clone();
        self.persist(&tickets);
        Some(out)
    }
}

/// Unambiguous lowercase letters and digits.
const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_keeps_its_first_valid_answer() {
        let s = Store::open(None);
        let t = s.create(Network::Test, "Acme", "ticket 1", 3600, 1000).unwrap();
        assert!(t.msg.starts_with("quittance:v1|Acme|ticket 1|"));
        let bad = serde_json::json!({ "valid": false });
        let good = serde_json::json!({ "valid": true });
        assert!(s.answer(&t.id, "zpdtest1bad".into(), bad.clone(), 1001).unwrap().verdict.unwrap()["valid"] == false);
        assert!(s.answer(&t.id, "zpdtest1good".into(), good, 1002).unwrap().verdict.unwrap()["valid"] == true);
        let after = s.answer(&t.id, "zpdtest1bad2".into(), bad, 1003).unwrap();
        assert_eq!(after.disclosure.as_deref(), Some("zpdtest1good"));
    }

    #[test]
    fn separators_in_the_context_are_refused() {
        let s = Store::open(None);
        assert!(s.create(Network::Main, "Acme", "a|b", 60, 0).is_err());
    }
}
