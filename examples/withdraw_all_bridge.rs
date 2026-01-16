//! Test: Withdraw all tokens using TypeScript bridge
//!
//! This will withdraw ALL private balances back to the wallet.
//!
//! Usage:
//!   SOLANA_PRIVATE_KEY="your_key" cargo run --example withdraw_all_bridge

use privacy_cash::bridge;
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("═══════════════════════════════════════════════════════════════");
    println!("       NOVA SHIELD - WITHDRAW ALL VIA BRIDGE");
    println!("═══════════════════════════════════════════════════════════════\n");

    let private_key = env::var("SOLANA_PRIVATE_KEY").expect(
        "SOLANA_PRIVATE_KEY environment variable not set.\n\
         Usage: SOLANA_PRIVATE_KEY=\"your_key\" cargo run --example withdraw_all_bridge"
    );
    let rpc_url = env::var("SOLANA_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());

    // Check balances first
    println!("📊 Current Private Balances:\n");

    let sol_balance = bridge::ts_get_balance(&rpc_url, &private_key).unwrap();
    println!("   SOL: {} lamports ({} SOL)", sol_balance.lamports, sol_balance.sol);

    let usdc_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    let usdc_balance = bridge::ts_get_balance_spl(&rpc_url, &private_key, usdc_mint).unwrap();
    println!("   USDC: {} base units ({} USDC)", usdc_balance.base_units, usdc_balance.amount);

    let usdt_mint = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
    let usdt_balance = bridge::ts_get_balance_spl(&rpc_url, &private_key, usdt_mint).unwrap();
    println!("   USDT: {} base units ({} USDT)\n", usdt_balance.base_units, usdt_balance.amount);

    // Withdraw SOL
    if sol_balance.lamports > 0 {
        println!("═══════════════════════════════════════════════════════════════");
        println!("WITHDRAWING ALL SOL ({} lamports)...", sol_balance.lamports);
        println!("═══════════════════════════════════════════════════════════════\n");

        match bridge::ts_withdraw_all(&rpc_url, &private_key, None) {
            Ok(result) => {
                println!("✅ SOL Withdrawal successful!");
                println!("   TX: {}", result.signature);
                println!("   Amount: {} lamports", result.amount_in_lamports);
                println!("   Privacy Cash fee: {} lamports", result.fee_in_lamports);
                println!("   Nova Shield fee: {} lamports\n", result.nova_shield_fee);
            }
            Err(e) => {
                println!("❌ SOL Withdrawal failed: {}\n", e);
            }
        }
    } else {
        println!("⏭️  No private SOL to withdraw\n");
    }

    // Withdraw USDC
    if usdc_balance.base_units > 0 {
        println!("═══════════════════════════════════════════════════════════════");
        println!("WITHDRAWING ALL USDC ({} base units)...", usdc_balance.base_units);
        println!("═══════════════════════════════════════════════════════════════\n");

        match bridge::ts_withdraw_all_spl(&rpc_url, &private_key, usdc_mint, None) {
            Ok(result) => {
                println!("✅ USDC Withdrawal successful!");
                println!("   TX: {}", result.signature);
                println!("   Amount: {} base units", result.base_units);
                println!("   Privacy Cash fee: {} base units", result.fee_base_units);
                println!("   Nova Shield fee: {} base units\n", result.nova_shield_fee);
            }
            Err(e) => {
                println!("❌ USDC Withdrawal failed: {}\n", e);
            }
        }
    } else {
        println!("⏭️  No private USDC to withdraw\n");
    }

    // Withdraw USDT
    if usdt_balance.base_units > 0 {
        println!("═══════════════════════════════════════════════════════════════");
        println!("WITHDRAWING ALL USDT ({} base units)...", usdt_balance.base_units);
        println!("═══════════════════════════════════════════════════════════════\n");

        match bridge::ts_withdraw_all_spl(&rpc_url, &private_key, usdt_mint, None) {
            Ok(result) => {
                println!("✅ USDT Withdrawal successful!");
                println!("   TX: {}", result.signature);
                println!("   Amount: {} base units", result.base_units);
                println!("   Privacy Cash fee: {} base units", result.fee_base_units);
                println!("   Nova Shield fee: {} base units\n", result.nova_shield_fee);
            }
            Err(e) => {
                println!("❌ USDT Withdrawal failed: {}\n", e);
            }
        }
    } else {
        println!("⏭️  No private USDT to withdraw\n");
    }

    println!("═══════════════════════════════════════════════════════════════");
    println!("                         COMPLETE!");
    println!("═══════════════════════════════════════════════════════════════\n");

    println!("✨ Done! Check wallet and Nova Shield fee wallet for transfers.");
    println!("   Nova Shield fee wallet: HKBrbp3h8B9tMCn4ceKCtmF8jWxvpfrb7YNLbCgxLUJL");
}
