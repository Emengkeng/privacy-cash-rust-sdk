//! UTXO fetching and management for SPL tokens

use crate::constants::{
    find_token_by_mint, FETCH_UTXOS_GROUP_SIZE, LSK_ENCRYPTED_OUTPUTS, LSK_FETCH_OFFSET,
    PROGRAM_ID, RELAYER_API_URL,
};
use crate::encryption::EncryptionService;
use crate::error::{PrivacyCashError, Result};
use crate::get_utxos::localstorage_key;
use crate::storage::Storage;
use crate::utxo::{get_balance_from_utxos_spl, SplBalance, Utxo};
use num_bigint::BigUint;
use serde::Deserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Fetch all SPL token UTXOs for a user
pub async fn get_utxos_spl(
    connection: &RpcClient,
    public_key: &Pubkey,
    encryption_service: &EncryptionService,
    storage: &Storage,
    mint_address: &Pubkey,
    abort_signal: Option<Arc<Mutex<bool>>>,
) -> Result<Vec<Utxo>> {
    let token = find_token_by_mint(mint_address)
        .ok_or_else(|| PrivacyCashError::TokenNotSupported(mint_address.to_string()))?;

    log::debug!("Fetching UTXOs for token: {}", token.name);

    // Get associated token address
    let ata = get_associated_token_address(public_key, mint_address);
    let storage_key = localstorage_key(&ata);

    let mut valid_utxos = Vec::new();
    let mut valid_strings = Vec::new();

    // Get starting offset from storage
    let round_start_index: u64 = storage
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
            "{}/utxos/range?token={}&start={}&end={}",
            *RELAYER_API_URL, token.name, fetch_offset, fetch_end
        );

        log::debug!("Fetching SPL UTXOs from: {}", url);

        let (fetched_utxos, encrypted_outputs, has_more, len) =
            fetch_user_utxos_spl(&url, encryption_service, storage, &storage_key, token.name)
                .await?;

        // Check which UTXOs are unspent
        let non_zero_utxos: Vec<_> = fetched_utxos
            .iter()
            .enumerate()
            .filter(|(_, u)| u.amount_u64() > 0)
            .collect();

        if !non_zero_utxos.is_empty() {
            let spent_flags = are_utxos_spent_spl(
                connection,
                &non_zero_utxos
                    .iter()
                    .map(|(_, u)| (*u).clone())
                    .collect::<Vec<_>>(),
            )
            .await?;

            for ((idx, utxo), is_spent) in non_zero_utxos.into_iter().zip(spent_flags) {
                if !is_spent {
                    log::debug!("Found unspent SPL UTXO: {:?}", encrypted_outputs.get(idx));
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

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
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

    // Filter UTXOs to only include those matching the mint address
    let filtered_utxos: Vec<_> = valid_utxos
        .into_iter()
        .filter(|u| u.mint_address == mint_address.to_string())
        .collect();

    Ok(filtered_utxos)
}

/// Fetch SPL UTXOs from API and decrypt
async fn fetch_user_utxos_spl(
    url: &str,
    encryption_service: &EncryptionService,
    storage: &Storage,
    storage_key: &str,
    token_name: &str,
) -> Result<(Vec<Utxo>, Vec<String>, bool, u64)> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to fetch SPL UTXOs: {}", e)))?;

    if !response.status().is_success() {
        return Err(PrivacyCashError::ApiError(format!(
            "SPL UTXO API returned status: {}",
            response.status()
        )));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to parse SPL UTXOs: {}", e)))?;

    let (encrypted_outputs, has_more, _total) =
        if let Some(outputs) = data.get("encrypted_outputs") {
            let outputs: Vec<String> = serde_json::from_value(outputs.clone()).unwrap_or_default();
            let has_more = data
                .get("hasMore")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let total = data.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
            (outputs, has_more, total)
        } else if data.is_array() {
            #[derive(Deserialize)]
            struct ApiUtxo {
                encrypted_output: String,
            }

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
            return Err(PrivacyCashError::ApiError(
                "Unexpected API response format".to_string(),
            ));
        };

    let len = encrypted_outputs.len() as u64;

    // Decrypt outputs
    let (utxos, decrypted_outputs) =
        decrypt_outputs_spl(&encrypted_outputs, encryption_service, token_name).await?;

    // Also check cached outputs if no more to fetch
    let mut all_utxos = utxos;
    let mut all_outputs = decrypted_outputs;

    if !has_more {
        if let Some(cached) = storage.get(&format!("{}{}", LSK_ENCRYPTED_OUTPUTS, storage_key)) {
            if let Ok(cached_outputs) = serde_json::from_str::<Vec<String>>(&cached) {
                let (cached_utxos, cached_decrypted) =
                    decrypt_outputs_spl(&cached_outputs, encryption_service, token_name).await?;
                all_utxos.extend(cached_utxos);
                all_outputs.extend(cached_decrypted);
            }
        }
    }

    Ok((all_utxos, all_outputs, has_more, len))
}

/// Decrypt encrypted SPL outputs
async fn decrypt_outputs_spl(
    encrypted_outputs: &[String],
    encryption_service: &EncryptionService,
    token_name: &str,
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
                continue;
            }
        }
    }

    // Fetch real indices
    if !outputs.is_empty() {
        let indices = fetch_utxo_indices_spl(&outputs, token_name).await?;
        for (utxo, index) in utxos.iter_mut().zip(indices) {
            if utxo.index != index {
                log::debug!("Updated SPL UTXO index from {} to {}", utxo.index, index);
                utxo.index = index;
            }
        }
    }

    Ok((utxos, outputs))
}

