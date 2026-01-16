//! Deposit functionality for native SOL

use crate::constants::{
    FEE_RECIPIENT, PROGRAM_ID, TRANSACT_IX_DISCRIMINATOR,
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
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
};
use std::str::FromStr;

/// Deposit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositResult {
    /// Transaction signature
    pub signature: String,
}

/// Parameters for deposit
pub struct DepositParams<'a> {
    pub connection: &'a RpcClient,
    pub keypair: &'a Keypair,
    pub encryption_service: &'a EncryptionService,
    pub storage: &'a Storage,
    pub amount_in_lamports: u64,
    pub key_base_path: &'a str,
    pub referrer: Option<&'a str>,
}

/// Execute a deposit
pub async fn deposit(params: DepositParams<'_>) -> Result<DepositResult> {
    let DepositParams {
        connection,
        keypair,
        encryption_service,
        storage,
        amount_in_lamports,
        key_base_path,
        referrer,
    } = params;

    let public_key = keypair.pubkey();
    let fee_amount = 0u64; // No deposit fee

    log::info!("Starting deposit of {} lamports", amount_in_lamports);

    // Check deposit limit
    let limit = check_deposit_limit(connection).await?;
    if let Some(max_lamports) = limit {
        if amount_in_lamports > max_lamports {
            return Err(PrivacyCashError::DepositLimitExceeded {
                amount: amount_in_lamports,
                limit: max_lamports,
            });
        }
    }

    // Check balance
    let balance = connection.get_balance(&public_key)?;
    if balance < amount_in_lamports + fee_amount {
        return Err(PrivacyCashError::InsufficientBalance {
            have: balance,
            need: amount_in_lamports + fee_amount,
        });
    }

    let (tree_account, tree_token_account, global_config_account) = get_program_accounts();

    // Get tree state
    let tree_state = query_remote_tree_state(None).await?;

    log::debug!(
        "Tree state: root={}, nextIndex={}",
        tree_state.root,
        tree_state.next_index
    );

    // Get UTXO keypair
    let utxo_private_key = encryption_service.get_utxo_private_key_v2()?;
    let utxo_keypair = ZkKeypair::from_hex(&utxo_private_key)?;

    // Fetch existing UTXOs
    let existing_utxos = get_utxos(connection, &public_key, encryption_service, storage, None).await?;

    // Build inputs and calculate amounts
    let (inputs, input_merkle_paths, ext_amount, output_amount) = if existing_utxos.is_empty() {
        // Fresh deposit - use dummy inputs
        let inputs = vec![
            Utxo::dummy(utxo_keypair.clone(), None),
            Utxo::dummy(utxo_keypair.clone(), None),
        ];
        let paths = vec![MerkleTree::zero_path(), MerkleTree::zero_path()];

        let ext_amount = amount_in_lamports as i64;
        let output_amount = BigUint::from(amount_in_lamports) - BigUint::from(fee_amount);

        (inputs, paths, ext_amount, output_amount)
    } else {
        // Consolidate with existing UTXOs
        let first_utxo = &existing_utxos[0];
        let second_utxo = if existing_utxos.len() > 1 {
            existing_utxos[1].clone()
        } else {
            Utxo::dummy(utxo_keypair.clone(), None)
        };

        // Fetch Merkle proofs
        let first_commitment = first_utxo.get_commitment()?;
        let first_proof = fetch_merkle_proof(&first_commitment, None).await?;

        let second_proof = if !second_utxo.is_dummy() {
            let second_commitment = second_utxo.get_commitment()?;
            fetch_merkle_proof(&second_commitment, None).await?
        } else {
            MerkleTree::zero_path()
        };

        let ext_amount = amount_in_lamports as i64;
        let output_amount = first_utxo.amount.clone()
            + second_utxo.amount.clone()
            + BigUint::from(amount_in_lamports)
            - BigUint::from(fee_amount);

        (
            vec![first_utxo.clone(), second_utxo],
            vec![first_proof, second_proof],
            ext_amount,
            output_amount,
        )
    };

    let public_amount = calculate_public_amount(ext_amount, fee_amount);

    // Create outputs
    let outputs = vec![
        Utxo::new(
            output_amount,
            utxo_keypair.clone(),
            tree_state.next_index,
            None,
            Some(UtxoVersion::V2),
        ),
        Utxo::new(
            0u64,
            utxo_keypair.clone(),
            tree_state.next_index + 1,
            None,
            Some(UtxoVersion::V2),
        ),
    ];

    // Generate nullifiers and commitments
    let input_nullifiers = vec![inputs[0].get_nullifier()?, inputs[1].get_nullifier()?];
    let output_commitments = vec![outputs[0].get_commitment()?, outputs[1].get_commitment()?];

    // Encrypt outputs
    let encrypted_output1 = encryption_service.encrypt_utxo(&outputs[0])?;
    let encrypted_output2 = encryption_service.encrypt_utxo(&outputs[1])?;

    // Create ExtData
    let recipient = Pubkey::from_str("AWexibGxNFKTa1b5R5MN4PJr9HWnWRwf8EW9g8cLx3dM").unwrap();
    let sol_mint = Pubkey::from_str("11111111111111111111111111111112").unwrap();

    let ext_data = ExtData {
        recipient,
        ext_amount,
        encrypted_output1: encrypted_output1.clone(),
        encrypted_output2: encrypted_output2.clone(),
        fee: fee_amount,
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

    // Serialize instruction data
    let instruction_data = serialize_deposit_instruction(
        &proof_bytes,
        &signals_bytes,
        &ext_data,
    );

    // Serialize instruction data for backend relay BEFORE moving it
    use base64::Engine;
    let serialized = base64::engine::general_purpose::STANDARD.encode(&instruction_data);

    // Build transaction (for reference, not used directly as we relay to backend)
    let _deposit_instruction = Instruction {
        program_id: *PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(tree_account, false),
            AccountMeta::new(nullifier0_pda, false),
            AccountMeta::new(nullifier1_pda, false),
            AccountMeta::new_readonly(nullifier2_pda, false),
            AccountMeta::new_readonly(nullifier3_pda, false),
            AccountMeta::new(tree_token_account, false),
            AccountMeta::new_readonly(global_config_account, false),
            AccountMeta::new(recipient, false),
            AccountMeta::new(*FEE_RECIPIENT, false),
            AccountMeta::new(public_key, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: instruction_data,
    };

    let _compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_000_000);

    log::info!("Submitting transaction to relayer...");
    let signature = relay_deposit_to_indexer(&serialized, &public_key, referrer).await?;

    // Wait for confirmation
    log::info!("Waiting for confirmation...");
    wait_for_confirmation(&encrypted_output1, None).await?;

    Ok(DepositResult { signature })
}

/// Relay deposit to indexer backend
async fn relay_deposit_to_indexer(
    signed_transaction: &str,
    sender: &Pubkey,
    referrer: Option<&str>,
) -> Result<String> {
    use crate::constants::RELAYER_API_URL;

    let mut body = serde_json::json!({
        "signedTransaction": signed_transaction,
        "senderAddress": sender.to_string()
    });

    if let Some(ref_addr) = referrer {
        body["referralWalletAddress"] = serde_json::Value::String(ref_addr.to_string());
    }

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/deposit", *RELAYER_API_URL))
        .json(&body)
        .send()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Relay failed: {}", e)))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(PrivacyCashError::ApiError(format!(
            "Deposit relay failed: {}",
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
    use crate::constants::RELAYER_API_URL;

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

/// Check deposit limit from on-chain account
async fn check_deposit_limit(connection: &RpcClient) -> Result<Option<u64>> {
    let (tree_account, _, _) = get_program_accounts();

    let account = connection.get_account(&tree_account)?;

    // Parse max deposit amount from account data
    // Offset 4120-4128 contains max_deposit_amount
    if account.data.len() >= 4128 {
        let max_deposit = u64::from_le_bytes(
            account.data[4120..4128]
                .try_into()
                .map_err(|_| PrivacyCashError::SerializationError("Invalid account data".to_string()))?,
        );
        return Ok(Some(max_deposit));
    }

    Ok(None)
}

/// Serialize deposit instruction data
fn serialize_deposit_instruction(
    proof_bytes: &crate::prover::ProofBytes,
    signals: &[[u8; 32]],
    ext_data: &ExtData,
) -> Vec<u8> {
    use num_bigint::BigInt;
    use num_traits::ToPrimitive;

    let mut data = Vec::new();

    // Discriminator
    data.extend_from_slice(&TRANSACT_IX_DISCRIMINATOR);

    // Proof
    data.extend_from_slice(&proof_bytes.proof_a);
    data.extend_from_slice(&proof_bytes.proof_b);
    data.extend_from_slice(&proof_bytes.proof_c);

    // Public signals: root, publicAmount, extDataHash, nullifiers, commitments
    for signal in signals.iter().take(7) {
        data.extend_from_slice(signal);
    }

    // ExtData (minified): extAmount (i64), fee (u64)
    data.extend_from_slice(&ext_data.ext_amount.to_le_bytes());
    data.extend_from_slice(&ext_data.fee.to_le_bytes());

    // Encrypted outputs with length prefixes
    data.extend_from_slice(&(ext_data.encrypted_output1.len() as u32).to_le_bytes());
    data.extend_from_slice(&ext_data.encrypted_output1);
    data.extend_from_slice(&(ext_data.encrypted_output2.len() as u32).to_le_bytes());
    data.extend_from_slice(&ext_data.encrypted_output2);

    data
}
