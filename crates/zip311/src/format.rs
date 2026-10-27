//! The disclosure data structure and its canonical encoding.
//!
//! ZIP 311 leaves the encoding open. This crate uses one binary layout, version 1:
//!
//! ```text
//! unsigned part (hashed into the digest):
//!   version            u8   = 1
//!   network            u8   0 = mainnet, 1 = testnet
//!   txid               [u8; 32]   (as serialized, not display order)
//!   height             u32le      height the prover says the transaction was mined at
//!   msg                CompactSize length || bytes   (the verifier's challenge)
//!   reproved spends    CompactSize n || n × (action u32le || cv_net [32] || rk [32] || cmx [32])
//!   resigned spends    CompactSize n || n × (action u32le)
//!   outputs            CompactSize n || n × (action u32le || ock [32])
//! authorization:
//!   reproved sigs      n × [u8; 64]
//!   reproof            CompactSize length || Halo2 proof bytes   (present iff reproved n > 0)
//!   resigned sigs      n × [u8; 64]
//! attachment (not signed; checked against txid by the verifier):
//!   tx                 CompactSize length || raw transaction    (length 0 = not attached)
//! ```
//!
//! The text form is Bech32m (unlimited length) of these bytes under the HRP
//! `zpd` (mainnet) or `zpdtest` (testnet).

use bech32::{primitives::decode::CheckedHrpstring, Hrp};

use crate::encoding::{write_compact_size, Bech32mLong, Reader};
use crate::error::DecodeError;
use crate::network::Network;

pub const VERSION: u8 = 1;
pub const MAX_MSG: usize = 1024;
pub const MAX_ACTIONS: usize = 64;
pub const MAX_PROOF: usize = 1 << 17;
pub const MAX_TX: usize = 2 << 20;

/// One action of the re-proof bundle, tied to the transaction action whose
/// nullifier it re-proves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReprovedAction {
    pub tx_action_index: u32,
    pub cv_net: [u8; 32],
    pub rk: [u8; 32],
    pub cmx: [u8; 32],
    pub spend_auth_sig: [u8; 64],
}

/// Spends re-proven under the transaction's anchor, with one aggregated proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReprovedSpends {
    pub actions: Vec<ReprovedAction>,
    pub proof: Vec<u8>,
}

/// A spend authorized by signing the digest under the transaction's own `rk`.
/// Requires the spend's randomizer `α`, which only a wallet that kept it has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResignedSpend {
    pub tx_action_index: u32,
    pub spend_auth_sig: [u8; 64],
}

/// The outgoing cipher key that opens one output of the transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputDisclosure {
    pub tx_action_index: u32,
    pub ock: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentDisclosure {
    pub network: Network,
    pub txid: [u8; 32],
    pub height: u32,
    pub msg: Vec<u8>,
    pub reproved: Option<ReprovedSpends>,
    pub resigned: Vec<ResignedSpend>,
    pub outputs: Vec<OutputDisclosure>,
    /// The raw transaction, attached so a verifier needn't look it up by txid.
    pub tx: Option<Vec<u8>>,
}

impl PaymentDisclosure {
    /// The bytes the digest commits to.
    pub fn unsigned_bytes(&self) -> Vec<u8> {
        let reproved: Vec<(u32, [u8; 32], [u8; 32], [u8; 32])> = self
            .reproved
            .iter()
            .flat_map(|r| r.actions.iter().map(|a| (a.tx_action_index, a.cv_net, a.rk, a.cmx)))
            .collect();
        let resigned: Vec<u32> = self.resigned.iter().map(|r| r.tx_action_index).collect();
        let outputs: Vec<(u32, [u8; 32])> = self.outputs.iter().map(|o| (o.tx_action_index, o.ock)).collect();
        unsigned_bytes(self.network, &self.txid, self.height, &self.msg, &reproved, &resigned, &outputs)
    }

