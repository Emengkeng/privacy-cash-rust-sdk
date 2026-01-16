//! UTXO (Unspent Transaction Output) model for Privacy Cash
//!
//! Based on Tornado Cash Nova's UTXO model.

use crate::constants::{FIELD_SIZE, SOL_MINT};
use crate::error::{PrivacyCashError, Result};
use crate::keypair::ZkKeypair;
use num_bigint::BigUint;
use num_traits::Zero;
use rand::Rng;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// UTXO version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UtxoVersion {
    V1,
    V2,
}

impl Default for UtxoVersion {
    fn default() -> Self {
        Self::V2
    }
}

/// UTXO (Unspent Transaction Output)
#[derive(Clone)]
pub struct Utxo {
    /// Amount in base units (lamports for SOL, base units for SPL)
    pub amount: BigUint,

    /// Random blinding factor for privacy
    pub blinding: BigUint,

    /// ZK keypair for ownership proof
    pub keypair: ZkKeypair,

    /// Index in the Merkle tree
    pub index: u64,

    /// Mint address (for SPL tokens, or SOL placeholder)
    pub mint_address: String,

    /// UTXO version
    pub version: UtxoVersion,
}

impl std::fmt::Debug for Utxo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Utxo")
            .field("amount", &self.amount.to_string())
            .field("index", &self.index)
            .field("mint_address", &self.mint_address)
            .field("version", &self.version)
            .finish()
    }
}

impl Utxo {
    /// Create a new UTXO
    pub fn new(
        amount: impl Into<BigUint>,
        keypair: ZkKeypair,
        index: u64,
        mint_address: Option<&str>,
        version: Option<UtxoVersion>,
    ) -> Self {
        let mut rng = rand::thread_rng();
        let blinding = BigUint::from(rng.gen::<u64>() % 1_000_000_000);

        Self {
            amount: amount.into(),
            blinding,
            keypair,
            index,
            mint_address: mint_address
                .unwrap_or("11111111111111111111111111111112")
                .to_string(),
            version: version.unwrap_or_default(),
        }
    }

    /// Create a new UTXO with specific blinding factor
    pub fn with_blinding(
        amount: impl Into<BigUint>,
        blinding: impl Into<BigUint>,
        keypair: ZkKeypair,
        index: u64,
        mint_address: Option<&str>,
        version: Option<UtxoVersion>,
    ) -> Self {
        Self {
            amount: amount.into(),
            blinding: blinding.into(),
            keypair,
            index,
            mint_address: mint_address
                .unwrap_or("11111111111111111111111111111112")
                .to_string(),
            version: version.unwrap_or_default(),
        }
    }

    /// Create a dummy (zero-value) UTXO
    pub fn dummy(keypair: ZkKeypair, mint_address: Option<&str>) -> Self {
        Self::new(0u64, keypair, 0, mint_address, Some(UtxoVersion::V2))
    }

    /// Get the amount as u64
    pub fn amount_u64(&self) -> u64 {
        use num_traits::ToPrimitive;
        self.amount.to_u64().unwrap_or(0)
    }

    /// Check if this is a dummy (zero-value) UTXO
    pub fn is_dummy(&self) -> bool {
        self.amount.is_zero()
    }

    /// Calculate the commitment for this UTXO
    ///
    /// commitment = Poseidon(amount, pubkey, blinding, mintAddressField)
    pub fn get_commitment(&self) -> Result<String> {
        let mint_field = self.get_mint_address_field()?;

        ZkKeypair::poseidon_hash_strings(&[
            &self.amount.to_string(),
            &self.keypair.pubkey_string(),
            &self.blinding.to_string(),
            &mint_field,
        ])
    }

    /// Calculate the nullifier for this UTXO
    ///
    /// nullifier = Poseidon(commitment, index, signature)
    /// where signature = keypair.sign(commitment, index)
    pub fn get_nullifier(&self) -> Result<String> {
        let commitment = self.get_commitment()?;
        let index_str = self.index.to_string();
        let signature = self.keypair.sign(&commitment, &index_str)?;

        ZkKeypair::poseidon_hash_strings(&[&commitment, &index_str, &signature])
    }

    /// Get the mint address field for circuit computation
    ///
    /// For SOL: returns the mint string as-is
    /// For SPL: returns first 31 bytes of mint as BigUint
    fn get_mint_address_field(&self) -> Result<String> {
        // Special case for SOL
        if self.mint_address == "11111111111111111111111111111112" {
            return Ok(self.mint_address.clone());
        }

        // For SPL tokens: use first 31 bytes
        let mint = Pubkey::from_str(&self.mint_address)
            .map_err(|e| PrivacyCashError::InvalidKeypair(format!("Invalid mint: {}", e)))?;

        let mint_bytes = &mint.to_bytes()[..31];
        let field_value = BigUint::from_bytes_be(mint_bytes);

        Ok(field_value.to_string())
    }

