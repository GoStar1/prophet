use chrono::{TimeZone, Utc};
use serde::Serialize;

fn format_timestamp(timestamp: i64) -> String {
    Utc.timestamp_millis_opt(timestamp)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "Invalid".to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeResult {
    pub symbol: String,
    pub buy_time: i64,
    pub buy_datetime: String,
    pub buy_price: f64,
    pub sell_time: i64,
    pub sell_datetime: String,
    pub sell_price: f64,
    pub profit_pct: f64,
    pub hold_hours: f64,
}

impl TradeResult {
    pub fn new(
        symbol: String,
        buy_time: i64,
        buy_price: f64,
        sell_time: i64,
        sell_price: f64,
    ) -> Self {
        let profit_pct = (sell_price - buy_price) / buy_price * 100.0;
        let hold_hours = (sell_time - buy_time) as f64 / (1000.0 * 60.0 * 60.0);

        Self {
            symbol,
            buy_time,
            buy_datetime: format_timestamp(buy_time),
            buy_price,
            sell_time,
            sell_datetime: format_timestamp(sell_time),
            sell_price,
            profit_pct,
            hold_hours,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BuySignal {
    pub timestamp: i64,
    pub datetime: String,
    pub symbol: String,
    pub price: f64,
    pub boll_15m_upper: f64,
    pub boll_30m_middle: f64,
    pub boll_4h_middle: f64,
    pub volume_ratio: f64,
}

impl BuySignal {
    pub fn new(
        timestamp: i64,
        symbol: String,
        price: f64,
        boll_15m_upper: f64,
        boll_30m_middle: f64,
        boll_4h_middle: f64,
        volume_ratio: f64,
    ) -> Self {
        Self {
            timestamp,
            datetime: format_timestamp(timestamp),
            symbol,
            price,
            boll_15m_upper,
            boll_30m_middle,
            boll_4h_middle,
            volume_ratio,
        }
    }
}