    /// The ZIP 311 digest: `BLAKE2b-256("ZIP311Signed" || coinType, unsigned bytes)`.
    pub fn digest(&self) -> [u8; 32] {
        digest(self.network, &self.unsigned_bytes())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.unsigned_bytes();
        if let Some(r) = &self.reproved {
            for a in &r.actions {
                out.extend_from_slice(&a.spend_auth_sig);
            }
            write_compact_size(&mut out, r.proof.len() as u64);
            out.extend_from_slice(&r.proof);
        }
        for s in &self.resigned {
            out.extend_from_slice(&s.spend_auth_sig);
        }
        match &self.tx {
            Some(tx) => {
                write_compact_size(&mut out, tx.len() as u64);
                out.extend_from_slice(tx);
            }
            None => write_compact_size(&mut out, 0),
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let version = r.u8()?;
        if version != VERSION {
            return Err(DecodeError::Version(version));
        }
        let nb = r.u8()?;
        let network = Network::from_byte(nb).ok_or(DecodeError::Network(nb))?;
        let txid = r.array::<32>()?;
        let height = r.u32()?;
        let msg = r.bytes("challenge", MAX_MSG)?;

        let n = r.count("reproved spends", MAX_ACTIONS)?;
        let mut reproved_parts = Vec::with_capacity(n);
        for _ in 0..n {
            reproved_parts.push((r.u32()?, r.array::<32>()?, r.array::<32>()?, r.array::<32>()?));
        }
        let n = r.count("resigned spends", MAX_ACTIONS)?;
        let mut resigned_idx = Vec::with_capacity(n);
        for _ in 0..n {
            resigned_idx.push(r.u32()?);
        }
        let n = r.count("outputs", MAX_ACTIONS)?;
        let mut outputs = Vec::with_capacity(n);
        for _ in 0..n {
            outputs.push(OutputDisclosure { tx_action_index: r.u32()?, ock: r.array()? });
        }

        let reproved = if reproved_parts.is_empty() {
            None
        } else {
            let mut actions = Vec::with_capacity(reproved_parts.len());
            for (tx_action_index, cv_net, rk, cmx) in reproved_parts {
                actions.push(ReprovedAction { tx_action_index, cv_net, rk, cmx, spend_auth_sig: r.array()? });
            }
            let proof = r.bytes("proof", MAX_PROOF)?;
            Some(ReprovedSpends { actions, proof })
        };
        let mut resigned = Vec::with_capacity(resigned_idx.len());
        for tx_action_index in resigned_idx {
            resigned.push(ResignedSpend { tx_action_index, spend_auth_sig: r.array()? });
        }
        let tx = r.bytes("transaction", MAX_TX)?;
        if r.remaining() != 0 {
            return Err(DecodeError::Trailing(r.remaining()));
        }
        Ok(PaymentDisclosure {
            network,
            txid,
            height,
            msg,
            reproved,
            resigned,
            outputs,
            tx: if tx.is_empty() { None } else { Some(tx) },
        })
    }

    /// Bech32m text form (`zpd1…` / `zpdtest1…`).
    pub fn encode(&self) -> String {
        bech32::encode::<Bech32mLong>(Hrp::parse_unchecked(self.network.hrp()), &self.to_bytes())
            .expect("length is within the extended Bech32m limit")
    }

    pub fn decode(text: &str) -> Result<Self, DecodeError> {
        let text = text.trim();
        let parsed = CheckedHrpstring::new::<Bech32mLong>(text).map_err(|e| DecodeError::Text(e.to_string()))?;
        let hrp = parsed.hrp().to_lowercase();
        let network = Network::from_hrp(&hrp).ok_or_else(|| DecodeError::Hrp(hrp.clone()))?;
        let bytes: Vec<u8> = parsed.byte_iter().collect();
        let d = Self::from_bytes(&bytes)?;
        if d.network != network {
            return Err(DecodeError::NetworkMismatch { text: network.to_string(), inner: d.network.to_string() });
        }
        Ok(d)
    }

    /// The txid in the byte order block explorers display.
    pub fn txid_display(&self) -> String {
        let mut b = self.txid;
        b.reverse();
        hex::encode(b)
    }
}

pub(crate) fn unsigned_bytes(
    network: Network,
    txid: &[u8; 32],
    height: u32,
    msg: &[u8],
    reproved: &[(u32, [u8; 32], [u8; 32], [u8; 32])],
    resigned: &[u32],
    outputs: &[(u32, [u8; 32])],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + msg.len() + reproved.len() * 100 + outputs.len() * 36);
    out.push(VERSION);
    out.push(network.to_byte());
    out.extend_from_slice(txid);
    out.extend_from_slice(&height.to_le_bytes());
    write_compact_size(&mut out, msg.len() as u64);
    out.extend_from_slice(msg);
    write_compact_size(&mut out, reproved.len() as u64);
    for (i, cv, rk, cmx) in reproved {
        out.extend_from_slice(&i.to_le_bytes());
        out.extend_from_slice(cv);
        out.extend_from_slice(rk);
        out.extend_from_slice(cmx);
    }
    write_compact_size(&mut out, resigned.len() as u64);
    for i in resigned {
        out.extend_from_slice(&i.to_le_bytes());
    }
    write_compact_size(&mut out, outputs.len() as u64);
    for (i, ock) in outputs {
        out.extend_from_slice(&i.to_le_bytes());
        out.extend_from_slice(ock);
    }
    out
}

