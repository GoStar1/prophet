use chrono::{TimeZone, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BuySignal {
    pub timestamp: i64,
    pub datetime: String,
    pub symbol: String,
    pub price: f64,
    pub boll_15m_upper: f64,
    pub boll_30m_middle: f64,
    pub boll_4h_middle: f64,
    pub current_oi: f64,
    pub min_oi_3d: f64,
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
        current_oi: f64,
        min_oi_3d: f64,
        volume_ratio: f64,
    ) -> Self {
        let datetime = Utc
            .timestamp_millis_opt(timestamp)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "Invalid".to_string());

        Self {
            timestamp,
            datetime,
            symbol,
            price,
            boll_15m_upper,
            boll_30m_middle,
            boll_4h_middle,
            current_oi,
            min_oi_3d,
            volume_ratio,
        }
    }
}
