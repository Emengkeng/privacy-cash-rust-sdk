//! Encryption service for UTXO data
//!
//! Implements AES-256-GCM encryption with versioned format.

use crate::constants::SIGN_MESSAGE;
use crate::error::{PrivacyCashError, Result};
use crate::keypair::ZkKeypair;
use crate::utxo::{Utxo, UtxoVersion};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use sha3::{Digest, Keccak256};
use solana_sdk::signature::{Keypair, Signer};

/// Version identifier for V2 encryption format (8 bytes)
const ENCRYPTION_VERSION_V2: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02];

/// Encryption key pair for V1 and V2 formats
#[derive(Clone)]
pub struct EncryptionKey {
    pub v1: Vec<u8>,
    pub v2: Vec<u8>,
}

/// Encryption service for UTXO data
#[derive(Clone)]
pub struct EncryptionService {
    /// V1 encryption key (legacy, 31 bytes from signature)
    encryption_key_v1: Option<Vec<u8>>,

    /// V2 encryption key (32 bytes, Keccak256 of signature)
    encryption_key_v2: Option<Vec<u8>>,

    /// V1 UTXO private key (cached)
    utxo_private_key_v1: Option<String>,

    /// V2 UTXO private key (cached)
    utxo_private_key_v2: Option<String>,
}

impl std::fmt::Debug for EncryptionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionService")
            .field("has_v1_key", &self.encryption_key_v1.is_some())
            .field("has_v2_key", &self.encryption_key_v2.is_some())
            .finish()
    }
}

impl EncryptionService {
    /// Create a new encryption service
    pub fn new() -> Self {
        Self {
            encryption_key_v1: None,
            encryption_key_v2: None,
            utxo_private_key_v1: None,
            utxo_private_key_v2: None,
        }
    }

    /// Derive encryption keys from a wallet keypair
    pub fn derive_encryption_key_from_wallet(&mut self, keypair: &Keypair) -> EncryptionKey {
        // Sign the constant message
        let message = SIGN_MESSAGE.as_bytes();
        let signature = keypair.sign_message(message);

        self.derive_encryption_key_from_signature(&signature.as_ref())
    }

    /// Derive encryption keys from a signature
    pub fn derive_encryption_key_from_signature(&mut self, signature: &[u8]) -> EncryptionKey {
        // V1: Extract first 31 bytes of signature (legacy method)
        let encryption_key_v1 = signature[..31].to_vec();
        self.encryption_key_v1 = Some(encryption_key_v1.clone());

        // Precompute V1 UTXO private key
        let hashed_seed_v1 = Sha256::digest(&encryption_key_v1);
        self.utxo_private_key_v1 = Some(format!("0x{}", hex::encode(hashed_seed_v1)));

        // V2: Use Keccak256 to derive full 32-byte key
        let encryption_key_v2 = Keccak256::digest(signature).to_vec();
        self.encryption_key_v2 = Some(encryption_key_v2.clone());

        // Precompute V2 UTXO private key
        let hashed_seed_v2 = Keccak256::digest(&encryption_key_v2);
        self.utxo_private_key_v2 = Some(format!("0x{}", hex::encode(hashed_seed_v2)));

        EncryptionKey {
            v1: encryption_key_v1,
            v2: encryption_key_v2,
        }
    }

    /// Encrypt data using V2 format (AES-256-GCM)
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let key = self
            .encryption_key_v2
            .as_ref()
            .ok_or_else(|| PrivacyCashError::EncryptionError("Encryption key not set".to_string()))?;

        // Generate random 12-byte IV for GCM
        let mut rng = rand::thread_rng();
        let mut iv = [0u8; 12];
        rng.fill(&mut iv);

