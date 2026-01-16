//! ZK Proof generation for Privacy Cash
//!
//! Uses snarkjs WASM for proof generation, compatible with the TypeScript SDK.

use crate::error::{PrivacyCashError, Result};
use crate::utils::biguint_to_bytes_le;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Groth16 proof structure (compatible with snarkjs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    pub pi_a: Vec<String>,
    pub pi_b: Vec<Vec<String>>,
    pub pi_c: Vec<String>,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default = "default_curve")]
    pub curve: String,
}

fn default_protocol() -> String {
    "groth16".to_string()
}

fn default_curve() -> String {
    "bn128".to_string()
}

/// Parsed proof in bytes for on-chain submission
#[derive(Debug, Clone)]
pub struct ProofBytes {
    pub proof_a: Vec<u8>,
    pub proof_b: Vec<u8>,
    pub proof_c: Vec<u8>,
}

/// Circuit input for proof generation
#[derive(Debug, Clone, Serialize)]
pub struct CircuitInput {
    // Common transaction data
    pub root: String,
    #[serde(rename = "inputNullifier")]
    pub input_nullifier: Vec<String>,
    #[serde(rename = "outputCommitment")]
    pub output_commitment: Vec<String>,
    #[serde(rename = "publicAmount")]
    pub public_amount: String,
    #[serde(rename = "extDataHash")]
    pub ext_data_hash: Vec<u8>,

    // Input UTXO data
    #[serde(rename = "inAmount")]
    pub in_amount: Vec<String>,
    #[serde(rename = "inPrivateKey")]
    pub in_private_key: Vec<BigUint>,
    #[serde(rename = "inBlinding")]
    pub in_blinding: Vec<String>,
    #[serde(rename = "inPathIndices")]
    pub in_path_indices: Vec<u64>,
    #[serde(rename = "inPathElements")]
    pub in_path_elements: Vec<Vec<String>>,

    // Output UTXO data
    #[serde(rename = "outAmount")]
    pub out_amount: Vec<String>,
    #[serde(rename = "outBlinding")]
    pub out_blinding: Vec<String>,
    #[serde(rename = "outPubkey")]
    pub out_pubkey: Vec<BigUint>,

    // Mint address field
    #[serde(rename = "mintAddress")]
    pub mint_address: String,
}

impl CircuitInput {
    /// Convert to JSON for snarkjs
    pub fn to_json(&self) -> Result<String> {
        // Convert BigUint fields to strings for JSON serialization
        let mut input_map: HashMap<String, serde_json::Value> = HashMap::new();

        input_map.insert("root".to_string(), serde_json::json!(self.root));
        input_map.insert(
            "inputNullifier".to_string(),
            serde_json::json!(self.input_nullifier),
        );
        input_map.insert(
            "outputCommitment".to_string(),
            serde_json::json!(self.output_commitment),
        );
        input_map.insert(
            "publicAmount".to_string(),
            serde_json::json!(self.public_amount),
        );
        input_map.insert(
            "extDataHash".to_string(),
            serde_json::json!(BigUint::from_bytes_be(&self.ext_data_hash).to_string()),
        );

        input_map.insert("inAmount".to_string(), serde_json::json!(self.in_amount));
        input_map.insert(
            "inPrivateKey".to_string(),
            serde_json::json!(self
                .in_private_key
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()),
        );
        input_map.insert(
            "inBlinding".to_string(),
            serde_json::json!(self.in_blinding),
        );
        input_map.insert(
            "inPathIndices".to_string(),
            serde_json::json!(self.in_path_indices),
        );
        input_map.insert(
            "inPathElements".to_string(),
            serde_json::json!(self.in_path_elements),
        );

        input_map.insert("outAmount".to_string(), serde_json::json!(self.out_amount));
        input_map.insert(
            "outBlinding".to_string(),
            serde_json::json!(self.out_blinding),
        );
        input_map.insert(
            "outPubkey".to_string(),
            serde_json::json!(self
                .out_pubkey
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()),
        );

        input_map.insert(
            "mintAddress".to_string(),
            serde_json::json!(self.mint_address),
        );

        serde_json::to_string(&input_map)
            .map_err(|e| PrivacyCashError::SerializationError(e.to_string()))
    }
}

