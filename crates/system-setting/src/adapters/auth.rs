use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString};
use argon2::Argon2;

use crate::ports::PasswordHasher;

#[derive(Clone, Default)]
pub struct Argon2PasswordHasher;

impl PasswordHasher for Argon2PasswordHasher {
    fn hash(&self, plain: &str) -> String {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(plain.as_bytes(), &salt)
            .expect("argon2 hash")
            .to_string()
    }

    fn verify(&self, plain: &str, hash: &str) -> bool {
        match PasswordHash::new(hash) {
            Ok(parsed) => Argon2::default().verify_password(plain.as_bytes(), &parsed).is_ok(),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_roundtrip() {
        let h = Argon2PasswordHasher;
        let hash = h.hash("correct horse");
        assert!(hash.starts_with("$argon2"));
        assert!(h.verify("correct horse", &hash));
        assert!(!h.verify("wrong", &hash));
    }
}
