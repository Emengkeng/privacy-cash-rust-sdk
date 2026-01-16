//! Withdrawal functionality for SPL tokens

use crate::config::Config;
use crate::constants::{
    find_token_by_mint, ALT_ADDRESS, FEE_RECIPIENT, PROGRAM_ID, RELAYER_API_URL,
    TRANSACT_SPL_IX_DISCRIMINATOR,
};
use crate::encryption::EncryptionService;
use crate::error::{PrivacyCashError, Result};
use crate::get_utxos_spl::get_utxos_spl;
use crate::keypair::ZkKeypair;
use crate::merkle_tree::MerkleTree;
use crate::prover::{parse_proof_to_bytes, parse_public_signals_to_bytes, CircuitInput, Prover};
use crate::storage::Storage;
use crate::utxo::{Utxo, UtxoVersion};
use crate::utils::{
    calculate_public_amount, fetch_merkle_proof, find_cross_check_nullifier_pdas,
    find_nullifier_pdas, get_mint_address_field, get_program_accounts, get_spl_tree_account,
    query_remote_tree_state, ExtData,
};
use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use spl_associated_token_account::get_associated_token_address;

/// SPL Withdrawal result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawSplResult {
    pub signature: String,
    pub recipient: String,
    pub base_units: u64,
    pub fee_base_units: u64,
    pub is_partial: bool,
}

/// Parameters for SPL withdrawal
pub struct WithdrawSplParams<'a> {
    pub connection: &'a RpcClient,
    pub keypair: &'a Keypair,
    pub encryption_service: &'a EncryptionService,
    pub storage: &'a Storage,
    pub base_units: u64,
    pub mint_address: &'a Pubkey,
    pub recipient: &'a Pubkey,
    pub key_base_path: &'a str,
    pub referrer: Option<&'a str>,
}

/// Execute an SPL token withdrawal
pub async fn withdraw_spl(params: WithdrawSplParams<'_>) -> Result<WithdrawSplResult> {
    let WithdrawSplParams {
        connection,
        keypair,
        encryption_service,
        storage,
        mut base_units,
        mint_address,
        recipient,
        key_base_path,
        referrer,
    } = params;

    let token = find_token_by_mint(mint_address)
        .ok_or_else(|| PrivacyCashError::TokenNotSupported(mint_address.to_string()))?;

    log::info!(
        "Starting {} withdrawal of {} base units",
        token.name,
        base_units
    );

    let public_key = keypair.pubkey();

    // Get fee configuration
    let withdraw_fee_rate = Config::get_withdraw_fee_rate().await?;
    let token_rent_fee = Config::get_token_rent_fee(token.name).await?;

    let fee_base_units =
        (base_units as f64 * withdraw_fee_rate + token.units_per_token as f64 * token_rent_fee)
            as u64;

    base_units = base_units.saturating_sub(fee_base_units);
    let mut is_partial = false;

    if base_units == 0 {
        return Err(PrivacyCashError::WithdrawalAmountTooLow {
            minimum: fee_base_units,
        });
    }

    // Get token accounts
    let recipient_ata = get_associated_token_address(recipient, mint_address);
    let fee_recipient_token_account = get_associated_token_address(&FEE_RECIPIENT, mint_address);

    // Get tree account
    let tree_account = get_spl_tree_account(mint_address);
    let (_, tree_token_account, global_config_account) = get_program_accounts();

    // Get global config for tree ATA
    let (global_config_pda, _) = Pubkey::find_program_address(&[b"global_config"], &PROGRAM_ID);
    let tree_ata = get_associated_token_address(&global_config_pda, mint_address);

    // Get tree state
    let tree_state = query_remote_tree_state(Some(token.name)).await?;

    // Get UTXO keypairs
    let utxo_private_key_v1 = encryption_service.get_utxo_private_key_v1()?;
    let utxo_keypair_v1 = ZkKeypair::from_hex(&utxo_private_key_v1)?;

    let utxo_private_key_v2 = encryption_service.get_utxo_private_key_v2()?;
    let utxo_keypair_v2 = ZkKeypair::from_hex(&utxo_private_key_v2)?;

    // Fetch existing UTXOs
    let mut unspent_utxos = get_utxos_spl(
        connection,
        &public_key,
        encryption_service,
        storage,
        mint_address,
        None,
    )
    .await?;

    if unspent_utxos.is_empty() {
        return Err(PrivacyCashError::NoUtxosAvailable);
    }

    // Sort by amount descending
    unspent_utxos.sort_by(|a, b| b.amount.cmp(&a.amount));

    let first_input = unspent_utxos[0].clone();
    let second_input = if unspent_utxos.len() > 1 {
        unspent_utxos[1].clone()
    } else {
        Utxo::dummy(utxo_keypair_v1.clone(), Some(&mint_address.to_string()))
    };

    let inputs = vec![first_input.clone(), second_input.clone()];
    let total_input_amount = first_input.amount.clone() + second_input.amount.clone();

    if total_input_amount.is_zero() {
        return Err(PrivacyCashError::NoUtxosAvailable);
    }

    // Check if partial withdrawal
    let required = BigUint::from(base_units + fee_base_units);
    if total_input_amount < required {
        is_partial = true;
        base_units = total_input_amount
            .to_u64()
            .unwrap_or(0)
            .saturating_sub(fee_base_units);
    }

    let change_amount =
        total_input_amount.clone() - BigUint::from(base_units) - BigUint::from(fee_base_units);

    log::debug!(
        "Withdrawing {} with {} fee, {} change",
        base_units,
        fee_base_units,
        change_amount
    );

    // Fetch Merkle proofs
    let input_merkle_paths = vec![
        if first_input.is_dummy() {
            MerkleTree::zero_path()
        } else {
            let commitment = first_input.get_commitment()?;
            fetch_merkle_proof(&commitment, Some(token.name)).await?
        },
        if second_input.is_dummy() {
            MerkleTree::zero_path()
        } else {
            let commitment = second_input.get_commitment()?;
            fetch_merkle_proof(&commitment, Some(token.name)).await?
        },
    ];

    // Create outputs with V2 keypair
    let outputs = vec![
        Utxo::new(
            change_amount,
            utxo_keypair_v2.clone(),
            tree_state.next_index,
            Some(&mint_address.to_string()),
            Some(UtxoVersion::V2),
        ),
        Utxo::new(
            0u64,
            utxo_keypair_v2.clone(),
            tree_state.next_index + 1,
            Some(&mint_address.to_string()),
            Some(UtxoVersion::V2),
        ),
    ];

    let ext_amount = -(base_units as i64);
    let public_amount = calculate_public_amount(ext_amount, fee_base_units);

    let input_nullifiers = vec![inputs[0].get_nullifier()?, inputs[1].get_nullifier()?];
    let output_commitments = vec![outputs[0].get_commitment()?, outputs[1].get_commitment()?];

    let encrypted_output1 = encryption_service.encrypt_utxo(&outputs[0])?;
    let encrypted_output2 = encryption_service.encrypt_utxo(&outputs[1])?;

    let ext_data = ExtData {
        recipient: recipient_ata,
        ext_amount,
        encrypted_output1: encrypted_output1.clone(),
        encrypted_output2: encrypted_output2.clone(),
        fee: fee_base_units,
        fee_recipient: fee_recipient_token_account,
        mint_address: *mint_address,
    };

    let ext_data_hash = ext_data.hash();

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

        mint_address: get_mint_address_field(mint_address),
    };

    log::info!("Generating ZK proof...");
    let prover = Prover::new(key_base_path);
    let (proof, public_signals) = prover.prove(&circuit_input).await?;

    let proof_bytes = parse_proof_to_bytes(&proof)?;
    let signals_bytes = parse_public_signals_to_bytes(&public_signals)?;

    let (nullifier0_pda, nullifier1_pda) =
        find_nullifier_pdas(&[signals_bytes[3], signals_bytes[4]]);
    let (nullifier2_pda, nullifier3_pda) =
        find_cross_check_nullifier_pdas(&[signals_bytes[3], signals_bytes[4]]);

    let serialized_proof = serialize_spl_proof(&proof_bytes, &signals_bytes, &ext_data);

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
        "fee": fee_base_units,
        "lookupTableAddress": ALT_ADDRESS.to_string(),
        "senderAddress": public_key.to_string(),
        "treeAta": tree_ata.to_string(),
        "recipientAta": recipient_ata.to_string(),
        "mintAddress": mint_address.to_string(),
        "feeRecipientTokenAccount": fee_recipient_token_account.to_string(),
        "referralWalletAddress": referrer
    });

    log::info!("Submitting SPL withdrawal to relayer...");
    let signature = submit_spl_withdraw_to_indexer(withdraw_params).await?;

    log::info!("Waiting for confirmation...");
    wait_for_spl_confirmation(&encrypted_output1, token.name).await?;

    Ok(WithdrawSplResult {
        signature,
        recipient: recipient.to_string(),
        base_units,
        fee_base_units,
        is_partial,
    })
}

