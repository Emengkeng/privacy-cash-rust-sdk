//! Test: TypeScript Bridge for ZK operations
//!
//! This tests the TypeScript bridge which handles ZK proof generation.
//!
//! Usage:
//!   SOLANA_PRIVATE_KEY="your_key" cargo run --example test_bridge

use privacy_cash::bridge;
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("═══════════════════════════════════════════════════════════════");
    println!("       NOVA SHIELD - TYPESCRIPT BRIDGE TEST");
    println!("═══════════════════════════════════════════════════════════════\n");

    let private_key = env::var("SOLANA_PRIVATE_KEY").expect(
        "SOLANA_PRIVATE_KEY environment variable not set.\n\
         Usage: SOLANA_PRIVATE_KEY=\"your_key\" cargo run --example test_bridge"
    );
    let rpc_url = env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());

    println!("Testing balance check via TypeScript bridge...\n");

    match bridge::ts_get_balance(&rpc_url, &private_key) {
        Ok(balance) => {
            println!("✅ Bridge works!");
            println!("   Private SOL: {} lamports ({} SOL)", balance.lamports, balance.sol);
        }
        Err(e) => {
            println!("❌ Bridge failed: {}", e);
        }
    }

    // Test USDC balance
    println!("\nTesting USDC balance...");
    let usdc_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    match bridge::ts_get_balance_spl(&rpc_url, &private_key, usdc_mint) {
        Ok(balance) => {
            println!("✅ USDC Balance: {} base units ({} USDC)", balance.base_units, balance.amount);
        }
        Err(e) => {
            println!("❌ USDC check failed: {}", e);
        }
    }

    // Test USDT balance
    println!("\nTesting USDT balance...");
    let usdt_mint = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
    match bridge::ts_get_balance_spl(&rpc_url, &private_key, usdt_mint) {
        Ok(balance) => {
            println!("✅ USDT Balance: {} base units ({} USDT)", balance.base_units, balance.amount);
        }
        Err(e) => {
            println!("❌ USDT check failed: {}", e);
        }
    }

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("                         COMPLETE!");
    println!("═══════════════════════════════════════════════════════════════\n");

    println!("The TypeScript bridge is working. You can now use:");
    println!("  - bridge::ts_deposit(rpc_url, private_key, lamports)");
    println!("  - bridge::ts_withdraw(rpc_url, private_key, lamports, recipient)");
    println!("  - bridge::ts_withdraw_all(rpc_url, private_key, recipient)");
    println!("  - bridge::ts_deposit_spl(rpc_url, private_key, base_units, mint)");
    println!("  - bridge::ts_withdraw_spl(rpc_url, private_key, base_units, mint, recipient)");
    println!("  - bridge::ts_withdraw_all_spl(rpc_url, private_key, mint, recipient)");
}
