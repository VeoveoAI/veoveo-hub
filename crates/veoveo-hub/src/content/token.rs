//! A token's value. The Hub stores only its digest, and the same browser reopening an item
//! gets the same token while it lives, so the value is derived and not kept: HMAC-SHA-256 under
//! the browser's session cookie, a random 256-bit value only that browser holds, of a random
//! nonce the row keeps and what the token names.
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// The bytes of a token: 160 bits, in hex one DNS label.
const TOKEN_BYTES: usize = 20;
/// The bytes of a nonce: 128 bits.
pub(super) const NONCE_BYTES: usize = 16;

/// A new random nonce.
pub(super) fn nonce() -> anyhow::Result<[u8; NONCE_BYTES]> {
    let mut bytes = [0_u8; NONCE_BYTES];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("random source: {error}"))?;
    Ok(bytes)
}

/// The token of `nonce` on `item` of `owner` for the browser holding `cookie`.
pub(super) fn derive(cookie: &str, nonce: &[u8], owner: &str, item: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(cookie.as_bytes()).expect("HMAC takes a key of any length");
    for part in [nonce, owner.as_bytes(), item.as_bytes()] {
        mac.update(&(part.len() as u64).to_be_bytes());
        mac.update(part);
    }
    hex::encode(&mac.finalize().into_bytes()[..TOKEN_BYTES])
}

/// Whether `label` is spelled as a token is.
pub(super) fn well_formed(label: &str) -> bool {
    label.len() == TOKEN_BYTES * 2
        && label
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_one_label_and_depends_on_all_it_names() {
        let cookie = "a".repeat(64);
        let token = derive(&cookie, &[1; NONCE_BYTES], "lore", "r/main");
        assert!(well_formed(&token), "{token}");
        assert_eq!(token, derive(&cookie, &[1; NONCE_BYTES], "lore", "r/main"));
        for other in [
            derive(&"b".repeat(64), &[1; NONCE_BYTES], "lore", "r/main"),
            derive(&cookie, &[2; NONCE_BYTES], "lore", "r/main"),
            derive(&cookie, &[1; NONCE_BYTES], "art", "r/main"),
            derive(&cookie, &[1; NONCE_BYTES], "lore", "r/dev"),
            // The parts cannot run into each other.
            derive(&cookie, &[1; NONCE_BYTES], "lorer", "/main"),
        ] {
            assert_ne!(token, other);
        }
        for refused in ["", "x1", &"A".repeat(40), &"g".repeat(40), &"a".repeat(41)] {
            assert!(!well_formed(refused), "{refused}");
        }
    }
}
