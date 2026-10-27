//! Reading the account's seed phrase.
//!
//! The phrase is read from a file each time a key is needed and dropped (and
//! zeroed) right after. No error message ever contains any part of it.

use std::path::Path;

use bip0039::{English, Mnemonic};
use secrecy::SecretVec;
use zeroize::Zeroizing;

use crate::error::{Error, Result};

/// Reads the BIP 39 phrase in `path` and returns its seed (empty passphrase).
///
/// The file holds a line `mnemonic: <words>` among any others (addresses,
/// notes); a file holding nothing but the phrase is accepted as well.
pub(crate) fn read_seed(path: &Path) -> Result<SecretVec<u8>> {
    let text = Zeroizing::new(std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?);
    let phrase = find_phrase(&text)
        .ok_or_else(|| Error::Mnemonic(format!("{} has no `mnemonic: <24 words>` line", path.display())))?;
    let mnemonic = Mnemonic::<English>::from_phrase(phrase).map_err(|_| {
        Error::Mnemonic(format!("the phrase in {} is not a valid BIP 39 English mnemonic", path.display()))
    })?;
    let seed = Zeroizing::new(mnemonic.to_seed(""));
    Ok(SecretVec::new(seed.to_vec()))
}

fn find_phrase(text: &str) -> Option<&str> {
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("mnemonic") {
                return Some(value.trim());
            }
        }
    }
    let bare = text.trim();
    (!bare.contains(':') && bare.split_whitespace().count() >= 12).then_some(bare)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn finds_the_phrase_among_other_lines() {
        let file = format!("main ua: u1xyz\nmnemonic:  {PHRASE}  \ntest ua: utest1xyz\n");
        assert_eq!(find_phrase(&file), Some(PHRASE));
        assert_eq!(find_phrase(&format!("{PHRASE}\n")), Some(PHRASE));
        assert_eq!(find_phrase("test ua: utest1xyz\n"), None);
        assert_eq!(find_phrase("too short\n"), None);
    }

    #[test]
    fn errors_never_echo_the_phrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.txt");
        std::fs::write(&path, "mnemonic: abandon abandon zebra abandon abandon abandon abandon abandon abandon abandon abandon about\n").unwrap();
        let err = read_seed(&path).unwrap_err().to_string();
        assert!(!err.contains("zebra") && !err.contains("abandon"), "{err}");

        std::fs::write(&path, format!("mnemonic: {PHRASE}\n")).unwrap();
        assert!(read_seed(&path).is_ok());
    }
}
