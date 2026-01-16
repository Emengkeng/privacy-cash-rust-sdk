//! Error types for Privacy Cash SDK

use thiserror::Error;

/// Result type alias for Privacy Cash operations
pub type Result<T> = std::result::Result<T, PrivacyCashError>;

/// Errors that can occur when using the Privacy Cash SDK
#[derive(Error, Debug)]
pub enum PrivacyCashError {
    /// Invalid keypair or private key
    #[error("Invalid keypair: {0}")]
    InvalidKeypair(String),

    /// Insufficient balance for operation
    #[error("Insufficient balance: have {have} lamports, need {need} lamports")]
    InsufficientBalance { have: u64, need: u64 },

    /// Insufficient SPL token balance
    #[error("Insufficient {token} balance: have {have}, need {need}")]
    InsufficientTokenBalance {
        token: String,
        have: u64,
        need: u64,
    },

    /// No UTXOs available for withdrawal
    #[error("No UTXOs available for withdrawal")]
    NoUtxosAvailable,

    /// Deposit amount exceeds limit
    #[error("Deposit amount {amount} exceeds limit {limit}")]
    DepositLimitExceeded { amount: u64, limit: u64 },

    /// Withdrawal amount too low
    #[error("Withdrawal amount too low, minimum is {minimum}")]
    WithdrawalAmountTooLow { minimum: u64 },

    /// Token not supported
    #[error("Token not supported: {0}")]
    TokenNotSupported(String),

    /// Encryption error
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    /// Decryption error
    #[error("Decryption error: {0}")]
    DecryptionError(String),

    /// Proof generation error
    #[error("Proof generation error: {0}")]
    ProofGenerationError(String),

    /// Merkle proof error
    #[error("Merkle proof error: {0}")]
    MerkleProofError(String),

    /// API request error
    #[error("API request error: {0}")]
    ApiError(String),

    /// Transaction error
    #[error("Transaction error: {0}")]
    TransactionError(String),

    /// Transaction confirmation timeout
    #[error("Transaction confirmation timeout after {retries} retries")]
    ConfirmationTimeout { retries: u32 },

    /// Solana client error
    #[error("Solana client error: {0}")]
    SolanaClientError(#[from] solana_client::client_error::ClientError),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// HTTP request error
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// JSON parsing error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Circuit file not found
    #[error("Circuit file not found: {0}")]
    CircuitNotFound(String),

    /// Operation aborted
    #[error("Operation aborted")]
    Aborted,
}
