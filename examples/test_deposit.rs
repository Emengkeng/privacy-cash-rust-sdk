//! Test deposit example
//!
//! Run with:
//!   SOLANA_PRIVATE_KEY="your-base58-private-key" cargo run --example test_deposit
//!
//! Or using a JSON keypair file:
//!   SOLANA_PRIVATE_KEY=$(cat ~/.config/solana/id.json) cargo run --example test_deposit

use privacy_cash::{PrivacyCash, Signer};
use solana_sdk::signature::Keypair;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("🔒 Privacy Cash - Deposit Test\n");

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

    // Use mainnet RPC
    let rpc_url = std::env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    println!("RPC: {}\n", rpc_url);

    // Create client with circuit path
    let client = privacy_cash::PrivacyCash::with_options(
        &rpc_url,
        keypair,
        None,
        Some("./circuit/transaction2".to_string()),
    )?;

    // Check current balances
    let sol_balance = client.get_sol_balance()?;
    println!("📊 On-chain SOL: {:.6} SOL", sol_balance as f64 / 1e9);

    let private_balance = client.get_private_balance().await?;
    println!("🔐 Private SOL: {:.6} SOL\n", private_balance.lamports as f64 / 1e9);

    // Deposit amount: 0.01 SOL = 10,000,000 lamports (configurable)
    let deposit_amount: u64 = std::env::var("DEPOSIT_AMOUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000_000); // Default: 0.01 SOL
    
    println!("💰 Depositing {} SOL...", deposit_amount as f64 / 1e9);
    println!("   This will generate a ZK proof (may take a moment)...\n");

    match client.deposit(deposit_amount).await {
        Ok(result) => {
            println!("✅ Deposit successful!");
            println!("   Transaction: {}", result.signature);
            
            // Check new balances
            println!("\n📊 Checking new balances...");
            
            let new_sol_balance = client.get_sol_balance()?;
            println!("   On-chain SOL: {:.6} SOL", new_sol_balance as f64 / 1e9);
            
            // Wait a moment for indexer to update
            println!("   Waiting for indexer...");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            
            let new_private_balance = client.get_private_balance().await?;
            println!("   Private SOL: {:.6} SOL", new_private_balance.lamports as f64 / 1e9);
        }
        Err(e) => {
            println!("❌ Deposit failed: {}", e);
        }
    }

    println!("\n✨ Done!");

    Ok(())
}
