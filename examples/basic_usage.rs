//! Basic usage example for Privacy Cash Rust SDK
//!
//! This example demonstrates how to use the Privacy Cash SDK to:
//! - Create a client
//! - Check private balances
//! - Deposit and withdraw SOL
//! - Deposit and withdraw USDC
//!
//! Run with: cargo run --example basic_usage

use privacy_cash::{PrivacyCash, Result, USDC_MINT};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::str::FromStr;

/// SOL example: deposit, check balance, withdraw
async fn sol_example(client: &PrivacyCash, recipient: &Pubkey) -> Result<()> {
    println!("\n=== SOL Example ===\n");

    // Check initial private balance
    let balance = client.get_private_balance().await?;
    println!(
        "Initial private SOL balance: {} lamports ({:.6} SOL)",
        balance.lamports,
        balance.lamports as f64 / 1_000_000_000.0
    );

    // Deposit 0.01 SOL
    println!("\nDepositing 0.01 SOL...");
    let deposit_result = client.deposit(10_000_000).await?;
    println!("Deposit successful! Tx: {}", deposit_result.signature);

    // Check balance after deposit
    let balance = client.get_private_balance().await?;
    println!(
        "Balance after deposit: {} lamports ({:.6} SOL)",
        balance.lamports,
        balance.lamports as f64 / 1_000_000_000.0
    );

    // Withdraw 0.005 SOL
    println!("\nWithdrawing 0.005 SOL...");
    let withdraw_result = client.withdraw(5_000_000, Some(recipient)).await?;
    println!(
        "Withdrawal successful! Tx: {}\n  Recipient: {}\n  Amount: {} lamports\n  Fee: {} lamports",
        withdraw_result.signature,
        withdraw_result.recipient,
        withdraw_result.amount_in_lamports,
        withdraw_result.fee_in_lamports
    );

    // Check final balance
    let balance = client.get_private_balance().await?;
    println!(
        "Final private SOL balance: {} lamports ({:.6} SOL)",
        balance.lamports,
        balance.lamports as f64 / 1_000_000_000.0
    );

    Ok(())
}

/// USDC example: deposit, check balance, withdraw
async fn usdc_example(client: &PrivacyCash, recipient: &Pubkey) -> Result<()> {
    println!("\n=== USDC Example ===\n");

    // Check initial private USDC balance
    let balance = client.get_private_balance_usdc().await?;
    println!(
        "Initial private USDC balance: {} base units ({:.2} USDC)",
        balance.base_units, balance.amount
    );

    // Deposit 1 USDC
    println!("\nDepositing 1 USDC...");
    let deposit_result = client.deposit_usdc(1_000_000).await?;
    println!("Deposit successful! Tx: {}", deposit_result.signature);

    // Check balance after deposit
    let balance = client.get_private_balance_usdc().await?;
    println!(
        "Balance after deposit: {} base units ({:.2} USDC)",
        balance.base_units, balance.amount
    );

    // Withdraw 0.5 USDC
    println!("\nWithdrawing 0.5 USDC...");
    let withdraw_result = client.withdraw_usdc(500_000, Some(recipient)).await?;
    println!(
        "Withdrawal successful! Tx: {}\n  Recipient: {}\n  Amount: {} base units\n  Fee: {} base units",
        withdraw_result.signature,
        withdraw_result.recipient,
        withdraw_result.base_units,
        withdraw_result.fee_base_units
    );

    // Check final balance
    let balance = client.get_private_balance_usdc().await?;
    println!(
        "Final private USDC balance: {} base units ({:.2} USDC)",
        balance.base_units, balance.amount
    );

    Ok(())
}

/// Generic SPL token example
async fn spl_example(client: &PrivacyCash, mint_address: &Pubkey, recipient: &Pubkey) -> Result<()> {
    println!("\n=== SPL Token Example (mint: {}) ===\n", mint_address);

    // Check balance
    let balance = client.get_private_balance_spl(mint_address).await?;
    println!(
        "Private balance: {} base units ({:.6} tokens)",
        balance.base_units, balance.amount
    );

    // Deposit
    println!("\nDepositing 100 base units...");
    let deposit_result = client.deposit_spl(100, mint_address).await?;
    println!("Deposit successful! Tx: {}", deposit_result.signature);

    // Withdraw
    println!("\nWithdrawing 50 base units...");
    let withdraw_result = client
        .withdraw_spl(50, mint_address, Some(recipient))
        .await?;
    println!("Withdrawal successful! Tx: {}", withdraw_result.signature);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger
    env_logger::init();

    println!("Privacy Cash Rust SDK - Basic Usage Example");
    println!("============================================\n");

    // Configuration - replace with your values
    let rpc_url = std::env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());

    let private_key = std::env::var("SOLANA_PRIVATE_KEY")
        .expect("Please set SOLANA_PRIVATE_KEY environment variable");

    // Parse private key (supports base58 or JSON array format)
    let keypair = if private_key.starts_with('[') {
        let bytes: Vec<u8> = serde_json::from_str(&private_key)
            .expect("Invalid private key format");
        Keypair::from_bytes(&bytes).expect("Invalid keypair bytes")
    } else {
        let bytes = bs58::decode(&private_key)
            .into_vec()
            .expect("Invalid base58 private key");
        Keypair::from_bytes(&bytes).expect("Invalid keypair bytes")
    };

    println!("Using wallet: {}", keypair.pubkey());
    println!("RPC URL: {}", rpc_url);

    // Create Privacy Cash client
    let client = PrivacyCash::new(&rpc_url, keypair)?;

    // Check on-chain SOL balance
    let sol_balance = client.get_sol_balance()?;
    println!(
        "On-chain SOL balance: {:.6} SOL",
        sol_balance as f64 / 1_000_000_000.0
    );

    // Optional: clear cache for fresh start
    // client.clear_cache().await;

    // Recipient for withdrawals (defaults to self if not specified)
    let recipient = client.pubkey();

    // Run examples
    // Uncomment the examples you want to run:

    // SOL example
    // sol_example(&client, &recipient).await?;

    // USDC example
    // usdc_example(&client, &recipient).await?;

    // Just check balances (safe, no transactions)
    println!("\n=== Balance Check ===\n");

    let sol_balance = client.get_private_balance().await?;
    println!(
        "Private SOL balance: {} lamports ({:.6} SOL)",
        sol_balance.lamports,
        sol_balance.lamports as f64 / 1_000_000_000.0
    );

    let usdc_balance = client.get_private_balance_usdc().await?;
    println!(
        "Private USDC balance: {} base units ({:.2} USDC)",
        usdc_balance.base_units, usdc_balance.amount
    );

    // Check other supported tokens
    let usdt_mint = Pubkey::from_str("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB").unwrap();
    let usdt_balance = client.get_private_balance_spl(&usdt_mint).await?;
    println!(
        "Private USDT balance: {} base units ({:.2} USDT)",
        usdt_balance.base_units, usdt_balance.amount
    );

    println!("\n✓ Example completed successfully!");

    Ok(())
}