    /// Serialize UTXO to a pipe-delimited string for encryption
    pub fn serialize_for_encryption(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.amount, self.blinding, self.index, self.mint_address
        )
    }

    /// Deserialize UTXO from a pipe-delimited string
    pub fn deserialize_from_encryption(
        data: &str,
        keypair: ZkKeypair,
        version: UtxoVersion,
    ) -> Result<Self> {
        let parts: Vec<&str> = data.split('|').collect();

        if parts.len() != 4 {
            return Err(PrivacyCashError::DecryptionError(
                "Invalid UTXO format".to_string(),
            ));
        }

        let amount = BigUint::parse_bytes(parts[0].as_bytes(), 10)
            .ok_or_else(|| PrivacyCashError::DecryptionError("Invalid amount".to_string()))?;

        let blinding = BigUint::parse_bytes(parts[1].as_bytes(), 10)
            .ok_or_else(|| PrivacyCashError::DecryptionError("Invalid blinding".to_string()))?;

        let index: u64 = parts[2]
            .parse()
            .map_err(|_| PrivacyCashError::DecryptionError("Invalid index".to_string()))?;

        let mint_address = parts[3].to_string();

        Ok(Self {
            amount,
            blinding,
            keypair,
            index,
            mint_address,
            version,
        })
    }

    /// Log UTXO details (for debugging)
    pub async fn log(&self) {
        let commitment = self.get_commitment().unwrap_or_else(|_| "ERROR".to_string());
        let nullifier = self.get_nullifier().unwrap_or_else(|_| "ERROR".to_string());

        log::debug!(
            "UTXO: amount={}, blinding={}, index={}, mint={}, commitment={}, nullifier={}",
            self.amount,
            self.blinding,
            self.index,
            self.mint_address,
            commitment,
            nullifier
        );
    }
}

/// Balance result structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    /// Balance in base units (lamports for SOL)
    pub lamports: u64,
}

/// SPL Token balance result structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplBalance {
    /// Balance in base units
    pub base_units: u64,

    /// Balance in token amount (base_units / units_per_token)
    pub amount: f64,

    /// Legacy: same as base_units
    #[deprecated(note = "Use base_units instead")]
    pub lamports: u64,
}

impl SplBalance {
    pub fn new(base_units: u64, units_per_token: u64) -> Self {
        #[allow(deprecated)]
        Self {
            base_units,
            amount: base_units as f64 / units_per_token as f64,
            lamports: base_units,
        }
    }

    pub fn zero() -> Self {
        #[allow(deprecated)]
        Self {
            base_units: 0,
            amount: 0.0,
            lamports: 0,
        }
    }
}

/// Calculate total balance from UTXOs
pub fn get_balance_from_utxos(utxos: &[Utxo]) -> Balance {
    let total: u64 = utxos.iter().map(|u| u.amount_u64()).sum();
    Balance { lamports: total }
}

/// Calculate total SPL balance from UTXOs
pub fn get_balance_from_utxos_spl(utxos: &[Utxo], units_per_token: u64) -> SplBalance {
    if utxos.is_empty() {
        return SplBalance::zero();
    }

    let total: u64 = utxos.iter().map(|u| u.amount_u64()).sum();
    SplBalance::new(total, units_per_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utxo_creation() {
        let keypair = ZkKeypair::generate().unwrap();
        let utxo = Utxo::new(1000u64, keypair, 0, None, None);

        assert_eq!(utxo.amount_u64(), 1000);
        assert_eq!(utxo.index, 0);
        assert!(!utxo.is_dummy());
    }

    #[test]
    fn test_dummy_utxo() {
        let keypair = ZkKeypair::generate().unwrap();
        let utxo = Utxo::dummy(keypair, None);

        assert!(utxo.is_dummy());
        assert_eq!(utxo.amount_u64(), 0);
    }

    #[test]
    fn test_commitment_calculation() {
        let keypair = ZkKeypair::generate().unwrap();
        let utxo = Utxo::new(1000u64, keypair, 0, None, None);

        let commitment = utxo.get_commitment().unwrap();
        assert!(!commitment.is_empty());

        // Commitment should be consistent
        let commitment2 = utxo.get_commitment().unwrap();
        assert_eq!(commitment, commitment2);
    }

    #[test]
    fn test_serialization() {
        let keypair = ZkKeypair::generate().unwrap();
        let utxo = Utxo::new(1000u64, keypair.clone(), 5, None, Some(UtxoVersion::V2));

        let serialized = utxo.serialize_for_encryption();
        let deserialized =
            Utxo::deserialize_from_encryption(&serialized, keypair, UtxoVersion::V2).unwrap();

        assert_eq!(utxo.amount, deserialized.amount);
        assert_eq!(utxo.blinding, deserialized.blinding);
        assert_eq!(utxo.index, deserialized.index);
        assert_eq!(utxo.mint_address, deserialized.mint_address);
    }
}
