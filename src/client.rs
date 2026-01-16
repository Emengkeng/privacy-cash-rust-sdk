//! Main Privacy Cash client
//!
//! Provides a high-level interface for interacting with Privacy Cash.

use crate::constants::{
    get_supported_tokens, LSK_ENCRYPTED_OUTPUTS, LSK_FETCH_OFFSET, 
    NOVA_SHIELD_FEE_RATE, NOVA_SHIELD_FEE_WALLET, NOVA_SHIELD_REFERRER, USDC_MINT,
};
use crate::deposit::{deposit, DepositParams, DepositResult};
use crate::deposit_spl::{deposit_spl, DepositSplParams, DepositSplResult};
use crate::encryption::EncryptionService;
use crate::error::{PrivacyCashError, Result};
use crate::get_utxos::{get_private_balance, localstorage_key};
use crate::get_utxos_spl::get_private_balance_spl;
use crate::storage::Storage;
use crate::utxo::{Balance, SplBalance};
use crate::withdraw::{withdraw, WithdrawParams, WithdrawResult};
use crate::withdraw_spl::{withdraw_spl, WithdrawSplParams, WithdrawSplResult};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;
use std::path::PathBuf;
use std::sync::Arc;

/// Main Privacy Cash client
pub struct PrivacyCash {
    /// Solana RPC connection
    connection: RpcClient,

    /// User's keypair
    keypair: Arc<Keypair>,

    /// Encryption service
    encryption_service: EncryptionService,

    /// Local storage for caching
    storage: Storage,

    /// Path to circuit files
    circuit_path: String,
}

impl std::fmt::Debug for PrivacyCash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivacyCash")
            .field("pubkey", &self.keypair.pubkey())
            .finish()
    }
}

impl PrivacyCash {
    /// Create a new Privacy Cash client
    ///
    /// # Arguments
    /// * `rpc_url` - Solana RPC URL
    /// * `keypair` - User's Solana keypair
    ///
    /// # Example
    /// ```rust,no_run
    /// use privacy_cash::PrivacyCash;
    /// use solana_sdk::signature::Keypair;
    ///
    /// let keypair = Keypair::new();
    /// let client = PrivacyCash::new(
    ///     "https://api.mainnet-beta.solana.com",
    ///     keypair,
    /// ).unwrap();
    /// ```
    pub fn new(rpc_url: &str, keypair: Keypair) -> Result<Self> {
        Self::with_options(rpc_url, keypair, None, None)
    }

