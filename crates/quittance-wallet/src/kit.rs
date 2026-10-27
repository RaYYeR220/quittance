//! Disclosure kits.
//!
//! Re-proving a spend for a ZIP 311 disclosure takes, for every note the
//! transaction spent, the note itself and its Merkle path *at the
//! transaction's own anchor*. A wallet holds both while it builds the
//! transaction and normally lets them go: the note is marked spent and the
//! tree checkpoint at that anchor is pruned within a hundred blocks or so.
//! A kit keeps them, one JSON file per transaction, so a disclosure can be
//! made at any later time.
//!
//! A kit is not a key. It reveals the spent notes (values, the wallet's own
//! receiving address) to whoever reads it, but it cannot spend or sign: the
//! spend authorizing key still comes from the mnemonic at proving time.

use std::path::{Path, PathBuf};

use incrementalmerkletree::Hashable;
use orchard::{
    note::{ExtractedNoteCommitment, NoteVersion, RandomSeed, Rho},
    tree::{MerkleHashOrchard, MerklePath},
    value::NoteValue,
    Address, Anchor, Note,
};
use serde::{Deserialize, Serialize};
use zcash_protocol::TxId;
use zip311::{Network, SpendWitness};

use crate::error::{Error, Result};

const KIT_VERSION: u32 = 1;
const DEPTH: usize = orchard::NOTE_COMMITMENT_TREE_DEPTH;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisclosureKit {
    pub version: u32,
    pub network: Network,
    /// In the usual display order (byte-reversed hex).
    pub txid: String,
    /// The transaction's Ironwood anchor: the note commitment tree root its spends were proven against.
    pub anchor: String,
    /// The block whose final Ironwood tree has that root.
    pub anchor_height: u32,
    pub notes: Vec<KitNote>,
}

/// One spent note, enough to rebuild it and its Merkle path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KitNote {
    /// The raw 43-byte Orchard address the note was received at.
    pub recipient: String,
    pub value: u64,
    pub rho: String,
    pub rseed: String,
    /// Note plaintext version: 2 for ZIP 212 (Orchard), 3 for ZIP 2005 (Ironwood).
    pub note_version: u8,
    /// The note commitment, kept so a damaged kit is caught on load.
    pub cmx: String,
    pub position: u32,
    /// The 32 sibling hashes from the leaf to the root.
    pub auth_path: Vec<String>,
}

impl DisclosureKit {
    /// Builds a kit, refusing any witness whose path does not lead to `anchor`.
    pub fn new(network: Network, txid: TxId, anchor: Anchor, anchor_height: u32, witnesses: &[SpendWitness]) -> Result<Self> {
        if witnesses.is_empty() {
            return Err(Error::Kit("no spent notes to record".into()));
        }
        let notes = witnesses
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let cmx = ExtractedNoteCommitment::from(w.note.commitment());
                if w.merkle_path.root(cmx) != anchor {
                    return Err(Error::Kit(format!("the Merkle path for spent note #{i} does not lead to the transaction's anchor")));
                }
                Ok(KitNote::from_witness(w, cmx))
            })
            .collect::<Result<_>>()?;
        Ok(DisclosureKit {
            version: KIT_VERSION,
            network,
            txid: txid.to_string(),
            anchor: hex::encode(anchor.to_bytes()),
            anchor_height,
            notes,
        })
    }

    pub fn txid(&self) -> Result<TxId> {
        parse_txid(&self.txid).map_err(Error::Kit)
    }

    pub fn anchor(&self) -> Result<Anchor> {
        let bytes = bytes32(&self.anchor, "anchor")?;
        Option::from(Anchor::from_bytes(bytes)).ok_or_else(|| Error::Kit("anchor is not a valid tree root".into()))
    }

    /// Rebuilds the notes and paths, checking each against the recorded
    /// commitment and anchor.
    pub fn witnesses(&self) -> Result<Vec<SpendWitness>> {
        if self.version != KIT_VERSION {
            return Err(Error::Kit(format!("unsupported kit version {}", self.version)));
        }
        let anchor = self.anchor()?;
        self.notes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let w = n.to_witness().map_err(|e| Error::Kit(format!("note #{i}: {e}")))?;
                let cmx = ExtractedNoteCommitment::from(w.note.commitment());
                if hex::encode(cmx.to_bytes()) != n.cmx {
                    return Err(Error::Kit(format!("note #{i} does not match its recorded commitment")));
                }
                if w.merkle_path.root(cmx) != anchor {
                    return Err(Error::Kit(format!("the Merkle path of note #{i} does not lead to the recorded anchor")));
                }
                Ok(w)
            })
            .collect()
    }

    pub fn save(&self, wallet_dir: &Path) -> Result<PathBuf> {
        let dir = crate::config::kits_dir(wallet_dir);
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let path = kit_path(wallet_dir, &self.txid);
        let text = serde_json::to_string_pretty(self).expect("kit serializes") + "\n";
        std::fs::write(&path, text).map_err(|e| Error::io(&path, e))?;
        Ok(path)
    }

    pub fn load(wallet_dir: &Path, txid: TxId) -> Result<Self> {
        let path = kit_path(wallet_dir, &txid.to_string());
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::NoKit { txid: txid.to_string(), dir: wallet_dir.to_owned() })
            }
            Err(e) => return Err(Error::io(&path, e)),
        };
        let kit: DisclosureKit = serde_json::from_str(&text).map_err(|e| Error::Kit(format!("{}: {e}", path.display())))?;
        if kit.txid != txid.to_string() {
            return Err(Error::Kit(format!("{} is for transaction {}", path.display(), kit.txid)));
        }
        Ok(kit)
    }
}

