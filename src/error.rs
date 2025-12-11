use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("CoinGecko API error: {0}")]
    CoinGeckoApi(String),

    #[error("Binance API error: {0}")]
    BinanceApi(String),

    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),

    #[error("Insufficient kline data: need {required}, got {actual}")]
    InsufficientData { required: usize, actual: usize },

    #[error("Email sending failed: {0}")]
    EmailError(String),

    #[error("Configuration error: {0}")]
    ConfigError(#[from] config::ConfigError),

    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Symbol not found on Binance: {0}")]
    SymbolNotFound(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
