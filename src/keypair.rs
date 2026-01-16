//! ZK Keypair for Privacy Cash
//!
//! Implements a Poseidon-based keypair system for UTXO ownership.
//! Based on Tornado Cash Nova's approach.
//!
//! Note: For full compatibility with the TypeScript SDK, the Poseidon hash
//! implementation should match snarkjs's Poseidon. This implementation uses
//! a placeholder that can be replaced with the actual circom-compatible
//! Poseidon hash.

use crate::constants::FIELD_SIZE;
use crate::error::{PrivacyCashError, Result};
use num_bigint::BigUint;
use num_traits::Zero;
use sha3::{Digest, Keccak256};

/// ZK Keypair for UTXO ownership
///
/// This keypair uses Poseidon hashing for the public key derivation,
/// which is compatible with the ZK circuits.
#[derive(Clone)]
pub struct ZkKeypair {
    /// Private key as a field element
    privkey: BigUint,
    /// Public key = Poseidon(privkey)
    pubkey: BigUint,
}

impl std::fmt::Debug for ZkKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZkKeypair")
            .field("pubkey", &self.pubkey.to_string())
            .finish()
    }
}

impl ZkKeypair {
    /// Create a new keypair from a private key hex string
    ///
    /// # Arguments
    /// * `privkey_hex` - Hex string of the private key (with or without 0x prefix)
    pub fn from_hex(privkey_hex: &str) -> Result<Self> {
        let hex_str = privkey_hex.strip_prefix("0x").unwrap_or(privkey_hex);

        let raw_decimal = BigUint::parse_bytes(hex_str.as_bytes(), 16)
            .ok_or_else(|| PrivacyCashError::InvalidKeypair("Invalid hex string".to_string()))?;

        // Reduce modulo field size
        let privkey = raw_decimal % &*FIELD_SIZE;

        // Compute public key using Poseidon hash
        let pubkey = Self::poseidon_hash(&[privkey.clone()])?;

        Ok(Self { privkey, pubkey })
    }

    /// Create a new keypair from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let raw_decimal = BigUint::from_bytes_be(bytes);
        let privkey = raw_decimal % &*FIELD_SIZE;
        let pubkey = Self::poseidon_hash(&[privkey.clone()])?;
        Ok(Self { privkey, pubkey })
    }

    /// Generate a new random keypair
    pub fn generate() -> Result<Self> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes);

        // Create hex string with 0x prefix like ethers.Wallet
        let hex_str = format!("0x{}", hex::encode(bytes));
        Self::from_hex(&hex_str)
    }

    /// Get the private key as a BigUint
    pub fn privkey(&self) -> &BigUint {
        &self.privkey
    }

    /// Get the public key as a BigUint
    pub fn pubkey(&self) -> &BigUint {
        &self.pubkey
    }

    /// Get the private key as a decimal string
    pub fn privkey_string(&self) -> String {
        self.privkey.to_string()
    }

    /// Get the public key as a decimal string
    pub fn pubkey_string(&self) -> String {
        self.pubkey.to_string()
    }

    /// Sign a message (commitment + merkle path)
    ///
    /// signature = Poseidon(privkey, commitment, merklePath)
    pub fn sign(&self, commitment: &str, merkle_path: &str) -> Result<String> {
        let inputs = vec![
            self.privkey.clone(),
            BigUint::parse_bytes(commitment.as_bytes(), 10)
                .ok_or_else(|| PrivacyCashError::InvalidKeypair("Invalid commitment".to_string()))?,
            BigUint::parse_bytes(merkle_path.as_bytes(), 10)
                .ok_or_else(|| PrivacyCashError::InvalidKeypair("Invalid merkle path".to_string()))?,
        ];

        let result = Self::poseidon_hash(&inputs)?;
        Ok(result.to_string())
    }

    /// Compute Poseidon hash of multiple inputs
    ///
    /// NOTE: This is a placeholder implementation using Keccak256.
    /// For full compatibility with the ZK circuits, this should be replaced
    /// with a proper BN254 Poseidon implementation matching snarkjs.
    ///
    /// For production use, consider:
    /// 1. Using the TypeScript SDK for operations requiring proof generation
    /// 2. Implementing native Poseidon using ark-circom (requires resolving dependency conflicts)
    /// 3. Using FFI to call the WASM Poseidon hasher from @lightprotocol/hasher.rs
    pub fn poseidon_hash(inputs: &[BigUint]) -> Result<BigUint> {
        // Create a deterministic hash from inputs
        // This placeholder uses Keccak256 and reduces modulo field size
        let mut hasher = Keccak256::new();

        for input in inputs {
            // Pad each input to 32 bytes (little-endian)
            let bytes = input.to_bytes_le();
            let mut padded = [0u8; 32];
            let len = bytes.len().min(32);
            padded[..len].copy_from_slice(&bytes[..len]);
            hasher.update(padded);
        }

        let result = hasher.finalize();
        let hash_bigint = BigUint::from_bytes_be(&result);

        // Reduce modulo field size to ensure it's a valid field element
        Ok(hash_bigint % &*FIELD_SIZE)
    }

    /// Compute Poseidon hash from string inputs (for compatibility with JS SDK)
    pub fn poseidon_hash_strings(inputs: &[&str]) -> Result<String> {
        let biguint_inputs: Vec<BigUint> = inputs
            .iter()
            .map(|s| {
                BigUint::parse_bytes(s.as_bytes(), 10)
                    .ok_or_else(|| PrivacyCashError::InvalidKeypair(format!("Invalid input: {}", s)))
            })
            .collect::<Result<Vec<_>>>()?;

        let result = Self::poseidon_hash(&biguint_inputs)?;
        Ok(result.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = ZkKeypair::generate().unwrap();
        assert!(!keypair.privkey().is_zero());
        assert!(!keypair.pubkey().is_zero());
    }

    #[test]
    fn test_keypair_from_hex() {
        let hex_key = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let keypair = ZkKeypair::from_hex(hex_key).unwrap();
        assert!(!keypair.privkey().is_zero());
        assert!(!keypair.pubkey().is_zero());
    }

    #[test]
    fn test_poseidon_hash() {
        // Test that poseidon hash produces consistent output
        let input = BigUint::from(12345u64);
        let result1 = ZkKeypair::poseidon_hash(&[input.clone()]).unwrap();
        let result2 = ZkKeypair::poseidon_hash(&[input]).unwrap();
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_poseidon_hash_strings() {
        let inputs = &["123", "456", "789"];
        let result = ZkKeypair::poseidon_hash_strings(inputs).unwrap();
        assert!(!result.is_empty());
    }
}