/// Fetch UTXO indices for SPL tokens
async fn fetch_utxo_indices_spl(encrypted_outputs: &[String], token_name: &str) -> Result<Vec<u64>> {
    let url = format!("{}/utxos/indices", *RELAYER_API_URL);

    let body = serde_json::json!({
        "encrypted_outputs": encrypted_outputs,
        "token": token_name
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to fetch SPL indices: {}", e)))?;

    if !response.status().is_success() {
        return Err(PrivacyCashError::ApiError(format!(
            "SPL indices API returned status: {}",
            response.status()
        )));
    }

    #[derive(Deserialize)]
    struct IndicesResponse {
        indices: Vec<u64>,
    }

    let data: IndicesResponse = response
        .json()
        .await
        .map_err(|e| PrivacyCashError::ApiError(format!("Failed to parse SPL indices: {}", e)))?;

    Ok(data.indices)
}

/// Check if SPL UTXOs are spent
async fn are_utxos_spent_spl(connection: &RpcClient, utxos: &[Utxo]) -> Result<Vec<bool>> {
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

/// Convert nullifier string to bytes
fn string_to_nullifier_bytes(nullifier: &str) -> Result<[u8; 32]> {
    let n = BigUint::parse_bytes(nullifier.as_bytes(), 10)
        .ok_or_else(|| PrivacyCashError::SerializationError("Invalid nullifier".to_string()))?;

    let bytes = n.to_bytes_le();
    let mut result = [0u8; 32];
    let len = bytes.len().min(32);
    result[..len].copy_from_slice(&bytes[..len]);
    result.reverse();
    Ok(result)
}

/// Get SPL private balance
pub async fn get_private_balance_spl(
    connection: &RpcClient,
    public_key: &Pubkey,
    encryption_service: &EncryptionService,
    storage: &Storage,
    mint_address: &Pubkey,
) -> Result<SplBalance> {
    let token = find_token_by_mint(mint_address)
        .ok_or_else(|| PrivacyCashError::TokenNotSupported(mint_address.to_string()))?;

    let utxos =
        get_utxos_spl(connection, public_key, encryption_service, storage, mint_address, None)
            .await?;

    Ok(get_balance_from_utxos_spl(&utxos, token.units_per_token))
}
