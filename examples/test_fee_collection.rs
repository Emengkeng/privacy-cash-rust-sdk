//! Test: Deposit then withdraw to verify Nova Shield fee collection
//!
//! This will:
//! 1. Deposit 0.02 SOL
//! 2. Withdraw it back (collecting 1% Nova Shield fee)
//!
//! Usage:
//!   SOLANA_PRIVATE_KEY="your_key" cargo run --example test_fee_collection

use privacy_cash::{PrivacyCash, Signer};
use solana_sdk::signature::Keypair;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("═══════════════════════════════════════════════════════════════");
    println!("       NOVA SHIELD - FEE COLLECTION TEST");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Parse private key from environment
    let private_key = env::var("SOLANA_PRIVATE_KEY").expect(
        "SOLANA_PRIVATE_KEY environment variable not set.\n\
         Usage: SOLANA_PRIVATE_KEY=\"your_key\" cargo run --example test_fee_collection"
    );
    let key_bytes = bs58::decode(&private_key).into_vec()?;
    let keypair = Keypair::from_bytes(&key_bytes)?;

    println!("Wallet: {}", keypair.pubkey());
    println!("Nova Shield Fee Wallet: HKBrbp3h8B9tMCn4ceKCtmF8jWxvpfrb7YNLbCgxLUJL");

    let rpc_url = env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    println!("RPC: {}\n", rpc_url);

    // Create client
    let client = PrivacyCash::with_options(
        &rpc_url,
        keypair,
        None,
        Some("./circuit/transaction2".to_string()),
    )?;

    // Check initial balances
    let public_sol = client.get_sol_balance()?;
    let private_sol = client.get_private_balance().await?;
    
    println!("📊 Initial Balances:");
    println!("   Public SOL: {:.6} SOL", public_sol as f64 / 1e9);
    println!("   Private SOL: {:.6} SOL\n", private_sol.lamports as f64 / 1e9);

    // Estimate fees for withdrawal
    let deposit_amount = 20_000_000u64; // 0.02 SOL
    let (pc_fee, ns_fee, total_fee) = client.estimate_withdraw_fees(deposit_amount).await?;
    
    println!("💰 Fee Estimate for withdrawing 0.02 SOL:");
    println!("   Privacy Cash fee: {} lamports ({:.6} SOL)", pc_fee, pc_fee as f64 / 1e9);
    println!("   Nova Shield fee (1%): {} lamports ({:.6} SOL)", ns_fee, ns_fee as f64 / 1e9);
    println!("   Total fees: {} lamports ({:.6} SOL)\n", total_fee, total_fee as f64 / 1e9);

    // Step 1: Deposit
    println!("═══════════════════════════════════════════════════════════════");
    println!("STEP 1: Depositing 0.02 SOL into Privacy Cash");
    println!("═══════════════════════════════════════════════════════════════\n");

    println!("💰 Depositing {} lamports...", deposit_amount);
    println!("   (Generating ZK proof - may take 30-60 seconds)\n");

    match client.deposit(deposit_amount).await {
        Ok(result) => {
            println!("   ✅ Deposit successful!");
            println!("   TX: {}\n", result.signature);
        }
        Err(e) => {
            println!("   ❌ Deposit failed: {}", e);
            return Ok(());
        }
    }

    // Wait for indexer
    println!("   Waiting 10 seconds for indexer...\n");
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Check balance after deposit
    let private_sol = client.get_private_balance().await?;
    println!("📊 After Deposit:");
    println!("   Private SOL: {:.6} SOL\n", private_sol.lamports as f64 / 1e9);

    // Step 2: Withdraw
    println!("═══════════════════════════════════════════════════════════════");
    println!("STEP 2: Withdrawing ALL (this will collect 1% Nova Shield fee)");
    println!("═══════════════════════════════════════════════════════════════\n");

    if private_sol.lamports > 0 {
        println!("💸 Withdrawing {} lamports...", private_sol.lamports);
        println!("   Nova Shield 1% fee: {} lamports will be sent to fee wallet", 
            (private_sol.lamports as f64 * 0.01) as u64);
        println!("   (Generating ZK proof - may take 30-60 seconds)\n");

        match client.withdraw_all(None).await {
            Ok(result) => {
                println!("   ✅ Withdrawal successful!");
                println!("   TX: {}", result.signature);
                println!("   Amount received: {} lamports", result.amount_in_lamports);
                println!("   Privacy Cash fee: {} lamports", result.fee_in_lamports);
                println!("\n   🎉 Nova Shield fee should now be in HKBrbp3h8B9tMCn4ceKCtmF8jWxvpfrb7YNLbCgxLUJL!");
            }
            Err(e) => {
                println!("   ❌ Withdrawal failed: {}", e);
            }
        }
    } else {
        println!("⚠️ No private SOL to withdraw!");
    }

    // Final balances
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("                         FINAL BALANCES");
    println!("═══════════════════════════════════════════════════════════════\n");

    let final_public = client.get_sol_balance()?;
    let final_private = client.get_private_balance().await?;
    
    println!("📊 Final:");
    println!("   Public SOL: {:.6} SOL", final_public as f64 / 1e9);
    println!("   Private SOL: {:.6} SOL", final_private.lamports as f64 / 1e9);

    println!("\n✨ Done! Check HKBrbp3h8B9tMCn4ceKCtmF8jWxvpfrb7YNLbCgxLUJL for the fee.");

    Ok(())
}
