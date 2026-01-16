//! Utility functions for Privacy Cash SDK

use crate::constants::{PROGRAM_ID, RELAYER_API_URL, FIELD_SIZE};
#[allow(unused_imports)]
use crate::error::{PrivacyCashError, Result};
use crate::merkle_tree::MerklePath;
use borsh::BorshSerialize;
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;

/// External data for proof
#[derive(Debug, Clone)]
pub struct ExtData {
    pub recipient: Pubkey,
    pub ext_amount: i64,
    pub encrypted_output1: Vec<u8>,
    pub encrypted_output2: Vec<u8>,
    pub fee: u64,
    pub fee_recipient: Pubkey,
    pub mint_address: Pubkey,
}

/// Borsh-serializable ExtData for hashing
#[derive(BorshSerialize)]
struct ExtDataForHash {
    recipient: [u8; 32],
    ext_amount: i64,
    encrypted_output1: Vec<u8>,
    encrypted_output2: Vec<u8>,
    fee: u64,
    fee_recipient: [u8; 32],
    mint_address: [u8; 32],
}

impl ExtData {
    /// Calculate the hash of external data (SHA-256)
    pub fn hash(&self) -> [u8; 32] {
        let data_for_hash = ExtDataForHash {
            recipient: self.recipient.to_bytes(),
            ext_amount: self.ext_amount,
            encrypted_output1: self.encrypted_output1.clone(),
            encrypted_output2: self.encrypted_output2.clone(),
            fee: self.fee,
            fee_recipient: self.fee_recipient.to_bytes(),
            mint_address: self.mint_address.to_bytes(),
        };

        let serialized = borsh::to_vec(&data_for_hash).unwrap();
        Sha256::digest(&serialized).into()
    }
}

/// Tree state from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeState {
    pub root: String,
    #[serde(rename = "nextIndex")]
    pub next_index: u64,
}

/// Fetch Merkle tree state from relayer API
pub async fn query_remote_tree_state(token_name: Option<&str>) -> Result<TreeState> {
    let mut url = format!("{}/merkle/root", *RELAYER_API_URL);
    if let Some(token) = token_name {
        url = format!("{}?token={}", url, token);
    }

    log::debug!("Fetching Merkle root from: {}", url);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to fetch tree state: {}", e)))?;

    if !response.status().is_success() {
        return Err(PrivacyCashError::ApiError(format!(
            "Tree state API returned status: {}",
            response.status()
        )));
    }

    let state: TreeState = response
        .json()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to parse tree state: {}", e)))?;

    log::debug!("Fetched root: {}, nextIndex: {}", state.root, state.next_index);

    Ok(state)
}

/// Merkle proof from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProofResponse {
    #[serde(rename = "pathElements")]
    pub path_elements: Vec<String>,
    #[serde(rename = "pathIndices")]
    pub path_indices: Vec<usize>,
}

impl From<MerkleProofResponse> for MerklePath {
    fn from(resp: MerkleProofResponse) -> Self {
        MerklePath {
            path_elements: resp.path_elements,
            path_indices: resp.path_indices,
        }
    }
}

/// Fetch Merkle proof for a commitment
pub async fn fetch_merkle_proof(commitment: &str, token_name: Option<&str>) -> Result<MerklePath> {
    let mut url = format!("{}/merkle/proof/{}", *RELAYER_API_URL, commitment);
    if let Some(token) = token_name {
        url = format!("{}?token={}", url, token);
    }

    log::debug!("Fetching Merkle proof for: {}", commitment);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to fetch Merkle proof: {}", e)))?;

    if !response.status().is_success() {
        return Err(PrivacyCashError::MerkleProofError(format!(
            "Merkle proof API returned status: {}",
            response.status()
        )));
    }

    let proof: MerkleProofResponse = response
        .json()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to parse Merkle proof: {}", e)))?;

    log::debug!("Fetched proof with {} elements", proof.path_elements.len());

    Ok(proof.into())
}

/// Derive program PDAs
pub fn get_program_accounts() -> (Pubkey, Pubkey, Pubkey) {
    let (tree_account, _) = Pubkey::find_program_address(&[b"merkle_tree"], &PROGRAM_ID);

    let (tree_token_account, _) = Pubkey::find_program_address(&[b"tree_token"], &PROGRAM_ID);

    let (global_config_account, _) = Pubkey::find_program_address(&[b"global_config"], &PROGRAM_ID);

    (tree_account, tree_token_account, global_config_account)
}

