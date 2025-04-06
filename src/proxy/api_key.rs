use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::error::Error;

const NONCE_SIZE: usize = 12;
const KEY_SIZE: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedApiKey {
    pub encrypted_data: String,
    pub nonce: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

pub struct ApiKeyManager {
    encryption_key: [u8; KEY_SIZE],
}

impl ApiKeyManager {
    pub fn new(encryption_key: [u8; KEY_SIZE]) -> Self {
        Self { encryption_key }
    }

    pub fn from_base64_key(key: &str) -> Result<Self, Error> {
        let key_bytes = BASE64.decode(key)
            .map_err(|e| Error::External(format!("Invalid encryption key: {}", e)))?;

        if key_bytes.len() != KEY_SIZE {
            return Err(Error::External("Invalid encryption key length".to_string()));
        }

        let mut encryption_key = [0u8; KEY_SIZE];
        encryption_key.copy_from_slice(&key_bytes);

        Ok(Self { encryption_key })
    }

    pub fn encrypt_api_key(&self, api_key: &str, ttl_seconds: Option<u64>) -> Result<EncryptedApiKey, Error> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| Error::External(format!("Encryption error: {}", e)))?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let encrypted = cipher
            .encrypt(nonce, api_key.as_bytes())
            .map_err(|e| Error::External(format!("Encryption failed: {}", e)))?;

        Ok(EncryptedApiKey {
            encrypted_data: BASE64.encode(encrypted),
            nonce: BASE64.encode(nonce_bytes),
            created_at: now,
            expires_at: ttl_seconds.map(|ttl| now + ttl),
        })
    }

    pub fn decrypt_api_key(&self, encrypted: &EncryptedApiKey) -> Result<String, Error> {
        // Check expiration
        if let Some(expires_at) = encrypted.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            if now > expires_at {
                return Err(Error::External("API key has expired".to_string()));
            }
        }

        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| Error::External(format!("Decryption error: {}", e)))?;

        let nonce_bytes = BASE64.decode(&encrypted.nonce)
            .map_err(|e| Error::External(format!("Invalid nonce: {}", e)))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let encrypted_data = BASE64.decode(&encrypted.encrypted_data)
            .map_err(|e| Error::External(format!("Invalid encrypted data: {}", e)))?;

        let decrypted = cipher
            .decrypt(nonce, encrypted_data.as_ref())
            .map_err(|e| Error::External(format!("Decryption failed: {}", e)))?;

        String::from_utf8(decrypted)
            .map_err(|e| Error::External(format!("Invalid UTF-8: {}", e)))
    }

    pub fn rotate_encryption_key(&mut self, new_key: [u8; KEY_SIZE]) -> [u8; KEY_SIZE] {
        let old_key = self.encryption_key;
        self.encryption_key = new_key;
        old_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_test_key() -> [u8; KEY_SIZE] {
        let mut key = [0u8; KEY_SIZE];
        rand::thread_rng().fill_bytes(&mut key);
        key
    }

    #[test]
    fn test_api_key_encryption() {
        let key = generate_test_key();
        let manager = ApiKeyManager::new(key);
        
        let api_key = "test-api-key-123";
        let encrypted = manager.encrypt_api_key(api_key, None).unwrap();
        
        assert!(encrypted.encrypted_data.len() > 0);
        assert!(encrypted.nonce.len() > 0);
        assert!(encrypted.created_at > 0);
        assert_eq!(encrypted.expires_at, None);
        
        let decrypted = manager.decrypt_api_key(&encrypted).unwrap();
        assert_eq!(decrypted, api_key);
    }

    #[test]
    fn test_api_key_expiration() {
        let key = generate_test_key();
        let manager = ApiKeyManager::new(key);
        
        let api_key = "test-api-key-123";
        let encrypted = manager.encrypt_api_key(api_key, Some(0)).unwrap();
        
        // Sleep to ensure expiration
        std::thread::sleep(std::time::Duration::from_secs(1));
        
        let result = manager.decrypt_api_key(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_key_rotation() {
        let key = generate_test_key();
        let mut manager = ApiKeyManager::new(key);
        
        // Encrypt with original key
        let api_key = "test-api-key-123";
        let encrypted = manager.encrypt_api_key(api_key, None).unwrap();
        
        // Verify decryption works with original key
        let decrypted = manager.decrypt_api_key(&encrypted).unwrap();
        assert_eq!(decrypted, api_key);
        
        // Rotate to new key
        let new_key = generate_test_key();
        let old_key = manager.rotate_encryption_key(new_key);
        
        // Verify old encrypted data can't be decrypted with new key
        let result = manager.decrypt_api_key(&encrypted);
        assert!(result.is_err());
        
        // Create new manager with old key to verify old data
        let old_manager = ApiKeyManager::new(old_key);
        let decrypted = old_manager.decrypt_api_key(&encrypted).unwrap();
        assert_eq!(decrypted, api_key);
    }
} 