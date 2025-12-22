use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Insufficient data: required {required}, actual {actual}")]
    InsufficientData { required: usize, actual: usize },

    #[error("No data available for symbol: {0}")]
    NoData(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
