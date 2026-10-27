//! A small convention for the `msg` a verifier asks the prover to sign.
//!
//! `quittance:v1|<verifier>|<context>|<nonce, 32 hex>|<expires, unix seconds>`
//!
//! Any byte string is a valid ZIP 311 message; this format only adds what a
//! verifier usually wants checked: who asked, about what, a fresh nonce so an
//! old disclosure can't be replayed, and an expiry.

use serde::{Deserialize, Serialize};

const PREFIX: &str = "quittance:v1|";
const MAX_FIELD: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Challenge {
    pub verifier: String,
    pub context: String,
    pub nonce: [u8; 16],
    pub expires: u64,
}

impl Challenge {
    pub fn new(verifier: &str, context: &str, nonce: [u8; 16], expires: u64) -> Result<Self, String> {
        for (name, v) in [("verifier", verifier), ("context", context)] {
            if v.contains('|') || v.contains('\n') {
                return Err(format!("{name} may not contain `|` or a newline"));
            }
            if v.len() > MAX_FIELD {
                return Err(format!("{name} is longer than {MAX_FIELD} bytes"));
            }
        }
        Ok(Challenge { verifier: verifier.trim().into(), context: context.trim().into(), nonce, expires })
    }

    pub fn to_msg(&self) -> Vec<u8> {
        format!("{PREFIX}{}|{}|{}|{}", self.verifier, self.context, hex::encode(self.nonce), self.expires).into_bytes()
    }

    /// Parses `msg` if it follows the convention; `None` for any other message.
    pub fn parse(msg: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(msg).ok()?;
        let rest = s.strip_prefix(PREFIX)?;
        let parts: Vec<&str> = rest.split('|').collect();
        let [verifier, context, nonce, expires] = parts.as_slice() else { return None };
        let nonce: [u8; 16] = hex::decode(nonce).ok()?.try_into().ok()?;
        Some(Challenge { verifier: verifier.to_string(), context: context.to_string(), nonce, expires: expires.parse().ok()? })
    }

    pub fn is_expired(&self, now_unix: u64) -> bool {
        now_unix > self.expires
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let c = Challenge::new("Acme Exchange", "deposit dispute #8812", [0xab; 16], 1_793_100_000).unwrap();
        let msg = c.to_msg();
        assert_eq!(Challenge::parse(&msg), Some(c.clone()));
        assert!(!c.is_expired(1_793_000_000));
        assert!(c.is_expired(1_793_100_001));
    }

    #[test]
    fn separators_are_refused_and_free_form_messages_are_not_parsed() {
        assert!(Challenge::new("a|b", "x", [0; 16], 0).is_err());
        assert_eq!(Challenge::parse(b"just sign this"), None);
        assert_eq!(Challenge::parse(b"quittance:v1|a|b|nothex|1"), None);
    }
}
