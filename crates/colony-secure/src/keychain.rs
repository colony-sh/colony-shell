use keyring::Entry;

use crate::{generate_key, Result, SecureError};

const SERVICE_NAME: &str = "colony-shell";
const KEY_NAME: &str = "master-encryption-key";

pub struct Keychain {
    entry: Entry,
}

impl Keychain {
    pub fn new() -> Result<Self> {
        let entry = Entry::new(SERVICE_NAME, KEY_NAME).map_err(|e| {
            SecureError::Keychain(format!("Failed to create keychain entry: {}", e))
        })?;
        Ok(Self { entry })
    }

    pub fn get_or_create_key(&self) -> Result<[u8; 32]> {
        match self.get_key() {
            Ok(key) => Ok(key),
            Err(_) => {
                let key = generate_key();
                self.store_key(&key)?;
                Ok(key)
            }
        }
    }

    pub fn get_key(&self) -> Result<[u8; 32]> {
        let secret = self
            .entry
            .get_password()
            .map_err(|e| SecureError::Keychain(format!("Failed to retrieve key: {}", e)))?;

        let bytes = hex::decode(&secret)
            .map_err(|e| SecureError::Keychain(format!("Invalid key format in keychain: {}", e)))?;

        if bytes.len() != 32 {
            return Err(SecureError::Keychain(format!(
                "Invalid key length: expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(key)
    }

    pub fn store_key(&self, key: &[u8; 32]) -> Result<()> {
        let hex_key = hex::encode(key);
        self.entry
            .set_password(&hex_key)
            .map_err(|e| SecureError::Keychain(format!("Failed to store key: {}", e)))?;
        Ok(())
    }

    pub fn delete_key(&self) -> Result<()> {
        self.entry
            .delete_credential()
            .map_err(|e| SecureError::Keychain(format!("Failed to delete key: {}", e)))?;
        Ok(())
    }

    pub fn key_exists(&self) -> bool {
        self.entry.get_password().is_ok()
    }
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new().expect("Failed to initialize keychain")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires OS keychain access - run manually"]
    fn test_keychain_roundtrip() {
        let keychain = Keychain::new().unwrap();

        if keychain.key_exists() {
            keychain.delete_key().unwrap();
        }

        let key = keychain.get_or_create_key().unwrap();
        assert_eq!(key.len(), 32);

        let retrieved = keychain.get_key().unwrap();
        assert_eq!(key, retrieved);

        keychain.delete_key().unwrap();
        assert!(!keychain.key_exists());
    }
}