pub(crate) fn kit_path(wallet_dir: &Path, txid: &str) -> PathBuf {
    crate::config::kits_dir(wallet_dir).join(format!("{txid}.json"))
}

impl KitNote {
    fn from_witness(w: &SpendWitness, cmx: ExtractedNoteCommitment) -> Self {
        let n = &w.note;
        KitNote {
            recipient: hex::encode(n.recipient().to_raw_address_bytes()),
            value: n.value().inner(),
            rho: hex::encode(n.rho().to_bytes()),
            rseed: hex::encode(n.rseed().as_bytes()),
            note_version: match n.version() {
                NoteVersion::V2 => 2,
                NoteVersion::V3 => 3,
            },
            cmx: hex::encode(cmx.to_bytes()),
            position: w.merkle_path.position(),
            auth_path: w.merkle_path.auth_path().iter().map(|h| hex::encode(h.to_bytes())).collect(),
        }
    }

    fn to_witness(&self) -> Result<SpendWitness> {
        let raw: [u8; 43] = hex::decode(&self.recipient)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| Error::Kit("recipient is not 43 bytes of hex".into()))?;
        let recipient = Option::from(Address::from_raw_address_bytes(&raw)).ok_or_else(|| Error::Kit("recipient is not an Orchard address".into()))?;
        let rho = Option::from(Rho::from_bytes(&bytes32(&self.rho, "rho")?)).ok_or_else(|| Error::Kit("rho is not a field element".into()))?;
        let rseed = Option::from(RandomSeed::from_bytes(bytes32(&self.rseed, "rseed")?, &rho)).ok_or_else(|| Error::Kit("rseed is invalid for this rho".into()))?;
        let version = match self.note_version {
            2 => NoteVersion::V2,
            3 => NoteVersion::V3,
            v => return Err(Error::Kit(format!("unknown note version {v}"))),
        };
        let note: Note = Option::from(Note::from_parts(recipient, NoteValue::from_raw(self.value), rho, rseed, version))
            .ok_or_else(|| Error::Kit("the note parts do not form a valid note".into()))?;

        if self.auth_path.len() != DEPTH {
            return Err(Error::Kit(format!("auth path has {} hashes, expected {DEPTH}", self.auth_path.len())));
        }
        let mut path = [MerkleHashOrchard::empty_leaf(); DEPTH];
        for (slot, h) in path.iter_mut().zip(&self.auth_path) {
            *slot = Option::from(MerkleHashOrchard::from_bytes(&bytes32(h, "auth path")?))
                .ok_or_else(|| Error::Kit("auth path holds a non-canonical hash".into()))?;
        }
        Ok(SpendWitness { note, merkle_path: MerklePath::from_parts(self.position, path) })
    }
}

