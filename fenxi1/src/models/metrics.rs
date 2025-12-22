use chrono::NaiveDateTime;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Metrics {
    pub create_time: String,
    pub symbol: String,
    pub sum_open_interest: f64,
    pub sum_open_interest_value: f64,
    pub count_toptrader_long_short_ratio: f64,
    pub sum_toptrader_long_short_ratio: f64,
    pub count_long_short_ratio: f64,
    pub sum_taker_long_short_vol_ratio: f64,
}

impl Metrics {
    pub fn timestamp_ms(&self) -> i64 {
        NaiveDateTime::parse_from_str(&self.create_time, "%Y-%m-%d %H:%M:%S")
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0)
    }
}
