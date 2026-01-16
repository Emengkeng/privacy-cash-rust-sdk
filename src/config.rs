//! Configuration fetching from the relayer API

use crate::constants::RELAYER_API_URL;
use crate::error::{PrivacyCashError, Result};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Global cached configuration
static CONFIG_CACHE: OnceCell<RwLock<Option<Config>>> = OnceCell::new();

/// Configuration from the relayer API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Fee rate for withdrawals (as a decimal, e.g., 0.01 = 1%)
    pub withdraw_fee_rate: f64,

    /// Rent fee for withdrawals in SOL
    pub withdraw_rent_fee: f64,

    /// Fee rate for deposits
    pub deposit_fee_rate: f64,

    /// USDC-specific withdraw rent fee
    pub usdc_withdraw_rent_fee: f64,

    /// Rent fees per token
    #[serde(default)]
    pub rent_fees: HashMap<String, f64>,
}

impl Config {
    /// Fetch configuration from the relayer API
    pub async fn fetch() -> Result<Self> {
        let url = format!("{}/config", *RELAYER_API_URL);

        let response = reqwest::get(&url)
            .await
            .map_err(|e| PrivacyCashError::ApiError(format!("Failed to fetch config: {}", e)))?;

        if !response.status().is_success() {
            return Err(PrivacyCashError::ApiError(format!(
                "Config API returned status: {}",
                response.status()
            )));
        }

        let config: Config = response
            .json()
            .await
            .map_err(|e| PrivacyCashError::ApiError(format!("Failed to parse config: {}", e)))?;

        Ok(config)
    }

    /// Get cached configuration or fetch if not cached
    pub async fn get_or_fetch() -> Result<Self> {
        let cache = CONFIG_CACHE.get_or_init(|| RwLock::new(None));

        // Try to read from cache first
        {
            let read_guard = cache.read();
            if let Some(config) = read_guard.as_ref() {
                return Ok(config.clone());
            }
        }

        // Fetch and cache
        let config = Self::fetch().await?;
        {
            let mut write_guard = cache.write();
            *write_guard = Some(config.clone());
        }

        Ok(config)
    }

    /// Clear the cached configuration
    pub fn clear_cache() {
        if let Some(cache) = CONFIG_CACHE.get() {
            let mut write_guard = cache.write();
            *write_guard = None;
        }
    }

    /// Get withdraw fee rate
    pub async fn get_withdraw_fee_rate() -> Result<f64> {
        let config = Self::get_or_fetch().await?;
        Ok(config.withdraw_fee_rate)
    }

    /// Get withdraw rent fee
    pub async fn get_withdraw_rent_fee() -> Result<f64> {
        let config = Self::get_or_fetch().await?;
        Ok(config.withdraw_rent_fee)
    }

    /// Get deposit fee rate
    pub async fn get_deposit_fee_rate() -> Result<f64> {
        let config = Self::get_or_fetch().await?;
        Ok(config.deposit_fee_rate)
    }

    /// Get rent fee for a specific token
    pub async fn get_token_rent_fee(token_name: &str) -> Result<f64> {
        let config = Self::get_or_fetch().await?;
        config
            .rent_fees
            .get(token_name)
            .copied()
            .ok_or_else(|| PrivacyCashError::ConfigError(format!("No rent fee for {}", token_name)))
    }
}
