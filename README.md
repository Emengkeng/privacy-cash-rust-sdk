# Privacy Cash Rust SDK

[![Crates.io](https://img.shields.io/crates/v/privacy-cash.svg)](https://crates.io/crates/privacy-cash)
[![Documentation](https://docs.rs/privacy-cash/badge.svg)](https://docs.rs/privacy-cash)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Rust SDK for [Privacy Cash](https://privacycash.org) - Privacy-preserving transactions on Solana using Zero-Knowledge Proofs.

**Created by [Nova Shield](https://nshield.org)**

[![Download on App Store](https://img.shields.io/badge/Download_on_the-App_Store-black?logo=apple&logoColor=white)](https://apps.apple.com/us/app/nova-for-solana/id6753857720)

## Features

- 🔒 **Private Transactions**: Send SOL and SPL tokens with complete privacy
- 🛡️ **Zero-Knowledge Proofs**: Industry-standard ZK-SNARKs for transaction privacy
- 💰 **Multi-Token Support**: SOL, USDC, USDT, and more
- ⚡ **Async/Await**: Built on Tokio for high-performance async operations
- 🔐 **Local Key Management**: Private keys never leave your machine

## Supported Tokens

| Token | Minimum Withdrawal | Rent Fee |
|-------|-------------------|----------|
| SOL   | 0.01 SOL          | 0.006 SOL |
| USDC  | 2 USDC            | ~0.85 USDC |
| USDT  | 2 USDT            | ~0.85 USDT |
| ZEC   | 0.01 ZEC          | ~0.002 ZEC |
| ORE   | 0.02 ORE          | ~0.007 ORE |
| STORE | 0.02 STORE        | ~0.007 STORE |

**Fee Structure:**
- Deposit Fee: **0%** (FREE)
- Withdrawal Fee: **0.35%** of amount + rent fee

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
privacy-cash = "0.1"
```

### Prerequisites

For deposit/withdrawal operations, you need `snarkjs` installed:

```bash
npm install -g snarkjs
```

Download the circuit files (required for ZK proof generation):

```bash
mkdir -p circuit
curl -L https://raw.githubusercontent.com/Privacy-Cash/privacy-cash-sdk/main/circuit2/transaction2.wasm -o circuit/transaction2.wasm
curl -L https://raw.githubusercontent.com/Privacy-Cash/privacy-cash-sdk/main/circuit2/transaction2.zkey -o circuit/transaction2.zkey
```

## Quick Start

```rust
use privacy_cash::PrivacyCash;
use solana_sdk::signature::Keypair;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load your keypair (NEVER hardcode private keys!)
    let private_key = std::env::var("SOLANA_PRIVATE_KEY")?;
    let key_bytes = bs58::decode(&private_key).into_vec()?;
    let keypair = Keypair::from_bytes(&key_bytes)?;

    // Create client
    let client = PrivacyCash::new("https://api.mainnet-beta.solana.com", keypair)?;

    // Check private balance
    let balance = client.get_private_balance().await?;
    println!("Private SOL: {} lamports", balance.lamports);

    // Deposit 0.01 SOL (10,000,000 lamports)
    let deposit = client.deposit(10_000_000).await?;
    println!("Deposit tx: {}", deposit.signature);

    // Withdraw 0.005 SOL to self
    let withdraw = client.withdraw(5_000_000, None).await?;
    println!("Withdraw tx: {}", withdraw.signature);

    Ok(())
}
```

## Examples

### Check Balances (Safe - No Transactions)

```bash
SOLANA_PRIVATE_KEY="your-base58-key" cargo run --example check_balance
```

### Deposit SOL

```bash
SOLANA_PRIVATE_KEY="your-base58-key" cargo run --example test_deposit
```

### Full Example

```bash
SOLANA_PRIVATE_KEY="your-base58-key" cargo run --example basic_usage
```

## API Reference

### Creating a Client

```rust
// Basic client
let client = PrivacyCash::new(rpc_url, keypair)?;

// With custom circuit path
let client = PrivacyCash::with_options(
    rpc_url,
    keypair,
    None,  // Optional storage path
    Some("./circuit/transaction2".to_string()),
)?;
```

### Balance Methods

```rust
// On-chain SOL balance
let balance = client.get_sol_balance()?;

// Private SOL balance
let private_sol = client.get_private_balance().await?;

// Private USDC balance
let private_usdc = client.get_private_balance_usdc().await?;

// Private balance for any SPL token
let mint = Pubkey::from_str("...")?;
let balance = client.get_private_balance_spl(&mint).await?;
```

### Deposit Methods

```rust
// Deposit SOL (amount in lamports)
let result = client.deposit(10_000_000).await?;

// Deposit USDC (amount in base units, 1 USDC = 1,000,000)
let result = client.deposit_usdc(1_000_000).await?;

// Deposit any SPL token
let result = client.deposit_spl(amount, &mint).await?;
```

### Withdraw Methods

```rust
// Withdraw SOL (amount in lamports)
// recipient: None = withdraw to self
let result = client.withdraw(5_000_000, None).await?;

// Withdraw to specific address
let recipient = Pubkey::from_str("...")?;
let result = client.withdraw(5_000_000, Some(&recipient)).await?;

// Withdraw USDC
let result = client.withdraw_usdc(500_000, None).await?;

// Withdraw any SPL token
let result = client.withdraw_spl(amount, &mint, None).await?;

// ⭐ WITHDRAW ALL - Simple one-call withdrawal of entire balance
let result = client.withdraw_all(None).await?;           // All SOL
let result = client.withdraw_all_usdc(None).await?;      // All USDC
let result = client.withdraw_all_spl(&mint, None).await?; // All of any token
```

## Security

⚠️ **IMPORTANT**: 

- **Never hardcode private keys** in your code
- Use environment variables or secure key management
- Private keys are used locally and never sent to any server
- All ZK proofs are generated client-side

## How It Works

1. **Deposit**: Your tokens are deposited into the Privacy Cash program, and an encrypted UTXO is created
2. **ZK Proof**: A zero-knowledge proof is generated proving you own the UTXO without revealing which one
3. **Withdraw**: The proof is verified on-chain, and tokens are sent to the recipient
4. **Privacy**: The link between deposit and withdrawal is cryptographically hidden

## License

MIT License - Copyright © 2026 Nova Shield

See [LICENSE](LICENSE) for details.

## Links

- [Nova Shield](https://nshield.org) - Created by Nova Shield
- [Nova for Solana - iOS App](https://apps.apple.com/us/app/nova-for-solana/id6753857720) - Download on the App Store
- [Privacy Cash Protocol](https://privacycash.org) - The underlying privacy protocol
- [Privacy Cash TypeScript SDK](https://github.com/Privacy-Cash/privacy-cash-sdk)
- [Security Audits](https://github.com/Privacy-Cash/privacy-cash-sdk/tree/main/audits)