        // Create cipher
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| PrivacyCashError::EncryptionError(format!("Invalid key: {}", e)))?;

        let nonce = Nonce::from_slice(&iv);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| PrivacyCashError::EncryptionError(format!("Encryption failed: {}", e)))?;

        // V2 format: [version(8)] + [IV(12)] + [ciphertext with auth tag]
        // Note: aes-gcm appends the 16-byte auth tag to the ciphertext
        let mut result = Vec::with_capacity(8 + 12 + ciphertext.len());
        result.extend_from_slice(&ENCRYPTION_VERSION_V2);
        result.extend_from_slice(&iv);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data (auto-detects V1 or V2 format)
    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        if encrypted_data.len() < 8 {
            return Err(PrivacyCashError::DecryptionError("Data too short".to_string()));
        }

        // Check if V2 format
        if encrypted_data[..8] == ENCRYPTION_VERSION_V2 {
            self.decrypt_v2(encrypted_data)
        } else {
            self.decrypt_v1(encrypted_data)
        }
    }

    /// Decrypt V2 format (AES-256-GCM)
    fn decrypt_v2(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        let key = self
            .encryption_key_v2
            .as_ref()
            .ok_or_else(|| PrivacyCashError::DecryptionError("V2 encryption key not set".to_string()))?;

        if encrypted_data.len() < 8 + 12 + 16 {
            // version + iv + min auth tag
            return Err(PrivacyCashError::DecryptionError("Data too short for V2".to_string()));
        }

        // Extract components
        let iv = &encrypted_data[8..20]; // 12 bytes
        let ciphertext = &encrypted_data[20..]; // rest (includes auth tag)

        // Create cipher
        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| PrivacyCashError::DecryptionError(format!("Invalid key: {}", e)))?;

        let nonce = Nonce::from_slice(iv);

        // Decrypt
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| PrivacyCashError::DecryptionError("Invalid key or corrupted data".to_string()))
    }

    /// Decrypt V1 format (AES-128-CTR with HMAC)
    fn decrypt_v1(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        let key = self
            .encryption_key_v1
            .as_ref()
            .ok_or_else(|| PrivacyCashError::DecryptionError("V1 encryption key not set".to_string()))?;

        if encrypted_data.len() < 32 {
            // iv(16) + auth_tag(16)
            return Err(PrivacyCashError::DecryptionError("Data too short for V1".to_string()));
        }

        // Extract components
        let iv = &encrypted_data[..16];
        let auth_tag = &encrypted_data[16..32];
        let data = &encrypted_data[32..];

        // Verify HMAC
        let hmac_key = &key[16..31];
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(hmac_key)
            .map_err(|e| PrivacyCashError::DecryptionError(format!("HMAC error: {}", e)))?;
        mac.update(iv);
        mac.update(data);
        let calculated_tag = &mac.finalize().into_bytes()[..16];

        if !constant_time_eq(auth_tag, calculated_tag) {
            return Err(PrivacyCashError::DecryptionError("Invalid key or corrupted data".to_string()));
        }

        // Decrypt using AES-128-CTR
        use aes::cipher::{KeyIvInit, StreamCipher};
        type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

        let cipher_key = &key[..16];
        let mut cipher = Aes128Ctr::new_from_slices(cipher_key, iv)
            .map_err(|e| PrivacyCashError::DecryptionError(format!("Cipher error: {}", e)))?;

        let mut plaintext = data.to_vec();
        cipher.apply_keystream(&mut plaintext);

        Ok(plaintext)
    }

    /// Encrypt a UTXO
    pub fn encrypt_utxo(&self, utxo: &Utxo) -> Result<Vec<u8>> {
        let serialized = utxo.serialize_for_encryption();
        self.encrypt(serialized.as_bytes())
    }

    /// Decrypt a UTXO
    pub fn decrypt_utxo(&self, encrypted_data: &[u8]) -> Result<Utxo> {
        let version = self.get_encryption_version(encrypted_data);
        let decrypted = self.decrypt(encrypted_data)?;

        let data_str = String::from_utf8(decrypted)
            .map_err(|_| PrivacyCashError::DecryptionError("Invalid UTF-8".to_string()))?;

        let private_key = self.get_utxo_private_key_with_version(version)?;
        let keypair = ZkKeypair::from_hex(&private_key)?;

        Utxo::deserialize_from_encryption(&data_str, keypair, version)
    }

    /// Decrypt UTXO from hex string
    pub fn decrypt_utxo_from_hex(&self, hex_data: &str) -> Result<Utxo> {
        let data = hex::decode(hex_data)
            .map_err(|e| PrivacyCashError::DecryptionError(format!("Invalid hex: {}", e)))?;
        self.decrypt_utxo(&data)
    }

    /// Get encryption version from encrypted data
    pub fn get_encryption_version(&self, encrypted_data: &[u8]) -> UtxoVersion {
        if encrypted_data.len() >= 8 && encrypted_data[..8] == ENCRYPTION_VERSION_V2 {
            UtxoVersion::V2
        } else {
            UtxoVersion::V1
        }
    }

    /// Get UTXO private key for a specific version
    pub fn get_utxo_private_key_with_version(&self, version: UtxoVersion) -> Result<String> {
        match version {
            UtxoVersion::V1 => self.utxo_private_key_v1.clone().ok_or_else(|| {
                PrivacyCashError::EncryptionError("V1 UTXO private key not set".to_string())
            }),
            UtxoVersion::V2 => self.utxo_private_key_v2.clone().ok_or_else(|| {
                PrivacyCashError::EncryptionError("V2 UTXO private key not set".to_string())
            }),
        }
    }

    /// Derive UTXO private key (V1 by default, or V2 if encrypted data is V2)
    pub fn derive_utxo_private_key(&self, encrypted_data: Option<&[u8]>) -> Result<String> {
        let version = encrypted_data
            .map(|data| self.get_encryption_version(data))
            .unwrap_or(UtxoVersion::V1);

        self.get_utxo_private_key_with_version(version)
    }

    /// Get V1 UTXO private key
    pub fn get_utxo_private_key_v1(&self) -> Result<String> {
        self.get_utxo_private_key_with_version(UtxoVersion::V1)
    }

    /// Get V2 UTXO private key
    pub fn get_utxo_private_key_v2(&self) -> Result<String> {
        self.get_utxo_private_key_with_version(UtxoVersion::V2)
    }

    /// Reset all keys
    pub fn reset(&mut self) {
        self.encryption_key_v1 = None;
        self.encryption_key_v2 = None;
        self.utxo_private_key_v1 = None;
        self.utxo_private_key_v2 = None;
    }
}

impl Default for EncryptionService {
    fn default() -> Self {
        Self::new()
    }
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let keypair = Keypair::new();
        let mut service = EncryptionService::new();
        service.derive_encryption_key_from_wallet(&keypair);

        let data = b"Hello, Privacy Cash!";
        let encrypted = service.encrypt(data).unwrap();
        let decrypted = service.decrypt(&encrypted).unwrap();

        assert_eq!(data.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_utxo_encryption() {
        let keypair = Keypair::new();
        let mut service = EncryptionService::new();
        service.derive_encryption_key_from_wallet(&keypair);

        let zk_keypair = ZkKeypair::from_hex(&service.get_utxo_private_key_v2().unwrap()).unwrap();
        let utxo = Utxo::new(1000u64, zk_keypair, 5, None, Some(UtxoVersion::V2));

        let encrypted = service.encrypt_utxo(&utxo).unwrap();
        let decrypted = service.decrypt_utxo(&encrypted).unwrap();

        assert_eq!(utxo.amount, decrypted.amount);
        assert_eq!(utxo.blinding, decrypted.blinding);
        assert_eq!(utxo.index, decrypted.index);
    }
}
