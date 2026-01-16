//! Constants used throughout the Privacy Cash SDK

use once_cell::sync::Lazy;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// BN254 field size used in ZK circuits
pub static FIELD_SIZE: Lazy<num_bigint::BigUint> = Lazy::new(|| {
    num_bigint::BigUint::parse_bytes(
        b"21888242871839275222246405745257275088548364400416034343698204186575808495617",
        10,
    )
    .unwrap()
});

/// Privacy Cash program ID on Solana mainnet
pub static PROGRAM_ID: Lazy<Pubkey> = Lazy::new(|| {
    std::env::var("PROGRAM_ID")
        .ok()
        .and_then(|s| Pubkey::from_str(&s).ok())
        .unwrap_or_else(|| {
            Pubkey::from_str("9fhQBbumKEFuXtMBDw8AaQyAjCorLGJQiS3skWZdQyQD").unwrap()
        })
});

/// Fee recipient address (Privacy Cash)
pub static FEE_RECIPIENT: Lazy<Pubkey> = Lazy::new(|| {
    Pubkey::from_str("AWexibGxNFKTa1b5R5MN4PJr9HWnWRwf8EW9g8cLx3dM").unwrap()
});

/// Nova Shield referrer wallet - earns referral fees on all transactions
/// Set NOVA_SHIELD_REFERRER env var to override, or None to disable
pub static NOVA_SHIELD_REFERRER: Lazy<Option<String>> = Lazy::new(|| {
    std::env::var("NOVA_SHIELD_REFERRER").ok().or_else(|| {
        // Nova Shield wallet receives referral fees from Privacy Cash
        Some("HKBrbp3h8B9tMCn4ceKCtmF8jWxvpfrb7YNLbCgxLUJL".to_string())
    })
});

/// Address Lookup Table address
pub static ALT_ADDRESS: Lazy<Pubkey> = Lazy::new(|| {
    std::env::var("ALT_ADDRESS")
        .ok()
        .and_then(|s| Pubkey::from_str(&s).ok())
        .unwrap_or_else(|| {
            Pubkey::from_str("HEN49U2ySJ85Vc78qprSW9y6mFDhs1NczRxyppNHjofe").unwrap()
        })
});

/// Relayer API URL
pub static RELAYER_API_URL: Lazy<String> = Lazy::new(|| {
    std::env::var("RELAYER_API_URL").unwrap_or_else(|_| "https://api3.privacycash.org".to_string())
});

/// USDC mint address on mainnet
pub static USDC_MINT: Lazy<Pubkey> = Lazy::new(|| {
    std::env::var("USDC_MINT")
        .ok()
        .and_then(|s| Pubkey::from_str(&s).ok())
        .unwrap_or_else(|| {
            Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap()
        })
});

/// USDT mint address on mainnet
pub static USDT_MINT: Lazy<Pubkey> = Lazy::new(|| {
    std::env::var("USDT_MINT")
        .ok()
        .and_then(|s| Pubkey::from_str(&s).ok())
        .unwrap_or_else(|| {
            Pubkey::from_str("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB").unwrap()
        })
});

/// ZEC wrapped token mint address
pub static ZEC_MINT: Lazy<Pubkey> = Lazy::new(|| {
    Pubkey::from_str("A7bdiYdS5GjqGFtxf17ppRHtDKPkkRqbKtR27dxvQXaS").unwrap()
});

/// ORE token mint address
pub static ORE_MINT: Lazy<Pubkey> = Lazy::new(|| {
    Pubkey::from_str("oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp").unwrap()
});

/// STORE token mint address
pub static STORE_MINT: Lazy<Pubkey> = Lazy::new(|| {
    Pubkey::from_str("sTorERYB6xAZ1SSbwpK3zoK2EEwbBrc7TZAzg1uCGiH").unwrap()
});

/// SOL "mint" address (system program placeholder)
pub static SOL_MINT: Lazy<Pubkey> = Lazy::new(|| {
    Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap()
});

/// Number of UTXOs to fetch per batch
pub const FETCH_UTXOS_GROUP_SIZE: u64 = 20_000;

/// Merkle tree depth (26 levels)
pub const MERKLE_TREE_DEPTH: usize = 26;

/// Transaction instruction discriminator for native SOL
pub const TRANSACT_IX_DISCRIMINATOR: [u8; 8] = [217, 149, 130, 143, 221, 52, 252, 119];

/// Transaction instruction discriminator for SPL tokens
pub const TRANSACT_SPL_IX_DISCRIMINATOR: [u8; 8] = [154, 66, 244, 204, 78, 225, 163, 151];

/// Sign message for deriving encryption keys
pub const SIGN_MESSAGE: &str = "Privacy Money account sign in";

/// LocalStorage key prefix for fetch offset
pub const LSK_FETCH_OFFSET: &str = "fetch_offset";

/// LocalStorage key prefix for encrypted outputs
pub const LSK_ENCRYPTED_OUTPUTS: &str = "encrypted_outputs";

/// Lamports per SOL
pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

/// Supported token information
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub name: &'static str,
    pub mint: Pubkey,
    pub prefix: &'static str,
    pub units_per_token: u64,
}

/// Get list of all supported tokens
pub fn get_supported_tokens() -> Vec<TokenInfo> {
    vec![
        TokenInfo {
            name: "sol",
            mint: *SOL_MINT,
            prefix: "",
            units_per_token: LAMPORTS_PER_SOL,
        },
        TokenInfo {
            name: "usdc",
            mint: *USDC_MINT,
            prefix: "usdc_",
            units_per_token: 1_000_000, // 6 decimals
        },
        TokenInfo {
            name: "usdt",
            mint: *USDT_MINT,
            prefix: "usdt_",
            units_per_token: 1_000_000, // 6 decimals
        },
        TokenInfo {
            name: "zec",
            mint: *ZEC_MINT,
            prefix: "zec_",
            units_per_token: 100_000_000, // 8 decimals
        },
        TokenInfo {
            name: "ore",
            mint: *ORE_MINT,
            prefix: "ore_",
            units_per_token: 100_000_000_000, // 11 decimals
        },
        TokenInfo {
            name: "store",
            mint: *STORE_MINT,
            prefix: "store_",
            units_per_token: 100_000_000_000, // 11 decimals
        },
    ]
}

/// Find token info by mint address
pub fn find_token_by_mint(mint: &Pubkey) -> Option<TokenInfo> {
    get_supported_tokens()
        .into_iter()
        .find(|t| &t.mint == mint)
}

/// Find token info by name
pub fn find_token_by_name(name: &str) -> Option<TokenInfo> {
    get_supported_tokens()
        .into_iter()
        .find(|t| t.name == name.to_lowercase())
}
