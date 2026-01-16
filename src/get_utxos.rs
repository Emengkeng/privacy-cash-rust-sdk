//! UTXO fetching and management for native SOL

use crate::constants::{
    FETCH_UTXOS_GROUP_SIZE, LSK_ENCRYPTED_OUTPUTS, LSK_FETCH_OFFSET, PROGRAM_ID, RELAYER_API_URL,
};
use crate::encryption::EncryptionService;
use crate::error::{PrivacyCashError, Result};
use crate::storage::Storage;
use crate::utxo::{get_balance_from_utxos, Balance, Utxo};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::Mutex;

/// API response for UTXOs
#[derive(Debug, Deserialize)]
struct ApiUtxo {
    commitment: String,
    encrypted_output: String,
    index: u64,
    #[serde(default)]
    nullifier: Option<String>,
}

/// API response format with encrypted outputs
#[derive(Debug, Deserialize)]
struct ApiResponse {
    count: u64,
    encrypted_outputs: Vec<String>,
    #[serde(default)]
    total: u64,
    #[serde(rename = "hasMore", default)]
    has_more: bool,
}

/// Request for fetching UTXO indices
#[derive(Debug, Serialize)]
struct IndicesRequest {
    encrypted_outputs: Vec<String>,
}

/// Response for UTXO indices
#[derive(Debug, Deserialize)]
struct IndicesResponse {
    indices: Vec<u64>,
}

/// Create a storage key for a public key
pub fn localstorage_key(pubkey: &Pubkey) -> String {
    let program_prefix = PROGRAM_ID.to_string();
    let prefix = &program_prefix[..6.min(program_prefix.len())];
    format!("{}{}", prefix, pubkey)
}

