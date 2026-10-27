//! Who an Ironwood-only wallet will pay.
//!
//! Ironwood notes are sent to Orchard-format receivers, so the only acceptable
//! recipient is a unified address carrying an Orchard receiver. Every other
//! kind is refused before any transaction is proposed, with the reason spelled
//! out: a transparent or TEX payment would be public, a Sapling payment would
//! land in a pool this wallet never discloses from.

use thiserror::Error;
use zcash_address::{
    unified::{self, Container, Encoding},
    ConversionError, ToAddress, TryFromAddress, ZcashAddress,
};
use zcash_protocol::consensus::NetworkType;
use zip311::Network;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RecipientError {
    #[error("`{0}` is not a Zcash address")]
    Unparseable(String),
    #[error("this is a {address} address, but the wallet is on {wallet}")]
    WrongNetwork { address: &'static str, wallet: &'static str },
    #[error("transparent addresses are refused: the payment would be public, and this wallet never touches the transparent pool")]
    Transparent,
    #[error("TEX addresses (ZIP 320) are refused: they can only be paid from transparent funds")]
    Tex,
    #[error("Sapling addresses are refused: this wallet pays only Ironwood; ask the payee for a unified address with an Orchard receiver")]
    Sapling,
    #[error("Sprout addresses can no longer receive funds")]
    Sprout,
    #[error("this unified address has no Orchard receiver, so it cannot receive Ironwood funds")]
    NoOrchardReceiver,
}

/// The Orchard receiver an Ironwood payment goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IronwoodRecipient(pub [u8; 43]);

impl IronwoodRecipient {
    /// Parses `address` for `network`, keeping only its Orchard receiver.
    pub fn parse(address: &str, network: Network) -> Result<Self, RecipientError> {
        let parsed = ZcashAddress::try_from_encoded(address.trim())
            .map_err(|_| RecipientError::Unparseable(address.trim().to_string()))?;
        parsed.convert_if_network::<Self>(network.network_type()).map_err(|e| match e {
            ConversionError::User(e) => e,
            ConversionError::IncorrectNetwork { actual, .. } => {
                RecipientError::WrongNetwork { address: network_name(actual), wallet: network_name(network.network_type()) }
            }
            // Every address kind is handled by the conversion below.
            ConversionError::Unsupported(_) => RecipientError::Unparseable(address.trim().to_string()),
        })
    }

    /// A unified address with this receiver alone, so the payment can only be
    /// made to the Orchard receiver, whatever else the original address offered.
    pub fn to_address(self, network: Network) -> ZcashAddress {
        let ua = unified::Address::try_from_items(unified::Revision::R0, vec![unified::Uitem::Data(unified::Receiver::Orchard(self.0))])
            .expect("a single Orchard receiver is a valid unified address");
        ZcashAddress::from_unified(network.network_type(), ua)
    }

    pub fn encode(self, network: Network) -> String {
        self.to_address(network).encode()
    }
}

impl TryFromAddress for IronwoodRecipient {
    type Error = RecipientError;

    fn try_from_sprout(_: NetworkType, _: [u8; 64]) -> Result<Self, ConversionError<Self::Error>> {
        Err(ConversionError::User(RecipientError::Sprout))
    }

    fn try_from_sapling(_: NetworkType, _: [u8; 43]) -> Result<Self, ConversionError<Self::Error>> {
        Err(ConversionError::User(RecipientError::Sapling))
    }

    fn try_from_unified(_: NetworkType, ua: unified::Address) -> Result<Self, ConversionError<Self::Error>> {
        ua.items()
            .into_iter()
            .find_map(|r| match r {
                unified::Receiver::Orchard(raw) => Some(IronwoodRecipient(raw)),
                _ => None,
            })
            .ok_or(ConversionError::User(RecipientError::NoOrchardReceiver))
    }

    fn try_from_transparent_p2pkh(_: NetworkType, _: [u8; 20]) -> Result<Self, ConversionError<Self::Error>> {
        Err(ConversionError::User(RecipientError::Transparent))
    }

    fn try_from_transparent_p2sh(_: NetworkType, _: [u8; 20]) -> Result<Self, ConversionError<Self::Error>> {
        Err(ConversionError::User(RecipientError::Transparent))
    }

    fn try_from_tex(_: NetworkType, _: [u8; 20]) -> Result<Self, ConversionError<Self::Error>> {
        Err(ConversionError::User(RecipientError::Tex))
    }
}

fn network_name(n: NetworkType) -> &'static str {
    match n {
        NetworkType::Main => "mainnet",
        NetworkType::Test => "testnet",
        NetworkType::Regtest => "regtest",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real testnet Orchard-only address (the demo payee's).
    const PAYEE: &str = "utest1c55d5uhv40c0cpcp5l89x7ps9n5vvc3tlywte4wvf78pcl55t5jys0rnvaycvt962dg02pkn2rrpv6d4vnl3xqvnwmmw5y7ldv0m7ujj";

    fn ua(net: NetworkType, receivers: Vec<unified::Receiver>) -> String {
        let items = receivers.into_iter().map(unified::Uitem::Data).collect();
        ZcashAddress::from_unified(net, unified::Address::try_from_items(unified::Revision::R0, items).unwrap()).encode()
    }

    fn orchard_receiver() -> [u8; 43] {
        IronwoodRecipient::parse(PAYEE, Network::Test).unwrap().0
    }

    #[test]
    fn orchard_only_address_round_trips() {
        let r = IronwoodRecipient::parse(PAYEE, Network::Test).unwrap();
        assert_eq!(r.encode(Network::Test), PAYEE);
    }

    #[test]
    fn keeps_only_the_orchard_receiver_of_a_mixed_address() {
        let mixed = ua(NetworkType::Test, vec![unified::Receiver::Orchard(orchard_receiver()), unified::Receiver::P2pkh([7; 20])]);
        let r = IronwoodRecipient::parse(&mixed, Network::Test).unwrap();
        assert_eq!(r.0, orchard_receiver());
        assert_eq!(r.encode(Network::Test), PAYEE);
    }

    #[test]
    fn refuses_everything_that_is_not_ironwood() {
        let t = NetworkType::Test;
        let cases = [
            (ZcashAddress::from_transparent_p2pkh(t, [1; 20]).encode(), RecipientError::Transparent),
            (ZcashAddress::from_transparent_p2sh(t, [1; 20]).encode(), RecipientError::Transparent),
            (ZcashAddress::from_tex(t, [1; 20]).encode(), RecipientError::Tex),
            (ZcashAddress::from_sapling(t, [1; 43]).encode(), RecipientError::Sapling),
            (ua(t, vec![unified::Receiver::P2pkh([1; 20])]), RecipientError::NoOrchardReceiver),
        ];
        for (addr, expected) in cases {
            assert_eq!(IronwoodRecipient::parse(&addr, Network::Test), Err(expected), "{addr}");
        }
    }

    #[test]
    fn refuses_other_networks_and_garbage() {
        assert_eq!(
            IronwoodRecipient::parse(PAYEE, Network::Main),
            Err(RecipientError::WrongNetwork { address: "testnet", wallet: "mainnet" })
        );
        assert!(matches!(IronwoodRecipient::parse("hello", Network::Test), Err(RecipientError::Unparseable(_))));
    }
}