fn bytes32(s: &str, what: &str) -> Result<[u8; 32]> {
    hex::decode(s)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Error::Kit(format!("{what} is not 32 bytes of hex")))
}

/// Parses a txid in display order.
pub fn parse_txid(s: &str) -> std::result::Result<TxId, String> {
    let mut bytes: [u8; 32] = hex::decode(s.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| format!("`{}` is not a txid (64 hex characters)", s.trim()))?;
    bytes.reverse();
    Ok(TxId::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use incrementalmerkletree::{frontier::CommitmentTree, witness::IncrementalWitness, Level};
    use orchard::keys::{FullViewingKey, Scope, SpendingKey};

    use super::*;

    /// A tree holding two of our notes among others, and witnesses for both.
    fn witnesses() -> (Anchor, Vec<SpendWitness>) {
        let sk = SpendingKey::from_bytes([3; 32]).unwrap();
        let addr = FullViewingKey::from(&sk).address_at(0u32, Scope::External);
        let mut tree = CommitmentTree::<MerkleHashOrchard, 32>::empty();
        let mut ours = vec![];
        let mut tracked: Vec<(Note, IncrementalWitness<MerkleHashOrchard, 32>)> = vec![];
        for i in 0..5u64 {
            let rho = Rho::from_bytes(&[i as u8; 32]).unwrap();
            let rseed = RandomSeed::from_bytes([0x40 + i as u8; 32], &rho).unwrap();
            let note = Note::from_parts(addr, NoteValue::from_raw(1000 + i), rho, rseed, NoteVersion::V3).unwrap();
            let leaf = MerkleHashOrchard::from_cmx(&ExtractedNoteCommitment::from(note.commitment()));
            tree.append(leaf).unwrap();
            for (_, w) in tracked.iter_mut() {
                w.append(leaf).unwrap();
            }
            if i == 1 || i == 3 {
                tracked.push((note, IncrementalWitness::from_tree(tree.clone()).unwrap()));
            }
        }
        for (note, w) in tracked {
            ours.push(SpendWitness { note, merkle_path: MerklePath::from(w.path().unwrap()) });
        }
        (Anchor::from(tree.root()), ours)
    }

    fn txid() -> TxId {
        TxId::from_bytes([9; 32])
    }

    #[test]
    fn round_trips_through_json() {
        let (anchor, ws) = witnesses();
        let kit = DisclosureKit::new(Network::Test, txid(), anchor, 4_545_600, &ws).unwrap();
        let json = serde_json::to_string(&kit).unwrap();
        let back: DisclosureKit = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kit);
        let rebuilt = back.witnesses().unwrap();
        assert_eq!(rebuilt.len(), 2);
        for (a, b) in ws.iter().zip(&rebuilt) {
            assert_eq!(a.note, b.note);
            assert_eq!(a.merkle_path.position(), b.merkle_path.position());
            assert_eq!(a.merkle_path.auth_path(), b.merkle_path.auth_path());
        }
        assert_eq!(back.txid().unwrap(), txid());
        assert_eq!(kit.notes[0].auth_path.len(), 32);
    }

    #[test]
    fn refuses_a_path_to_another_anchor() {
        let (_, ws) = witnesses();
        let other = Anchor::from(MerkleHashOrchard::empty_root(Level::from(32)));
        assert!(DisclosureKit::new(Network::Test, txid(), other, 1, &ws).is_err());
    }

    #[test]
    fn catches_a_damaged_kit() {
        let (anchor, ws) = witnesses();
        let kit = DisclosureKit::new(Network::Test, txid(), anchor, 1, &ws).unwrap();

        let mut value = kit.clone();
        value.notes[0].value += 1;
        assert!(value.witnesses().is_err());

        let mut sibling = kit.clone();
        sibling.notes[1].auth_path[4] = hex::encode(MerkleHashOrchard::empty_leaf().to_bytes());
        assert!(sibling.witnesses().is_err());

        let mut position = kit;
        position.notes[0].position ^= 1;
        assert!(position.witnesses().is_err());
    }

    #[test]
    fn txids_parse_in_display_order() {
        let t = TxId::from_bytes(core::array::from_fn(|i| i as u8));
        assert_eq!(parse_txid(&t.to_string()).unwrap(), t);
        assert!(parse_txid("abcd").is_err());
    }
}
