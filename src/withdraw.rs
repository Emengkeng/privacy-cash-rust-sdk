//! Withdrawal functionality for native SOL

use crate::config::Config;
use crate::constants::{
    ALT_ADDRESS, FEE_RECIPIENT, LAMPORTS_PER_SOL, PROGRAM_ID,
    RELAYER_API_URL, TRANSACT_IX_DISCRIMINATOR,
};
use crate::encryption::EncryptionService;
use crate::error::{PrivacyCashError, Result};
use crate::get_utxos::get_utxos;
use crate::keypair::ZkKeypair;
use crate::merkle_tree::MerkleTree;
use crate::prover::{parse_proof_to_bytes, parse_public_signals_to_bytes, CircuitInput, Prover};
use crate::storage::Storage;
use crate::utxo::{Utxo, UtxoVersion};
use crate::utils::{
    calculate_public_amount, fetch_merkle_proof, find_cross_check_nullifier_pdas,
    find_nullifier_pdas, get_mint_address_field, get_program_accounts, query_remote_tree_state,
    ExtData,
};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::str::FromStr;

/// Withdrawal result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawResult {
    /// Transaction signature
    pub signature: String,

    /// Recipient address
    pub recipient: String,

    /// Amount withdrawn (after fees)
    pub amount_in_lamports: u64,

    /// Fee charged
    pub fee_in_lamports: u64,

    /// Whether this was a partial withdrawal
    pub is_partial: bool,
}

/// Parameters for withdrawal
pub struct WithdrawParams<'a> {
    pub connection: &'a RpcClient,
    pub keypair: &'a Keypair,
    pub encryption_service: &'a EncryptionService,
    pub storage: &'a Storage,
    pub amount_in_lamports: u64,
    pub recipient: &'a Pubkey,
    pub key_base_path: &'a str,
    pub referrer: Option<&'a str>,
}

