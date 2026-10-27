use serde::{Deserialize, Serialize};
use zcash_protocol::consensus::{self, NetworkType};

/// The network a disclosure is about. It fixes the SLIP-44 coin type mixed
/// into the signed digest, so a disclosure can't be moved between networks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Main,
    Test,
}

impl Network {
    /// The SLIP-44 coin type ZIP 311 mixes into the digest personalization
    /// (not the hardened form).
    pub fn coin_type(self) -> u32 {
        match self {
            Network::Main => 133,
            Network::Test => 1,
        }
    }

    /// Human-readable part of the text encoding.
    pub fn hrp(self) -> &'static str {
        match self {
            Network::Main => "zpd",
            Network::Test => "zpdtest",
        }
    }

    pub(crate) fn from_hrp(hrp: &str) -> Option<Self> {
        match hrp {
            "zpd" => Some(Network::Main),
            "zpdtest" => Some(Network::Test),
            _ => None,
        }
    }

    pub(crate) fn to_byte(self) -> u8 {
        match self {
            Network::Main => 0,
            Network::Test => 1,
        }
    }

    pub(crate) fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Network::Main),
            1 => Some(Network::Test),
            _ => None,
        }
    }

    pub fn network_type(self) -> NetworkType {
        match self {
            Network::Main => NetworkType::Main,
            Network::Test => NetworkType::Test,
        }
    }

    pub fn params(self) -> consensus::Network {
        match self {
            Network::Main => consensus::Network::MainNetwork,
            Network::Test => consensus::Network::TestNetwork,
        }
    }
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Network::Main => "mainnet",
            Network::Test => "testnet",
        })
    }
}