/// Fetch all UTXOs for a user
pub async fn get_utxos(
    connection: &RpcClient,
    public_key: &Pubkey,
    encryption_service: &EncryptionService,
    storage: &Storage,
    abort_signal: Option<Arc<Mutex<bool>>>,
) -> Result<Vec<Utxo>> {
    let mut valid_utxos = Vec::new();
    let mut valid_strings = Vec::new();
    let mut history_indexes = Vec::new();

    let storage_key = localstorage_key(public_key);

    // Get starting offset from storage
    let mut round_start_index: u64 = storage
        .get(&format!("{}{}", LSK_FETCH_OFFSET, storage_key))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    loop {
        // Check for abort
        if let Some(ref signal) = abort_signal {
            if *signal.lock().await {
                return Err(PrivacyCashError::Aborted);
            }
        }

        let fetch_offset: u64 = storage
            .get(&format!("{}{}", LSK_FETCH_OFFSET, storage_key))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
            .max(round_start_index);

        let fetch_end = fetch_offset + FETCH_UTXOS_GROUP_SIZE;
        let url = format!(
            "{}/utxos/range?start={}&end={}",
            *RELAYER_API_URL, fetch_offset, fetch_end
        );

        log::debug!("Fetching UTXOs from: {}", url);

        let (fetched_utxos, encrypted_outputs, has_more, len) =
            fetch_user_utxos(&url, encryption_service, storage, &storage_key).await?;

        // Check which UTXOs are unspent
        let non_zero_utxos: Vec<_> = fetched_utxos
            .iter()
            .enumerate()
            .filter(|(_, u)| u.amount_u64() > 0)
            .collect();

        if !non_zero_utxos.is_empty() {
            let spent_flags = are_utxos_spent(
                connection,
                &non_zero_utxos.iter().map(|(_, u)| (*u).clone()).collect::<Vec<_>>(),
            )
            .await?;

            for ((idx, utxo), is_spent) in non_zero_utxos.into_iter().zip(spent_flags) {
                history_indexes.push(utxo.index);
                if !is_spent {
                    log::debug!("Found unspent UTXO: {:?}", encrypted_outputs.get(idx));
                    valid_utxos.push(utxo.clone());
                    if let Some(enc) = encrypted_outputs.get(idx) {
                        valid_strings.push(enc.clone());
                    }
                }
            }
        }

        // Update storage offset
        storage.set(
            &format!("{}{}", LSK_FETCH_OFFSET, storage_key),
            &(fetch_offset + len).to_string(),
        );

        if !has_more {
            break;
        }

        // Small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    // Store valid encrypted outputs
    let unique_strings: Vec<_> = valid_strings
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    storage.set(
        &format!("{}{}", LSK_ENCRYPTED_OUTPUTS, storage_key),
        &serde_json::to_string(&unique_strings).unwrap_or_default(),
    );

    Ok(valid_utxos)
}

/// Fetch UTXOs from API and decrypt
async fn fetch_user_utxos(
    url: &str,
    encryption_service: &EncryptionService,
    storage: &Storage,
    storage_key: &str,
) -> Result<(Vec<Utxo>, Vec<String>, bool, u64)> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to fetch UTXOs: {}", e)))?;

    if !response.status().is_success() {
        return Err(PrivacyCashError::ApiError(format!(
            "UTXO API returned status: {}",
            response.status()
        )));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to parse UTXOs: {}", e)))?;

    let (encrypted_outputs, has_more, total) = if let Some(outputs) = data.get("encrypted_outputs") {
        let outputs: Vec<String> = serde_json::from_value(outputs.clone()).unwrap_or_default();
        let has_more = data.get("hasMore").and_then(|v| v.as_bool()).unwrap_or(false);
        let total = data.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        (outputs, has_more, total)
    } else if data.is_array() {
        let utxos: Vec<ApiUtxo> = serde_json::from_value(data.clone()).unwrap_or_default();
        let outputs: Vec<String> = utxos
            .into_iter()
            .filter_map(|u| {
                if u.encrypted_output.is_empty() {
                    None
                } else {
                    Some(u.encrypted_output)
                }
            })
            .collect();
        let len = outputs.len() as u64;
        (outputs, false, len)
    } else {
        return Err(PrivacyCashError::ApiError("Unexpected API response format".to_string()));
    };

    let len = encrypted_outputs.len() as u64;

    // Decrypt outputs
    let (utxos, decrypted_outputs) =
        decrypt_outputs(&encrypted_outputs, encryption_service, None).await?;

    // Also check cached outputs if no more to fetch
    let mut all_utxos = utxos;
    let mut all_outputs = decrypted_outputs;

    if !has_more {
        if let Some(cached) = storage.get(&format!("{}{}", LSK_ENCRYPTED_OUTPUTS, storage_key)) {
            if let Ok(cached_outputs) = serde_json::from_str::<Vec<String>>(&cached) {
                let (cached_utxos, cached_decrypted) =
                    decrypt_outputs(&cached_outputs, encryption_service, None).await?;
                all_utxos.extend(cached_utxos);
                all_outputs.extend(cached_decrypted);
            }
        }
    }

    Ok((all_utxos, all_outputs, has_more, len))
}

/// Decrypt encrypted outputs
async fn decrypt_outputs(
    encrypted_outputs: &[String],
    encryption_service: &EncryptionService,
    token_name: Option<&str>,
) -> Result<(Vec<Utxo>, Vec<String>)> {
    let mut utxos = Vec::new();
    let mut outputs = Vec::new();

    for encrypted in encrypted_outputs {
        if encrypted.is_empty() {
            continue;
        }

        match encryption_service.decrypt_utxo_from_hex(encrypted) {
            Ok(utxo) => {
                utxos.push(utxo);
                outputs.push(encrypted.clone());
            }
            Err(_) => {
                // UTXO doesn't belong to this user, skip
                continue;
            }
        }
    }

    // Fetch real indices for decrypted UTXOs
    if !outputs.is_empty() {
        let indices = fetch_utxo_indices(&outputs, token_name).await?;
        for (utxo, index) in utxos.iter_mut().zip(indices) {
            if utxo.index != index {
                log::debug!("Updated UTXO index from {} to {}", utxo.index, index);
                utxo.index = index;
            }
        }
    }

    Ok((utxos, outputs))
}

/// Fetch UTXO indices from API
async fn fetch_utxo_indices(encrypted_outputs: &[String], token_name: Option<&str>) -> Result<Vec<u64>> {
    let mut url = format!("{}/utxos/indices", *RELAYER_API_URL);

    let body = if let Some(token) = token_name {
        serde_json::json!({
            "encrypted_outputs": encrypted_outputs,
            "token": token
        })
    } else {
        serde_json::json!({
            "encrypted_outputs": encrypted_outputs
        })
    };

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to fetch indices: {}", e)))?;

    if !response.status().is_success() {
        return Err(PrivacyCashError::ApiError(format!(
            "Indices API returned status: {}",
            response.status()
        )));
    }

    let data: IndicesResponse = response
        .json()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to parse indices: {}", e)))?;

    Ok(data.indices)
}

/// Check if UTXOs are spent
async fn are_utxos_spent(connection: &RpcClient, utxos: &[Utxo]) -> Result<Vec<bool>> {
    let mut all_pdas = Vec::new();

    for (i, utxo) in utxos.iter().enumerate() {
        let nullifier = utxo.get_nullifier()?;
        let nullifier_bytes = string_to_nullifier_bytes(&nullifier)?;

        let (nullifier0_pda, _) =
            Pubkey::find_program_address(&[b"nullifier0", &nullifier_bytes], &PROGRAM_ID);
        let (nullifier1_pda, _) =
            Pubkey::find_program_address(&[b"nullifier1", &nullifier_bytes], &PROGRAM_ID);

        all_pdas.push((i, nullifier0_pda));
        all_pdas.push((i, nullifier1_pda));
    }

    // Batch fetch account info
    let pubkeys: Vec<Pubkey> = all_pdas.iter().map(|(_, p)| *p).collect();

    let accounts = connection
        .get_multiple_accounts(&pubkeys)
        .map_err(|e| PrivacyCashError::SolanaClientError(e))?;

    let mut spent_flags = vec![false; utxos.len()];

    for ((utxo_idx, _), account) in all_pdas.iter().zip(accounts.iter()) {
        if account.is_some() {
            spent_flags[*utxo_idx] = true;
        }
    }

    Ok(spent_flags)
}

/// Check if a single UTXO is spent
pub async fn is_utxo_spent(connection: &RpcClient, utxo: &Utxo) -> Result<bool> {
    let result = are_utxos_spent(connection, &[utxo.clone()]).await?;
    Ok(result.first().copied().unwrap_or(false))
}

/// Convert nullifier string to bytes
fn string_to_nullifier_bytes(nullifier: &str) -> Result<[u8; 32]> {
    let n = BigUint::parse_bytes(nullifier.as_bytes(), 10)
        .ok_or_else(|| PrivacyCashError::SerializationError("Invalid nullifier".to_string()))?;

    let bytes = n.to_bytes_le();
    let mut result = [0u8; 32];
    let len = bytes.len().min(32);
    result[..len].copy_from_slice(&bytes[..len]);

    // Reverse for on-chain format
    result.reverse();
    Ok(result)
}

/// Get private balance from UTXOs
pub async fn get_private_balance(
    connection: &RpcClient,
    public_key: &Pubkey,
    encryption_service: &EncryptionService,
    storage: &Storage,
) -> Result<Balance> {
    let utxos = get_utxos(connection, public_key, encryption_service, storage, None).await?;
    Ok(get_balance_from_utxos(&utxos))
}