/// Get SPL tree account PDA
pub fn get_spl_tree_account(mint: &Pubkey) -> Pubkey {
    let (tree_account, _) =
        Pubkey::find_program_address(&[b"merkle_tree", &mint.to_bytes()], &PROGRAM_ID);
    tree_account
}

/// Find nullifier PDAs for proof validation
pub fn find_nullifier_pdas(nullifiers: &[[u8; 32]]) -> (Pubkey, Pubkey) {
    let (nullifier0_pda, _) =
        Pubkey::find_program_address(&[b"nullifier0", &nullifiers[0]], &PROGRAM_ID);

    let (nullifier1_pda, _) =
        Pubkey::find_program_address(&[b"nullifier1", &nullifiers[1]], &PROGRAM_ID);

    (nullifier0_pda, nullifier1_pda)
}

/// Find cross-check nullifier PDAs
pub fn find_cross_check_nullifier_pdas(nullifiers: &[[u8; 32]]) -> (Pubkey, Pubkey) {
    // Cross-check uses swapped seed prefixes
    let (nullifier2_pda, _) =
        Pubkey::find_program_address(&[b"nullifier0", &nullifiers[1]], &PROGRAM_ID);

    let (nullifier3_pda, _) =
        Pubkey::find_program_address(&[b"nullifier1", &nullifiers[0]], &PROGRAM_ID);

    (nullifier2_pda, nullifier3_pda)
}

/// Get mint address field for circuit
pub fn get_mint_address_field(mint: &Pubkey) -> String {
    let mint_str = mint.to_string();

    // Special case for SOL
    if mint_str == "11111111111111111111111111111112" {
        return mint_str;
    }

    // For SPL tokens: use first 31 bytes
    let mint_bytes = &mint.to_bytes()[..31];
    BigUint::from_bytes_be(mint_bytes).to_string()
}

/// Calculate public amount for circuit
pub fn calculate_public_amount(ext_amount: i64, fee: u64) -> BigUint {
    let ext_bn = if ext_amount >= 0 {
        BigUint::from(ext_amount as u64)
    } else {
        // For negative amounts, we need to compute (ext_amount + FIELD_SIZE) % FIELD_SIZE
        let abs_amount = BigUint::from((-ext_amount) as u64);
        &*FIELD_SIZE - &abs_amount
    };

    let fee_bn = BigUint::from(fee);

    // public_amount = (ext_amount - fee + FIELD_SIZE) % FIELD_SIZE
    let result = if ext_bn >= fee_bn {
        &ext_bn - &fee_bn
    } else {
        &*FIELD_SIZE - (&fee_bn - &ext_bn)
    };

    result % &*FIELD_SIZE
}

/// Convert BigUint to 32-byte array (big-endian, reversed for circuit)
pub fn biguint_to_bytes_be(n: &BigUint) -> [u8; 32] {
    let bytes = n.to_bytes_be();
    let mut result = [0u8; 32];
    let start = 32usize.saturating_sub(bytes.len());
    let len = bytes.len().min(32);
    result[start..start + len].copy_from_slice(&bytes[..len]);
    result
}

/// Convert BigUint to 32-byte array (little-endian)
pub fn biguint_to_bytes_le(n: &BigUint) -> [u8; 32] {
    let bytes = n.to_bytes_le();
    let mut result = [0u8; 32];
    let len = bytes.len().min(32);
    result[..len].copy_from_slice(&bytes[..len]);
    result
}

/// Convert string decimal to 32-byte array for circuit
pub fn string_to_circuit_bytes(s: &str) -> Result<[u8; 32]> {
    let n = BigUint::parse_bytes(s.as_bytes(), 10)
        .ok_or_else(|| PrivacyCashError::SerializationError("Invalid decimal string".to_string()))?;

    // Convert to LE bytes and reverse for circuit format
    let le_bytes = biguint_to_bytes_le(&n);
    let mut result = le_bytes;
    result.reverse();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_amount_positive() {
        let result = calculate_public_amount(1000, 100);
        assert_eq!(result, BigUint::from(900u64));
    }

    #[test]
    fn test_public_amount_negative() {
        // For withdrawals, ext_amount is negative
        let result = calculate_public_amount(-1000, 100);
        // Result should be FIELD_SIZE - 1100
        let expected = &*FIELD_SIZE - BigUint::from(1100u64);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_program_accounts() {
        let (tree, token, config) = get_program_accounts();
        assert_ne!(tree, Pubkey::default());
        assert_ne!(token, Pubkey::default());
        assert_ne!(config, Pubkey::default());
    }
}