/// Execute a withdrawal
pub async fn withdraw(params: WithdrawParams<'_>) -> Result<WithdrawResult> {
    let WithdrawParams {
        connection,
        keypair,
        encryption_service,
        storage,
        mut amount_in_lamports,
        recipient,
        key_base_path,
        referrer,
    } = params;

    let public_key = keypair.pubkey();

    // Get fee configuration
    let withdraw_fee_rate = Config::get_withdraw_fee_rate().await?;
    let withdraw_rent_fee = Config::get_withdraw_rent_fee().await?;

    let fee_in_lamports =
        (amount_in_lamports as f64 * withdraw_fee_rate + LAMPORTS_PER_SOL as f64 * withdraw_rent_fee)
            as u64;

    amount_in_lamports = amount_in_lamports.saturating_sub(fee_in_lamports);
    let mut is_partial = false;

    log::info!(
        "Starting withdrawal of {} lamports (fee: {})",
        amount_in_lamports,
        fee_in_lamports
    );

    let (tree_account, tree_token_account, global_config_account) = get_program_accounts();

    // Get tree state
    let tree_state = query_remote_tree_state(None).await?;

    // Get UTXO keypairs
    let utxo_private_key_v1 = encryption_service.get_utxo_private_key_v1()?;
    let utxo_keypair_v1 = ZkKeypair::from_hex(&utxo_private_key_v1)?;

    let utxo_private_key_v2 = encryption_service.get_utxo_private_key_v2()?;
    let utxo_keypair_v2 = ZkKeypair::from_hex(&utxo_private_key_v2)?;

    // Fetch existing UTXOs
    let mut unspent_utxos =
        get_utxos(connection, &public_key, encryption_service, storage, None).await?;

    if unspent_utxos.is_empty() {
        return Err(PrivacyCashError::NoUtxosAvailable);
    }

    // Sort by amount descending
    unspent_utxos.sort_by(|a, b| b.amount.cmp(&a.amount));

    let first_input = unspent_utxos[0].clone();
    let second_input = if unspent_utxos.len() > 1 {
        unspent_utxos[1].clone()
    } else {
        Utxo::dummy(utxo_keypair_v1.clone(), None)
    };

    let inputs = vec![first_input.clone(), second_input.clone()];
    let total_input_amount = first_input.amount.clone() + second_input.amount.clone();

    if total_input_amount.is_zero() {
        return Err(PrivacyCashError::NoUtxosAvailable);
    }

    // Check if partial withdrawal
    let required = BigUint::from(amount_in_lamports + fee_in_lamports);
    if total_input_amount < required {
        is_partial = true;
        amount_in_lamports = total_input_amount
            .to_u64_digits()
            .first()
            .copied()
            .unwrap_or(0)
            .saturating_sub(fee_in_lamports);
    }

    // Calculate change
    let change_amount = total_input_amount.clone()
        - BigUint::from(amount_in_lamports)
        - BigUint::from(fee_in_lamports);

    log::debug!(
        "Withdrawing {} with {} fee, {} change",
        amount_in_lamports,
        fee_in_lamports,
        change_amount
    );

    // Fetch Merkle proofs
    let input_merkle_paths = vec![
        if first_input.is_dummy() {
            MerkleTree::zero_path()
        } else {
            let commitment = first_input.get_commitment()?;
            fetch_merkle_proof(&commitment, None).await?
        },
        if second_input.is_dummy() {
            MerkleTree::zero_path()
        } else {
            let commitment = second_input.get_commitment()?;
            fetch_merkle_proof(&commitment, None).await?
        },
    ];

    // Create outputs with V2 keypair
    let outputs = vec![
        Utxo::new(
            change_amount,
            utxo_keypair_v2.clone(),
            tree_state.next_index,
            None,
            Some(UtxoVersion::V2),
        ),
        Utxo::new(
            0u64,
            utxo_keypair_v2.clone(),
            tree_state.next_index + 1,
            None,
            Some(UtxoVersion::V2),
        ),
    ];

    // For withdrawal, ext_amount is negative
    let ext_amount = -(amount_in_lamports as i64);
    let public_amount = calculate_public_amount(ext_amount, fee_in_lamports);

    // Generate nullifiers and commitments
    let input_nullifiers = vec![inputs[0].get_nullifier()?, inputs[1].get_nullifier()?];
    let output_commitments = vec![outputs[0].get_commitment()?, outputs[1].get_commitment()?];

    // Encrypt outputs
    let encrypted_output1 = encryption_service.encrypt_utxo(&outputs[0])?;
    let encrypted_output2 = encryption_service.encrypt_utxo(&outputs[1])?;

    // Create ExtData
    let sol_mint = Pubkey::from_str("11111111111111111111111111111112").unwrap();

    let ext_data = ExtData {
        recipient: *recipient,
        ext_amount,
        encrypted_output1: encrypted_output1.clone(),
        encrypted_output2: encrypted_output2.clone(),
        fee: fee_in_lamports,
        fee_recipient: *FEE_RECIPIENT,
        mint_address: sol_mint,
    };

    let ext_data_hash = ext_data.hash();

    // Build circuit input
    let circuit_input = CircuitInput {
        root: tree_state.root.clone(),
        input_nullifier: input_nullifiers.clone(),
        output_commitment: output_commitments.clone(),
        public_amount: public_amount.to_string(),
        ext_data_hash: ext_data_hash.to_vec(),

        in_amount: inputs.iter().map(|u| u.amount.to_string()).collect(),
        in_private_key: inputs.iter().map(|u| u.keypair.privkey().clone()).collect(),
        in_blinding: inputs.iter().map(|u| u.blinding.to_string()).collect(),
        in_path_indices: inputs.iter().map(|u| u.index).collect(),
        in_path_elements: input_merkle_paths
            .iter()
            .map(|p| p.path_elements.clone())
            .collect(),

        out_amount: outputs.iter().map(|u| u.amount.to_string()).collect(),
        out_blinding: outputs.iter().map(|u| u.blinding.to_string()).collect(),
        out_pubkey: outputs.iter().map(|u| u.keypair.pubkey().clone()).collect(),

        mint_address: get_mint_address_field(&sol_mint),
    };

    // Generate proof
    log::info!("Generating ZK proof...");
    let prover = Prover::new(key_base_path);
    let (proof, public_signals) = prover.prove(&circuit_input).await?;

    // Parse proof to bytes
    let proof_bytes = parse_proof_to_bytes(&proof)?;
    let signals_bytes = parse_public_signals_to_bytes(&public_signals)?;

    // Find nullifier PDAs
    let (nullifier0_pda, nullifier1_pda) =
        find_nullifier_pdas(&[signals_bytes[3], signals_bytes[4]]);
    let (nullifier2_pda, nullifier3_pda) =
        find_cross_check_nullifier_pdas(&[signals_bytes[3], signals_bytes[4]]);

    // Serialize proof
    let serialized_proof = serialize_withdraw_proof(&proof_bytes, &signals_bytes, &ext_data);

    // Build withdraw parameters for backend
    let withdraw_params = serde_json::json!({
        "serializedProof": base64::encode(&serialized_proof),
        "treeAccount": tree_account.to_string(),
        "nullifier0PDA": nullifier0_pda.to_string(),
        "nullifier1PDA": nullifier1_pda.to_string(),
        "nullifier2PDA": nullifier2_pda.to_string(),
        "nullifier3PDA": nullifier3_pda.to_string(),
        "treeTokenAccount": tree_token_account.to_string(),
        "globalConfigAccount": global_config_account.to_string(),
        "recipient": recipient.to_string(),
        "feeRecipientAccount": FEE_RECIPIENT.to_string(),
        "extAmount": ext_amount,
        "encryptedOutput1": base64::encode(&encrypted_output1),
        "encryptedOutput2": base64::encode(&encrypted_output2),
        "fee": fee_in_lamports,
        "lookupTableAddress": ALT_ADDRESS.to_string(),
        "senderAddress": public_key.to_string(),
        "referralWalletAddress": referrer
    });

    // Submit to backend
    log::info!("Submitting withdrawal to relayer...");
    let signature = submit_withdraw_to_indexer(withdraw_params).await?;

    // Wait for confirmation
    log::info!("Waiting for confirmation...");
    wait_for_confirmation(&encrypted_output1, None).await?;

    Ok(WithdrawResult {
        signature,
        recipient: recipient.to_string(),
        amount_in_lamports,
        fee_in_lamports,
        is_partial,
    })
}