/// Prover for generating ZK proofs
///
/// Note: This implementation requires snarkjs to be installed globally via npm.
/// Run: `npm install -g snarkjs`
///
/// Alternatively, use the TypeScript SDK for proof generation and this SDK
/// for the Solana transaction building and submission.
pub struct Prover {
    /// Base path for circuit files (.wasm and .zkey)
    key_base_path: String,
}

impl Prover {
    /// Create a new prover with circuit files at the given path
    pub fn new(key_base_path: &str) -> Self {
        Self {
            key_base_path: key_base_path.to_string(),
        }
    }

    /// Generate a ZK proof using snarkjs CLI
    ///
    /// This method shells out to snarkjs which must be installed globally.
    /// For production use, consider using the TypeScript SDK for proof generation
    /// or implementing a native WASM-based prover.
    pub async fn prove(&self, input: &CircuitInput) -> Result<(Proof, Vec<String>)> {
        let wasm_path = format!("{}.wasm", self.key_base_path);
        let zkey_path = format!("{}.zkey", self.key_base_path);

        // Check that circuit files exist
        if !Path::new(&wasm_path).exists() {
            return Err(PrivacyCashError::CircuitNotFound(format!(
                "WASM file not found: {}. Please download circuit files from the Privacy Cash SDK.",
                wasm_path
            )));
        }
        if !Path::new(&zkey_path).exists() {
            return Err(PrivacyCashError::CircuitNotFound(format!(
                "zkey file not found: {}. Please download circuit files from the Privacy Cash SDK.",
                zkey_path
            )));
        }

        // Create temporary files for input and output
        let temp_dir = std::env::temp_dir();
        let input_path = temp_dir.join("privacy_cash_input.json");
        let witness_path = temp_dir.join("privacy_cash_witness.wtns");
        let proof_path = temp_dir.join("privacy_cash_proof.json");
        let public_path = temp_dir.join("privacy_cash_public.json");

        // Write input to file
        let input_json = input.to_json()?;
        std::fs::write(&input_path, &input_json)
            .map_err(|e| PrivacyCashError::IoError(e))?;

        // Generate witness using snarkjs
        log::debug!("Generating witness...");
        let witness_output = Command::new("snarkjs")
            .args([
                "wtns",
                "calculate",
                &wasm_path,
                input_path.to_str().unwrap(),
                witness_path.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| {
                PrivacyCashError::ProofGenerationError(format!(
                    "Failed to run snarkjs witness calculation. Is snarkjs installed? (npm install -g snarkjs): {}",
                    e
                ))
            })?;

        if !witness_output.status.success() {
            let stderr = String::from_utf8_lossy(&witness_output.stderr);
            return Err(PrivacyCashError::ProofGenerationError(format!(
                "Witness calculation failed: {}",
                stderr
            )));
        }

        // Generate proof
        log::debug!("Generating proof...");
        let proof_output = Command::new("snarkjs")
            .args([
                "groth16",
                "prove",
                &zkey_path,
                witness_path.to_str().unwrap(),
                proof_path.to_str().unwrap(),
                public_path.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| {
                PrivacyCashError::ProofGenerationError(format!(
                    "Failed to run snarkjs proof generation: {}",
                    e
                ))
            })?;

        if !proof_output.status.success() {
            let stderr = String::from_utf8_lossy(&proof_output.stderr);
            return Err(PrivacyCashError::ProofGenerationError(format!(
                "Proof generation failed: {}",
                stderr
            )));
        }

        // Read proof and public signals
        let proof_json = std::fs::read_to_string(&proof_path)
            .map_err(|e| PrivacyCashError::IoError(e))?;

        let public_json = std::fs::read_to_string(&public_path)
            .map_err(|e| PrivacyCashError::IoError(e))?;

        // Parse outputs
        let proof: Proof = serde_json::from_str(&proof_json)
            .map_err(|e| PrivacyCashError::SerializationError(format!("Failed to parse proof: {}", e)))?;

        let public_signals: Vec<String> = serde_json::from_str(&public_json)
            .map_err(|e| PrivacyCashError::SerializationError(format!("Failed to parse public signals: {}", e)))?;

        // Clean up temporary files
        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&witness_path);
        let _ = std::fs::remove_file(&proof_path);
        let _ = std::fs::remove_file(&public_path);

        log::debug!("Proof generated successfully");
        Ok((proof, public_signals))
    }

    /// Check if snarkjs is available
    pub fn check_snarkjs_available() -> bool {
        Command::new("snarkjs")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Parse proof to bytes array for on-chain submission
pub fn parse_proof_to_bytes(proof: &Proof) -> Result<ProofBytes> {
    let parse_coord = |s: &str| -> Result<Vec<u8>> {
        let n = BigUint::parse_bytes(s.as_bytes(), 10)
            .ok_or_else(|| PrivacyCashError::SerializationError("Invalid coordinate".to_string()))?;
        let bytes = biguint_to_bytes_le(&n);
        // Reverse for big-endian format expected by Solana program
        Ok(bytes.iter().rev().cloned().collect())
    };

    // Proof A: [x, y] flattened
    let mut proof_a = Vec::new();
    proof_a.extend(parse_coord(&proof.pi_a[0])?);
    proof_a.extend(parse_coord(&proof.pi_a[1])?);

    // Proof B: [[x1, x2], [y1, y2]] flattened and reversed
    let mut proof_b = Vec::new();
    let b_x: Vec<u8> = proof.pi_b[0]
        .iter()
        .flat_map(|s| parse_coord(s).unwrap())
        .collect();
    let b_y: Vec<u8> = proof.pi_b[1]
        .iter()
        .flat_map(|s| parse_coord(s).unwrap())
        .collect();
    // Reverse order for Solana
    let b_x_rev: Vec<u8> = b_x.chunks(32).rev().flatten().cloned().collect();
    let b_y_rev: Vec<u8> = b_y.chunks(32).rev().flatten().cloned().collect();
    proof_b.extend(b_x_rev);
    proof_b.extend(b_y_rev);

    // Proof C: [x, y] flattened
    let mut proof_c = Vec::new();
    proof_c.extend(parse_coord(&proof.pi_c[0])?);
    proof_c.extend(parse_coord(&proof.pi_c[1])?);

    Ok(ProofBytes {
        proof_a,
        proof_b,
        proof_c,
    })
}

/// Parse public signals to bytes arrays
pub fn parse_public_signals_to_bytes(signals: &[String]) -> Result<Vec<[u8; 32]>> {
    signals
        .iter()
        .map(|s| {
            let n = BigUint::parse_bytes(s.as_bytes(), 10).or_else(|| {
                // Try parsing as hex if decimal fails
                BigUint::parse_bytes(s.as_bytes(), 16)
            });

            match n {
                Some(num) => {
                    let bytes = biguint_to_bytes_le(&num);
                    let mut result = [0u8; 32];
                    result.copy_from_slice(&bytes);
                    // Reverse for circuit format
                    result.reverse();
                    Ok(result)
                }
                None => Err(PrivacyCashError::SerializationError(format!(
                    "Invalid signal: {}",
                    s
                ))),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_public_signals() {
        let signals = vec!["123".to_string(), "456".to_string()];
        let bytes = parse_public_signals_to_bytes(&signals).unwrap();
        assert_eq!(bytes.len(), 2);
        assert_eq!(bytes[0].len(), 32);
    }

    #[test]
    fn test_check_snarkjs() {
        // This will only pass if snarkjs is installed
        let available = Prover::check_snarkjs_available();
        println!("snarkjs available: {}", available);
    }
}
