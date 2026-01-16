//! Quick balance check example - no transactions, just reads
//!
//! Run with:
//!   SOLANA_PRIVATE_KEY="your-base58-private-key" cargo run --example check_balance
//!
//! Or using a JSON keypair file:
//!   SOLANA_PRIVATE_KEY=$(cat ~/.config/solana/id.json) cargo run --example check_balance

use privacy_cash::{PrivacyCash, Signer};
use solana_sdk::signature::Keypair;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("🔒 Privacy Cash Balance Checker\n");

    // Get private key from environment variable (REQUIRED)
    let private_key = std::env::var("SOLANA_PRIVATE_KEY")
        .expect("❌ Please set SOLANA_PRIVATE_KEY environment variable");

    // Parse private key (supports base58 or JSON array format)
    let keypair = if private_key.trim().starts_with('[') {
        // JSON array format
        let bytes: Vec<u8> = serde_json::from_str(&private_key)
            .expect("Invalid JSON private key format");
        Keypair::from_bytes(&bytes)?
    } else {
        // Base58 format
        let key_bytes = bs58::decode(&private_key).into_vec()?;
        Keypair::from_bytes(&key_bytes)?
    };

    println!("Wallet: {}", keypair.pubkey());

    // Use mainnet RPC (can be overridden with SOLANA_RPC_URL)
    let rpc_url = std::env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    println!("RPC: {}\n", rpc_url);

    // Create client
    let client = PrivacyCash::new(&rpc_url, keypair)?;

    // Check on-chain SOL balance
    match client.get_sol_balance() {
        Ok(balance) => {
            println!("📊 On-chain SOL: {:.6} SOL", balance as f64 / 1e9);
        }
        Err(e) => {
            println!("❌ Failed to get SOL balance: {}", e);
        }
    }

    println!("\n🔍 Checking private balances (this queries the relayer)...\n");

    // Check private SOL balance
    println!("Fetching private SOL balance...");
    match client.get_private_balance().await {
        Ok(balance) => {
            println!("✅ Private SOL: {} lamports ({:.6} SOL)", 
                balance.lamports, 
                balance.lamports as f64 / 1e9
            );
        }
        Err(e) => {
            println!("❌ Private SOL error: {}", e);
        }
    }

    // Check private USDC balance
    println!("\nFetching private USDC balance...");
    match client.get_private_balance_usdc().await {
        Ok(balance) => {
            println!("✅ Private USDC: {} base units ({:.2} USDC)", 
                balance.base_units, 
                balance.amount
            );
        }
        Err(e) => {
            println!("❌ Private USDC error: {}", e);
        }
    }

    // Check private USDT balance
    println!("\nFetching private USDT balance...");
    let usdt_mint = solana_sdk::pubkey::Pubkey::from_str(
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"
    )?;
    match client.get_private_balance_spl(&usdt_mint).await {
        Ok(balance) => {
            println!("✅ Private USDT: {} base units ({:.2} USDT)", 
                balance.base_units, 
                balance.amount
            );
        }
        Err(e) => {
            println!("❌ Private USDT error: {}", e);
        }
    }

    println!("\n✨ Done!");

    Ok(())
}
