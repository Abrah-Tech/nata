use rand::RngCore;
use sha2::{Digest, Sha256};

/// Peppered SHA-256 used for OTP codes and invitation tokens.
///
/// The pepper is deliberately not the JWT signing secret. Signing keys rotate;
/// a pepper that moves with them would invalidate every pending code and
/// invitation on each rotation. Configure `HASH_PEPPER` separately.
pub fn hash(pepper: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(pepper.as_bytes());
    hasher.update(b":");
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

/// Stable key id for a signing secret, so a token can name the key it was
/// signed with. Derived rather than configured: the API is stateless, and a
/// rotation is just "set `JWT_SECRET_PREV` to the old secret".
pub fn signing_kid(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"nata-jwt-kid:");
    hasher.update(secret.as_bytes());
    hex::encode(hasher.finalize())[..12].to_string()
}

/// 32 random bytes, hex encoded. Returned once and never stored.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn hashes_are_stable_and_secret_dependent() {
        assert_eq!(hash("pepper", "123456"), hash("pepper", "123456"));
        assert_ne!(hash("pepper", "123456"), hash("other", "123456"));
        assert_ne!(hash("pepper", "123456"), hash("pepper", "654321"));
    }

    #[test]
    fn key_ids_are_stable_and_secret_dependent() {
        assert_eq!(signing_kid("secret"), signing_kid("secret"));
        assert_ne!(signing_kid("secret"), signing_kid("other"));
        assert_eq!(signing_kid("secret").len(), 12);
    }

    #[test]
    fn tokens_are_256_bits_of_hex() {
        let mut seen = HashSet::new();

        for _ in 0..64 {
            let token = random_token();
            assert_eq!(token.len(), 64);
            assert!(token.chars().all(|character| character.is_ascii_hexdigit()));
            assert!(seen.insert(token), "token repeated");
        }
    }
}
