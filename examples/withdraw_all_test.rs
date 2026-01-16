//! Test: Withdraw all private balances for all tokens
//!
//! This will withdraw ALL private balances back to the wallet.

use privacy_cash::{PrivacyCash, Signer};
use solana_sdk::signature::Keypair;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("═══════════════════════════════════════════════════════════════");
    println!("       NOVA SHIELD - WITHDRAW ALL PRIVATE BALANCES TEST");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Parse private key
    let private_key = "2Rub9j5xV9YjzPFC7yxqfwyra5hnv3NCcRgYNcGosNp6qeJX3Fb2ppnRSwYmfFbVX9NMSh5qGvppA7qVWMmMLWMj";
    let key_bytes = bs58::decode(private_key).into_vec()?;
    let keypair = Keypair::from_bytes(&key_bytes)?;

    println!("Wallet: {}", keypair.pubkey());

    let rpc_url = "https://api.mainnet-beta.solana.com";
    println!("RPC: {}\n", rpc_url);

    // Create client with circuit path
    let client = PrivacyCash::with_options(
        rpc_url,
        keypair,
        None,
        Some("./circuit/transaction2".to_string()),
    )?;

    // Get supported tokens
    println!("📋 Fetching supported tokens...\n");
    let tokens = client.get_supported_tokens().await?;
    
    println!("Supported tokens:");
    for token in &tokens {
        println!("  - {}: min={}, rent_fee={:.4}, price=${:.2}",
            token.name.to_uppercase(), 
            token.min_withdrawal, 
            token.rent_fee, 
            token.price_usd
        );
    }

    // Check all private balances
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("                    CHECKING PRIVATE BALANCES");
    println!("═══════════════════════════════════════════════════════════════\n");

    // SOL balance
    println!("🔍 Checking private SOL...");
    let sol_balance = client.get_private_balance().await?;
    println!("   Private SOL: {} lamports ({:.6} SOL)\n", 
        sol_balance.lamports, 
        sol_balance.lamports as f64 / 1e9
    );

    // USDC balance
    println!("🔍 Checking private USDC...");
    let usdc_balance = client.get_private_balance_usdc().await?;
    println!("   Private USDC: {} base units ({:.2} USDC)\n", 
        usdc_balance.base_units, 
        usdc_balance.amount
    );

    // USDT balance
    println!("🔍 Checking private USDT...");
    let usdt_mint = solana_sdk::pubkey::Pubkey::from_str("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB")?;
    let usdt_balance = client.get_private_balance_spl(&usdt_mint).await?;
    println!("   Private USDT: {} base units ({:.2} USDT)\n", 
        usdt_balance.base_units, 
        usdt_balance.amount
    );

    // Withdraw all SOL
    println!("═══════════════════════════════════════════════════════════════");
    println!("                    WITHDRAWING ALL BALANCES");
    println!("═══════════════════════════════════════════════════════════════\n");

    // SOL
    if sol_balance.lamports > 0 {
        println!("💸 Withdrawing ALL private SOL ({:.6} SOL)...", sol_balance.lamports as f64 / 1e9);
        println!("   (This generates a ZK proof - may take 30-60 seconds)\n");
        
        match client.withdraw_all(None).await {
            Ok(result) => {
                println!("   ✅ SOL Withdrawal successful!");
                println!("   TX: {}", result.signature);
                println!("   Amount: {} lamports", result.amount_in_lamports);
                println!("   Fee: {} lamports\n", result.fee_in_lamports);
            }
            Err(e) => {
                println!("   ❌ SOL Withdrawal failed: {}\n", e);
            }
        }
    } else {
        println!("⏭️  No private SOL to withdraw\n");
    }

    // USDC
    if usdc_balance.base_units > 0 {
        println!("💸 Withdrawing ALL private USDC ({:.2} USDC)...", usdc_balance.amount);
        println!("   (This generates a ZK proof - may take 30-60 seconds)\n");
        
        match client.withdraw_all_usdc(None).await {
            Ok(result) => {
                println!("   ✅ USDC Withdrawal successful!");
                println!("   TX: {}", result.signature);
                println!("   Amount: {} base units", result.base_units);
                println!("   Fee: {} base units\n", result.fee_base_units);
            }
            Err(e) => {
                println!("   ❌ USDC Withdrawal failed: {}\n", e);
            }
        }
    } else {
        println!("⏭️  No private USDC to withdraw\n");
    }

    // USDT
    if usdt_balance.base_units > 0 {
        println!("💸 Withdrawing ALL private USDT ({:.2} USDT)...", usdt_balance.amount);
        println!("   (This generates a ZK proof - may take 30-60 seconds)\n");
        
        match client.withdraw_all_spl(&usdt_mint, None).await {
            Ok(result) => {
                println!("   ✅ USDT Withdrawal successful!");
                println!("   TX: {}", result.signature);
                println!("   Amount: {} base units", result.base_units);
                println!("   Fee: {} base units\n", result.fee_base_units);
            }
            Err(e) => {
                println!("   ❌ USDT Withdrawal failed: {}\n", e);
            }
        }
    } else {
        println!("⏭️  No private USDT to withdraw\n");
    }

    println!("═══════════════════════════════════════════════════════════════");
    println!("                         COMPLETE!");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Final balance check
    println!("📊 Final private balances:");
    
    let final_sol = client.get_private_balance().await?;
    println!("   SOL: {} lamports ({:.6} SOL)", final_sol.lamports, final_sol.lamports as f64 / 1e9);
    
    let final_usdc = client.get_private_balance_usdc().await?;
    println!("   USDC: {} base units ({:.2} USDC)", final_usdc.base_units, final_usdc.amount);
    
    let final_usdt = client.get_private_balance_spl(&usdt_mint).await?;
    println!("   USDT: {} base units ({:.2} USDT)", final_usdt.base_units, final_usdt.amount);

    println!("\n✨ Done!");

    Ok(())
}