/// Submit withdrawal to indexer backend
async fn submit_withdraw_to_indexer(params: serde_json::Value) -> Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/withdraw", *RELAYER_API_URL))
        .json(&params)
        .send()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Withdraw submit failed: {}", e)))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(PrivacyCashError::ApiError(format!(
            "Withdraw failed: {}",
            error_text
        )));
    }

    #[derive(Deserialize)]
    struct Response {
        signature: String,
    }

    let result: Response = response
        .json()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Parse response: {}", e)))?;

    Ok(result.signature)
}

/// Wait for transaction confirmation
async fn wait_for_confirmation(encrypted_output: &[u8], token_name: Option<&str>) -> Result<()> {
    let encrypted_hex = hex::encode(encrypted_output);
    let mut retries = 0;
    let max_retries = 10;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let mut url = format!("{}/utxos/check/{}", *RELAYER_API_URL, encrypted_hex);
        if let Some(token) = token_name {
            url = format!("{}?token={}", url, token);
        }

        let response = reqwest::get(&url).await;

        if let Ok(resp) = response {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if data.get("exists").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Ok(());
                }
            }
        }

        retries += 1;
        if retries >= max_retries {
            return Err(PrivacyCashError::ConfirmationTimeout { retries });
        }

        log::info!("Confirming transaction... (retry {})", retries);
    }
}

/// Serialize withdrawal proof
fn serialize_withdraw_proof(
    proof_bytes: &crate::prover::ProofBytes,
    signals: &[[u8; 32]],
    ext_data: &ExtData,
) -> Vec<u8> {
    let mut data = Vec::new();

    // Discriminator
    data.extend_from_slice(&TRANSACT_IX_DISCRIMINATOR);

    // Proof
    data.extend_from_slice(&proof_bytes.proof_a);
    data.extend_from_slice(&proof_bytes.proof_b);
    data.extend_from_slice(&proof_bytes.proof_c);

    // Public signals
    for signal in signals.iter().take(7) {
        data.extend_from_slice(signal);
    }

    // ExtData (minified)
    data.extend_from_slice(&ext_data.ext_amount.to_le_bytes());
    data.extend_from_slice(&ext_data.fee.to_le_bytes());

    // Encrypted outputs
    data.extend_from_slice(&(ext_data.encrypted_output1.len() as u32).to_le_bytes());
    data.extend_from_slice(&ext_data.encrypted_output1);
    data.extend_from_slice(&(ext_data.encrypted_output2.len() as u32).to_le_bytes());
    data.extend_from_slice(&ext_data.encrypted_output2);

    data
}

// Re-export BigUint conversion for withdraw
use num_traits::ToPrimitive;
trait BigUintExt {
    fn to_u64_safe(&self) -> u64;
}

impl BigUintExt for BigUint {
    fn to_u64_safe(&self) -> u64 {
        self.to_u64().unwrap_or(0)
    }
}
