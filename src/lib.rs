//! # Privacy Cash Rust SDK
//!
//! A Rust SDK for interacting with Privacy Cash on Solana.
//! Provides privacy-preserving deposit and withdrawal functionality using zero-knowledge proofs.
//!
//! ## Features
//!
//! - **SOL Operations**: Deposit and withdraw native SOL privately
//! - **SPL Token Operations**: Deposit and withdraw SPL tokens (USDC, USDT, etc.)
//! - **Balance Queries**: Check private balances without revealing identity
//! - **UTXO Management**: Automatic UTXO consolidation and caching
//!
//! ## Example
//!
//! ```rust,no_run
//! use privacy_cash::PrivacyCash;
//! use solana_sdk::signature::Keypair;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create client
//!     let keypair = Keypair::new();
//!     let client = PrivacyCash::new(
//!         "https://api.mainnet-beta.solana.com",
//!         keypair,
//!     )?;
//!
//!     // Check private balance
//!     let balance = client.get_private_balance().await?;
//!     println!("Private balance: {} lamports", balance.lamports);
//!
//!     // Deposit SOL
//!     let result = client.deposit(10_000_000).await?; // 0.01 SOL
//!     println!("Deposit tx: {}", result.signature);
//!
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod config;
pub mod constants;
pub mod deposit;
pub mod deposit_spl;
pub mod encryption;
pub mod error;
pub mod get_utxos;
pub mod get_utxos_spl;
pub mod keypair;
pub mod merkle_tree;
pub mod prover;
pub mod storage;
pub mod utxo;
pub mod utils;
pub mod withdraw;
pub mod withdraw_spl;

// Re-export main types
pub use client::PrivacyCash;
pub use config::Config;
pub use constants::*;
pub use error::{PrivacyCashError, Result};
pub use keypair::ZkKeypair;
pub use utxo::Utxo;

// Re-export Solana types for convenience
pub use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