fn serialize_spl_proof(
    proof_bytes: &crate::prover::ProofBytes,
    signals: &[[u8; 32]],
    ext_data: &ExtData,
) -> Vec<u8> {
    let mut data = Vec::new();

    data.extend_from_slice(&TRANSACT_SPL_IX_DISCRIMINATOR);

    data.extend_from_slice(&proof_bytes.proof_a);
    data.extend_from_slice(&proof_bytes.proof_b);
    data.extend_from_slice(&proof_bytes.proof_c);

    for signal in signals.iter().take(7) {
        data.extend_from_slice(signal);
    }

    data.extend_from_slice(&ext_data.ext_amount.to_le_bytes());
    data.extend_from_slice(&ext_data.fee.to_le_bytes());

    data.extend_from_slice(&(ext_data.encrypted_output1.len() as u32).to_le_bytes());
    data.extend_from_slice(&ext_data.encrypted_output1);
    data.extend_from_slice(&(ext_data.encrypted_output2.len() as u32).to_le_bytes());
    data.extend_from_slice(&ext_data.encrypted_output2);

    data
}

async fn submit_spl_withdraw_to_indexer(params: serde_json::Value) -> Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/withdraw/spl", *RELAYER_API_URL))
        .json(&params)
        .send()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("SPL withdraw submit failed: {}", e)))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(PrivacyCashError::ApiError(format!(
            "SPL withdraw failed: {}",
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

async fn wait_for_spl_confirmation(encrypted_output: &[u8], token_name: &str) -> Result<()> {
    let encrypted_hex = hex::encode(encrypted_output);
    let mut retries = 0;
    let max_retries = 10;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let url = format!(
            "{}/utxos/check/{}?token={}",
            *RELAYER_API_URL, encrypted_hex, token_name
        );

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

        log::info!("Confirming SPL transaction... (retry {})", retries);
    }
}
