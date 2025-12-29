use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;

use crate::{Result, SecureError};

const ENCRYPTED_PREFIX: &str = "COLONY_ENC_v1:";
const NONCE_SIZE: usize = 12;
const KEY_SIZE: usize = 32;

pub fn encrypt(plaintext: &str, key: &[u8; KEY_SIZE]) -> Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| SecureError::Encryption(format!("Invalid key: {}", e)))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| SecureError::Encryption(format!("Encryption failed: {}", e)))?;

    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(format!("{}{}", ENCRYPTED_PREFIX, BASE64.encode(&combined)))
}

pub fn decrypt(encrypted: &str, key: &[u8; KEY_SIZE]) -> Result<String> {
    let encoded = encrypted.strip_prefix(ENCRYPTED_PREFIX).ok_or_else(|| {
        SecureError::Encryption("Invalid encrypted format: missing prefix".into())
    })?;

    let combined = BASE64
        .decode(encoded)
        .map_err(|e| SecureError::Encryption(format!("Invalid base64: {}", e)))?;

    if combined.len() < NONCE_SIZE {
        return Err(SecureError::Encryption("Ciphertext too short".into()));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| SecureError::Encryption(format!("Invalid key: {}", e)))?;

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        SecureError::Encryption("Decryption failed: invalid key or corrupted data".into())
    })?;

    String::from_utf8(plaintext)
        .map_err(|e| SecureError::Encryption(format!("Invalid UTF-8 in decrypted data: {}", e)))
}

pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

pub fn generate_key() -> [u8; KEY_SIZE] {
    let mut key = [0u8; KEY_SIZE];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_key();
        let plaintext = "my-secret-api-key-12345";

        let encrypted = encrypt(plaintext, &key).unwrap();
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX));
        assert!(is_encrypted(&encrypted));

        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_encryptions_differ() {
        let key = generate_key();
        let plaintext = "same-value";

        let enc1 = encrypt(plaintext, &key).unwrap();
        let enc2 = encrypt(plaintext, &key).unwrap();

        assert_ne!(enc1, enc2);

        assert_eq!(decrypt(&enc1, &key).unwrap(), plaintext);
        assert_eq!(decrypt(&enc2, &key).unwrap(), plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_key();
        let key2 = generate_key();
        let plaintext = "secret";

        let encrypted = encrypt(plaintext, &key1).unwrap();
        let result = decrypt(&encrypted, &key2);

        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_format_fails() {
        let key = generate_key();

        assert!(decrypt("not-encrypted", &key).is_err());
        assert!(decrypt("COLONY_ENC_v1:invalid-base64!!!", &key).is_err());
        assert!(decrypt("COLONY_ENC_v1:YWJj", &key).is_err());
    }

    #[test]
    fn test_is_encrypted() {
        assert!(is_encrypted("COLONY_ENC_v1:abc123"));
        assert!(!is_encrypted("plain-value"));
        assert!(!is_encrypted("COLONY_ENC_v2:different-version"));
    }

    #[test]
    fn test_empty_string() {
        let key = generate_key();
        let encrypted = encrypt("", &key).unwrap();
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_unicode() {
        let key = generate_key();
        let plaintext = "日本語テスト 🔐 émojis";

        let encrypted = encrypt(plaintext, &key).unwrap();
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