pub(crate) fn digest(network: Network, unsigned: &[u8]) -> [u8; 32] {
    let mut personal = [0u8; 16];
    personal[..12].copy_from_slice(b"ZIP311Signed");
    personal[12..].copy_from_slice(&network.coin_type().to_le_bytes());
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(&personal)
        .hash(unsigned)
        .as_bytes()
        .try_into()
        .expect("32-byte hash")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PaymentDisclosure {
        PaymentDisclosure {
            network: Network::Test,
            txid: [7; 32],
            height: 4_500_000,
            msg: b"quittance:v1|acme|ticket 8812|00112233445566778899aabbccddeeff|1793100000".to_vec(),
            reproved: Some(ReprovedSpends {
                actions: vec![ReprovedAction { tx_action_index: 0, cv_net: [1; 32], rk: [2; 32], cmx: [3; 32], spend_auth_sig: [4; 64] }],
                proof: vec![9; 4992],
            }),
            resigned: vec![],
            outputs: vec![OutputDisclosure { tx_action_index: 1, ock: [5; 32] }],
            tx: None,
        }
    }

    #[test]
    fn bytes_round_trip() {
        let d = sample();
        assert_eq!(PaymentDisclosure::from_bytes(&d.to_bytes()).unwrap(), d);
        let mut with_tx = d.clone();
        with_tx.tx = Some(vec![0xab; 3000]);
        with_tx.resigned.push(ResignedSpend { tx_action_index: 2, spend_auth_sig: [8; 64] });
        assert_eq!(PaymentDisclosure::from_bytes(&with_tx.to_bytes()).unwrap(), with_tx);
    }

    #[test]
    fn text_round_trip_and_prefix() {
        let d = sample();
        let s = d.encode();
        assert!(s.starts_with("zpdtest1"));
        assert_eq!(PaymentDisclosure::decode(&s).unwrap(), d);
        assert_eq!(PaymentDisclosure::decode(&s.to_uppercase()).unwrap(), d);
    }

    #[test]
    fn a_typo_is_caught_by_the_checksum() {
        let s = sample().encode();
        let mut chars: Vec<char> = s.chars().collect();
        let i = chars.len() / 2;
        chars[i] = if chars[i] == 'q' { 'p' } else { 'q' };
        let typo: String = chars.into_iter().collect();
        assert!(matches!(PaymentDisclosure::decode(&typo), Err(DecodeError::Text(_))));
    }

    #[test]
    fn signatures_and_attachment_are_outside_the_digest() {
        let d = sample();
        let mut other = d.clone();
        other.reproved.as_mut().unwrap().actions[0].spend_auth_sig = [0; 64];
        other.reproved.as_mut().unwrap().proof = vec![1; 10];
        other.tx = Some(vec![1, 2, 3]);
        assert_eq!(d.digest(), other.digest());
    }

    #[test]
    fn every_signed_field_moves_the_digest() {
        let d = sample();
        let base = d.digest();
        let mut variants: Vec<PaymentDisclosure> = vec![];
        let mut v = d.clone(); v.network = Network::Main; variants.push(v);
        let mut v = d.clone(); v.txid[0] ^= 1; variants.push(v);
        let mut v = d.clone(); v.height += 1; variants.push(v);
        let mut v = d.clone(); v.msg.push(b'!'); variants.push(v);
        let mut v = d.clone(); v.reproved.as_mut().unwrap().actions[0].tx_action_index = 1; variants.push(v);
        let mut v = d.clone(); v.reproved.as_mut().unwrap().actions[0].cv_net[0] ^= 1; variants.push(v);
        let mut v = d.clone(); v.reproved.as_mut().unwrap().actions[0].rk[0] ^= 1; variants.push(v);
        let mut v = d.clone(); v.reproved.as_mut().unwrap().actions[0].cmx[0] ^= 1; variants.push(v);
        let mut v = d.clone(); v.outputs[0].ock[0] ^= 1; variants.push(v);
        let mut v = d.clone(); v.outputs[0].tx_action_index = 0; variants.push(v);
        let mut v = d.clone(); v.resigned.push(ResignedSpend { tx_action_index: 3, spend_auth_sig: [0; 64] }); variants.push(v);
        for v in variants {
            assert_ne!(v.digest(), base);
        }
    }

    #[test]
    fn the_digest_is_domain_separated_by_network() {
        assert_ne!(digest(Network::Main, b"x"), digest(Network::Test, b"x"));
    }

    #[test]
    fn oversized_and_trailing_input_is_rejected() {
        let mut bytes = sample().to_bytes();
        bytes.push(0);
        assert_eq!(PaymentDisclosure::from_bytes(&bytes), Err(DecodeError::Trailing(1)));
        let mut long = sample();
        long.msg = vec![b'a'; MAX_MSG + 1];
        assert_eq!(PaymentDisclosure::from_bytes(&long.to_bytes()), Err(DecodeError::TooLarge("challenge", MAX_MSG)));
        assert_eq!(PaymentDisclosure::from_bytes(&[2]), Err(DecodeError::Version(2)));
    }
}
