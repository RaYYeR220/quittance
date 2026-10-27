//! Byte-level helpers: Zcash CompactSize integers and an unlimited-length
//! Bech32m checksum (the same parameters ZIP 316 uses for unified addresses).

use crate::error::DecodeError;

/// Bech32m with the length limit lifted, as in ZIP 316. A disclosure carries a
/// Halo2 proof, so its text form is thousands of characters long.
pub(crate) enum Bech32mLong {}

impl bech32::Checksum for Bech32mLong {
    type MidstateRepr = <bech32::Bech32m as bech32::Checksum>::MidstateRepr;
    const CODE_LENGTH: usize = 4_194_368;
    const CHECKSUM_LENGTH: usize = <bech32::Bech32m as bech32::Checksum>::CHECKSUM_LENGTH;
    const GENERATOR_SH: [u32; 5] = <bech32::Bech32m as bech32::Checksum>::GENERATOR_SH;
    const TARGET_RESIDUE: u32 = <bech32::Bech32m as bech32::Checksum>::TARGET_RESIDUE;
}

pub(crate) fn write_compact_size(out: &mut Vec<u8>, n: u64) {
    if n < 0xfd {
        out.push(n as u8);
    } else if n <= 0xffff {
        out.push(0xfd);
        out.extend_from_slice(&(n as u16).to_le_bytes());
    } else if n <= 0xffff_ffff {
        out.push(0xfe);
        out.extend_from_slice(&(n as u32).to_le_bytes());
    } else {
        out.push(0xff);
        out.extend_from_slice(&n.to_le_bytes());
    }
}

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < n {
            return Err(DecodeError::Truncated);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    pub(crate) fn compact_size(&mut self) -> Result<u64, DecodeError> {
        let first = self.u8()?;
        let (n, min) = match first {
            0xfd => (u16::from_le_bytes(self.array()?) as u64, 0xfd),
            0xfe => (u32::from_le_bytes(self.array()?) as u64, 0x1_0000),
            0xff => (u64::from_le_bytes(self.array()?), 0x1_0000_0000),
            b => return Ok(b as u64),
        };
        if n < min {
            return Err(DecodeError::NonCanonical);
        }
        Ok(n)
    }

    /// A CompactSize-prefixed count, rejected above `limit` before anything is allocated.
    pub(crate) fn count(&mut self, what: &'static str, limit: usize) -> Result<usize, DecodeError> {
        let n = self.compact_size()?;
        if n > limit as u64 {
            return Err(DecodeError::TooLarge(what, limit));
        }
        Ok(n as usize)
    }

    pub(crate) fn bytes(&mut self, what: &'static str, limit: usize) -> Result<Vec<u8>, DecodeError> {
        let n = self.count(what, limit)?;
        Ok(self.take(n)?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_size_round_trips_at_the_boundaries() {
        for n in [0u64, 1, 0xfc, 0xfd, 0xffff, 0x1_0000, 0xffff_ffff, 0x1_0000_0000, u64::MAX] {
            let mut out = vec![];
            write_compact_size(&mut out, n);
            let mut r = Reader::new(&out);
            assert_eq!(r.compact_size().unwrap(), n);
            assert_eq!(r.remaining(), 0);
        }
    }

    #[test]
    fn non_canonical_compact_size_is_rejected() {
        let mut r = Reader::new(&[0xfd, 0x10, 0x00]);
        assert_eq!(r.compact_size(), Err(DecodeError::NonCanonical));
    }
}
