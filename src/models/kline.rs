use crate::error::{AppError, Result};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Kline {
    pub open_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub close_time: i64,
}

impl Kline {
    pub fn from_binance_response(data: &[Value]) -> Result<Self> {
        if data.len() < 7 {
            return Err(AppError::BinanceApi("Invalid kline data format".to_string()));
        }

        let parse_f64 = |v: &Value| -> Result<f64> {
            v.as_str()
                .ok_or_else(|| AppError::BinanceApi("Expected string".to_string()))?
                .parse()
                .map_err(|_| AppError::BinanceApi("Failed to parse float".to_string()))
        };

        Ok(Kline {
            open_time: data[0]
                .as_i64()
                .ok_or_else(|| AppError::BinanceApi("Invalid open_time".to_string()))?,
            open: parse_f64(&data[1])?,
            high: parse_f64(&data[2])?,
            low: parse_f64(&data[3])?,
            close: parse_f64(&data[4])?,
            volume: parse_f64(&data[5])?,
            close_time: data[6]
                .as_i64()
                .ok_or_else(|| AppError::BinanceApi("Invalid close_time".to_string()))?,
        })
    }
}