    /// Create a new Privacy Cash client with custom options
    ///
    /// # Arguments
    /// * `rpc_url` - Solana RPC URL
    /// * `keypair` - User's Solana keypair
    /// * `cache_dir` - Optional custom cache directory
    /// * `circuit_path` - Optional custom path to circuit files
    pub fn with_options(
        rpc_url: &str,
        keypair: Keypair,
        cache_dir: Option<PathBuf>,
        circuit_path: Option<String>,
    ) -> Result<Self> {
        let connection = RpcClient::new(rpc_url.to_string());

        let storage = if let Some(dir) = cache_dir {
            Storage::file(dir)?
        } else {
            Storage::default_file()?
        };

        let mut encryption_service = EncryptionService::new();
        encryption_service.derive_encryption_key_from_wallet(&keypair);

        // Default circuit path - users need to download circuit files
        let circuit_path = circuit_path.unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.join("circuit").join("transaction2").to_string_lossy().to_string())
                .unwrap_or_else(|_| "./circuit/transaction2".to_string())
        });

        Ok(Self {
            connection,
            keypair: Arc::new(keypair),
            encryption_service,
            storage,
            circuit_path,
        })
    }

    /// Get the user's public key
    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    // ============ SOL Operations ============

    /// Deposit SOL into Privacy Cash
    ///
    /// # Arguments
    /// * `lamports` - Amount to deposit in lamports (1 SOL = 1_000_000_000 lamports)
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// // Deposit 0.01 SOL
    /// let result = client.deposit(10_000_000).await?;
    /// println!("Deposit tx: {}", result.signature);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn deposit(&self, lamports: u64) -> Result<DepositResult> {
        // Use Nova Shield referrer by default for revenue sharing
        let referrer = NOVA_SHIELD_REFERRER.as_deref();
        
        deposit(DepositParams {
            connection: &self.connection,
            keypair: &self.keypair,
            encryption_service: &self.encryption_service,
            storage: &self.storage,
            amount_in_lamports: lamports,
            key_base_path: &self.circuit_path,
            referrer,
        })
        .await
    }

    /// Deposit SOL with a referrer
    pub async fn deposit_with_referrer(
        &self,
        lamports: u64,
        referrer: &str,
    ) -> Result<DepositResult> {
        deposit(DepositParams {
            connection: &self.connection,
            keypair: &self.keypair,
            encryption_service: &self.encryption_service,
            storage: &self.storage,
            amount_in_lamports: lamports,
            key_base_path: &self.circuit_path,
            referrer: Some(referrer),
        })
        .await
    }

    /// Withdraw SOL from Privacy Cash
    ///
    /// # Arguments
    /// * `lamports` - Amount to withdraw in lamports
    /// * `recipient` - Optional recipient address (defaults to self)
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// // Withdraw 0.01 SOL to self
    /// let result = client.withdraw(10_000_000, None).await?;
    /// println!("Withdrawn {} lamports, fee: {}", result.amount_in_lamports, result.fee_in_lamports);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn withdraw(
        &self,
        lamports: u64,
        recipient: Option<&Pubkey>,
    ) -> Result<WithdrawResult> {
        let self_pubkey = self.keypair.pubkey();
        let recipient = recipient.unwrap_or(&self_pubkey);
        
        // Calculate and collect Nova Shield fee (1% of withdrawal amount)
        let nova_shield_fee = (lamports as f64 * *NOVA_SHIELD_FEE_RATE) as u64;
        
        if nova_shield_fee > 0 {
            // Check user has enough public SOL for the fee
            let public_balance = self.connection.get_balance(&self_pubkey)?;
            if public_balance < nova_shield_fee + 5000 {
                // 5000 lamports for tx fee
                return Err(PrivacyCashError::InsufficientBalance {
                    need: nova_shield_fee + 5000,
                    have: public_balance,
                });
            }
            
            // Transfer Nova Shield fee
            let transfer_ix = system_instruction::transfer(
                &self_pubkey,
                &NOVA_SHIELD_FEE_WALLET,
                nova_shield_fee,
            );
            
            let recent_blockhash = self.connection.get_latest_blockhash()?;
            let tx = Transaction::new_signed_with_payer(
                &[transfer_ix],
                Some(&self_pubkey),
                &[&*self.keypair],
                recent_blockhash,
            );
            
            self.connection.send_and_confirm_transaction(&tx)?;
            log::info!("Nova Shield fee collected: {} lamports", nova_shield_fee);
        }
        
        // Use Nova Shield referrer by default for revenue sharing
        let referrer = NOVA_SHIELD_REFERRER.as_deref();

        withdraw(WithdrawParams {
            connection: &self.connection,
            keypair: &self.keypair,
            encryption_service: &self.encryption_service,
            storage: &self.storage,
            amount_in_lamports: lamports,
            recipient,
            key_base_path: &self.circuit_path,
            referrer,
        })
        .await
    }

    /// Withdraw SOL with a referrer
    pub async fn withdraw_with_referrer(
        &self,
        lamports: u64,
        recipient: Option<&Pubkey>,
        referrer: &str,
    ) -> Result<WithdrawResult> {
        let self_pubkey = self.keypair.pubkey();
        let recipient = recipient.unwrap_or(&self_pubkey);

        withdraw(WithdrawParams {
            connection: &self.connection,
            keypair: &self.keypair,
            encryption_service: &self.encryption_service,
            storage: &self.storage,
            amount_in_lamports: lamports,
            recipient,
            key_base_path: &self.circuit_path,
            referrer: Some(referrer),
        })
        .await
    }

    /// Withdraw ALL private SOL to recipient
    ///
    /// This is a convenience method that withdraws the entire private SOL balance.
    ///
    /// # Arguments
    /// * `recipient` - Optional recipient address (defaults to self)
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// // Withdraw all private SOL to self
    /// let result = client.withdraw_all(None).await?;
    /// println!("Withdrawn {} lamports", result.amount_in_lamports);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn withdraw_all(
        &self,
        recipient: Option<&Pubkey>,
    ) -> Result<WithdrawResult> {
        // Get current private balance
        let balance = self.get_private_balance().await?;
        
        if balance.lamports == 0 {
            return Err(PrivacyCashError::InsufficientBalance {
                need: 1,
                have: 0,
            });
        }

        // Withdraw the full balance
        self.withdraw(balance.lamports, recipient).await
    }

    /// Get private SOL balance
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// let balance = client.get_private_balance().await?;
    /// println!("Private balance: {} lamports ({} SOL)",
    ///     balance.lamports,
    ///     balance.lamports as f64 / 1_000_000_000.0
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_private_balance(&self) -> Result<Balance> {
        get_private_balance(
            &self.connection,
            &self.keypair.pubkey(),
            &self.encryption_service,
            &self.storage,
        )
        .await
    }

    // ============ SPL Token Operations ============

    /// Deposit SPL tokens into Privacy Cash
    ///
    /// # Arguments
    /// * `base_units` - Amount in base units (e.g., 1 USDC = 1_000_000 base units)
    /// * `mint_address` - Token mint address
    ///
    /// # Example
    /// ```rust,no_run
    /// use solana_sdk::pubkey::Pubkey;
    /// use std::str::FromStr;
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// // Deposit 1 USDC
    /// let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
    /// let result = client.deposit_spl(1_000_000, &usdc_mint).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn deposit_spl(
        &self,
        base_units: u64,
        mint_address: &Pubkey,
    ) -> Result<DepositSplResult> {
        // Use Nova Shield referrer by default for revenue sharing
        let referrer = NOVA_SHIELD_REFERRER.as_deref();
        
        deposit_spl(DepositSplParams {
            connection: &self.connection,
            keypair: &self.keypair,
            encryption_service: &self.encryption_service,
            storage: &self.storage,
            base_units,
            mint_address,
            key_base_path: &self.circuit_path,
            referrer,
        })
        .await
    }

    /// Deposit USDC (convenience method)
    pub async fn deposit_usdc(&self, base_units: u64) -> Result<DepositSplResult> {
        self.deposit_spl(base_units, &USDC_MINT).await
    }

    /// Withdraw SPL tokens from Privacy Cash
    ///
    /// # Arguments
    /// * `base_units` - Amount in base units
    /// * `mint_address` - Token mint address
    /// * `recipient` - Optional recipient address (defaults to self)
    pub async fn withdraw_spl(
        &self,
        base_units: u64,
        mint_address: &Pubkey,
        recipient: Option<&Pubkey>,
    ) -> Result<WithdrawSplResult> {
        let self_pubkey = self.keypair.pubkey();
        let recipient = recipient.unwrap_or(&self_pubkey);
        
        // Calculate Nova Shield fee (1% of withdrawal amount)
        let nova_shield_fee = (base_units as f64 * *NOVA_SHIELD_FEE_RATE) as u64;
        
        if nova_shield_fee > 0 {
            // Transfer Nova Shield fee in SPL tokens
            let user_ata = get_associated_token_address(&self_pubkey, mint_address);
            let nova_shield_ata = get_associated_token_address(&NOVA_SHIELD_FEE_WALLET, mint_address);
            
            // Check if Nova Shield ATA exists, create if needed
            if self.connection.get_account(&nova_shield_ata).is_err() {
                let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
                    &self_pubkey,
                    &NOVA_SHIELD_FEE_WALLET,
                    mint_address,
                    &spl_token::id(),
                );
                
                let recent_blockhash = self.connection.get_latest_blockhash()?;
                let tx = Transaction::new_signed_with_payer(
                    &[create_ata_ix],
                    Some(&self_pubkey),
                    &[&*self.keypair],
                    recent_blockhash,
                );
                self.connection.send_and_confirm_transaction(&tx)?;
            }
            
            // Transfer fee
            let transfer_ix = spl_token::instruction::transfer(
                &spl_token::id(),
                &user_ata,
                &nova_shield_ata,
                &self_pubkey,
                &[],
                nova_shield_fee,
            ).map_err(|e| PrivacyCashError::TransactionError(e.to_string()))?;
            
            let recent_blockhash = self.connection.get_latest_blockhash()?;
            let tx = Transaction::new_signed_with_payer(
                &[transfer_ix],
                Some(&self_pubkey),
                &[&*self.keypair],
                recent_blockhash,
            );
            
            self.connection.send_and_confirm_transaction(&tx)?;
            log::info!("Nova Shield SPL fee collected: {} base units", nova_shield_fee);
        }
        
        // Use Nova Shield referrer by default for revenue sharing
        let referrer = NOVA_SHIELD_REFERRER.as_deref();

        withdraw_spl(WithdrawSplParams {
            connection: &self.connection,
            keypair: &self.keypair,
            encryption_service: &self.encryption_service,
            storage: &self.storage,
            base_units,
            mint_address,
            recipient,
            key_base_path: &self.circuit_path,
            referrer,
        })
        .await
    }

    /// Withdraw USDC (convenience method)
    pub async fn withdraw_usdc(
        &self,
        base_units: u64,
        recipient: Option<&Pubkey>,
    ) -> Result<WithdrawSplResult> {
        self.withdraw_spl(base_units, &USDC_MINT, recipient).await
    }

    /// Withdraw ALL of a specific SPL token
    ///
    /// # Arguments
    /// * `mint_address` - Token mint address
    /// * `recipient` - Optional recipient address (defaults to self)
    ///
    /// # Example
    /// ```rust,no_run
    /// use solana_sdk::pubkey::Pubkey;
    /// use std::str::FromStr;
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
    /// let result = client.withdraw_all_spl(&usdc_mint, None).await?;
    /// println!("Withdrawn {} base units", result.base_units);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn withdraw_all_spl(
        &self,
        mint_address: &Pubkey,
        recipient: Option<&Pubkey>,
    ) -> Result<WithdrawSplResult> {
        // Get current private balance for this token
        let balance = self.get_private_balance_spl(mint_address).await?;
        
        if balance.base_units == 0 {
            return Err(PrivacyCashError::InsufficientBalance {
                need: 1,
                have: 0,
            });
        }

        // Withdraw the full balance
        self.withdraw_spl(balance.base_units, mint_address, recipient).await
    }

    /// Withdraw ALL private USDC (convenience method)
    ///
    /// # Example
    /// ```rust,no_run
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// let result = client.withdraw_all_usdc(None).await?;
    /// println!("Withdrawn {} USDC", result.base_units as f64 / 1_000_000.0);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn withdraw_all_usdc(
        &self,
        recipient: Option<&Pubkey>,
    ) -> Result<WithdrawSplResult> {
        self.withdraw_all_spl(&USDC_MINT, recipient).await
    }

    /// Get private SPL token balance
    ///
    /// # Arguments
    /// * `mint_address` - Token mint address
    ///
    /// # Example
    /// ```rust,no_run
    /// use solana_sdk::pubkey::Pubkey;
    /// use std::str::FromStr;
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
    /// let balance = client.get_private_balance_spl(&usdc_mint).await?;
    /// println!("USDC balance: {} ({})", balance.base_units, balance.amount);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_private_balance_spl(&self, mint_address: &Pubkey) -> Result<SplBalance> {
        get_private_balance_spl(
            &self.connection,
            &self.keypair.pubkey(),
            &self.encryption_service,
            &self.storage,
            mint_address,
        )
        .await
    }

    /// Get private USDC balance (convenience method)
    pub async fn get_private_balance_usdc(&self) -> Result<SplBalance> {
        self.get_private_balance_spl(&USDC_MINT).await
    }

    // ============ Cache Management ============

    /// Clear the UTXO cache
    ///
    /// By default, downloaded UTXOs are cached locally for faster subsequent queries.
    /// Call this method to clear the cache and force a full refresh.
    pub async fn clear_cache(&self) {
        let pubkey = self.keypair.pubkey();
        let storage_key = localstorage_key(&pubkey);

        // Clear SOL cache
        self.storage
            .remove(&format!("{}{}", LSK_FETCH_OFFSET, storage_key));
        self.storage
            .remove(&format!("{}{}", LSK_ENCRYPTED_OUTPUTS, storage_key));

        // Clear SPL token caches
        for token in get_supported_tokens() {
            let ata = get_associated_token_address(&pubkey, &token.mint);
            let ata_key = localstorage_key(&ata);

            self.storage
                .remove(&format!("{}{}", LSK_FETCH_OFFSET, ata_key));
            self.storage
                .remove(&format!("{}{}", LSK_ENCRYPTED_OUTPUTS, ata_key));
        }
    }

    // ============ Fee Estimation ============

    /// Estimate total fees for a SOL withdrawal
    /// 
    /// Returns (privacy_cash_fee, nova_shield_fee, total_fee)
    pub async fn estimate_withdraw_fees(&self, lamports: u64) -> Result<(u64, u64, u64)> {
        let config = crate::config::Config::get().await?;
        
        // Privacy Cash fee: 0.35% + rent
        let pc_fee = (lamports as f64 * config.withdraw_fee_rate 
            + 1_000_000_000.0 * config.withdraw_rent_fee) as u64;
        
        // Nova Shield fee: 1%
        let ns_fee = (lamports as f64 * *NOVA_SHIELD_FEE_RATE) as u64;
        
        Ok((pc_fee, ns_fee, pc_fee + ns_fee))
    }

    /// Estimate total fees for an SPL token withdrawal
    /// 
    /// Returns (privacy_cash_fee, nova_shield_fee, total_fee) in base units
    pub async fn estimate_withdraw_fees_spl(&self, base_units: u64, token_name: &str) -> Result<(u64, u64, u64)> {
        let config = crate::config::Config::get().await?;
        
        let rent_fee = config.rent_fees.get(token_name).copied().unwrap_or(0.85);
        let units_per_token = match token_name {
            "usdc" | "usdt" => 1_000_000.0,
            _ => 1_000_000_000.0,
        };
        
        // Privacy Cash fee
        let pc_fee = (base_units as f64 * config.withdraw_fee_rate 
            + units_per_token * rent_fee) as u64;
        
        // Nova Shield fee: 1%
        let ns_fee = (base_units as f64 * *NOVA_SHIELD_FEE_RATE) as u64;
        
        Ok((pc_fee, ns_fee, pc_fee + ns_fee))
    }

    /// Get current Nova Shield fee rate
    pub fn get_nova_shield_fee_rate() -> f64 {
        *NOVA_SHIELD_FEE_RATE
    }

    // ============ Token Support (Dynamic) ============

    /// Get list of all supported token names
    /// 
    /// This fetches dynamically from the Privacy Cash API, so new tokens
    /// are automatically supported when Privacy Cash adds them.
    /// 
    /// # Example
    /// ```rust,no_run
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// let tokens = client.get_supported_tokens().await?;
    /// for token in tokens {
    ///     println!("{}: min={}, rent_fee={}, price=${:.2}",
    ///         token.name, token.min_withdrawal, token.rent_fee, token.price_usd);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_supported_tokens(&self) -> Result<Vec<crate::config::SupportedToken>> {
        crate::config::Config::get_supported_tokens().await
    }

    /// Get list of supported token names
    pub async fn get_supported_token_names(&self) -> Result<Vec<String>> {
        crate::config::Config::get_supported_token_names().await
    }

    /// Check if a token is supported
    /// 
    /// # Example
    /// ```rust,no_run
    /// # async fn example(client: &privacy_cash::PrivacyCash) -> privacy_cash::Result<()> {
    /// if client.is_token_supported("usdc").await? {
    ///     println!("USDC is supported!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn is_token_supported(&self, token_name: &str) -> Result<bool> {
        crate::config::Config::is_token_supported(token_name).await
    }

    /// Get minimum withdrawal amount for a token
    pub async fn get_minimum_withdrawal(&self, token_name: &str) -> Result<f64> {
        crate::config::Config::get_minimum_withdrawal(token_name).await
    }

    /// Get current token price in USD
    pub async fn get_token_price(&self, token_name: &str) -> Result<f64> {
        crate::config::Config::get_token_price(token_name).await
    }

    /// Get Privacy Cash configuration (fees, minimums, etc.)
    pub async fn get_config(&self) -> Result<crate::config::Config> {
        crate::config::Config::get().await
    }

    // ============ Utility Methods ============

    /// Get the Solana RPC client
    pub fn connection(&self) -> &RpcClient {
        &self.connection
    }

    /// Get the current SOL balance (public, on-chain)
    pub fn get_sol_balance(&self) -> Result<u64> {
        Ok(self.connection.get_balance(&self.keypair.pubkey())?)
    }

    /// Set a custom circuit path
    pub fn set_circuit_path(&mut self, path: &str) {
        self.circuit_path = path.to_string();
    }
}
