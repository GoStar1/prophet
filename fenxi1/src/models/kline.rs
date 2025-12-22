use serde::{Deserialize, Deserializer};

fn deserialize_f64_or_default<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) if !s.is_empty() => s.parse().map_err(serde::de::Error::custom),
        _ => Ok(0.0),
    }
}

fn deserialize_i64_or_default<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) if !s.is_empty() => s.parse().map_err(serde::de::Error::custom),
        _ => Ok(0),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Kline {
    #[serde(deserialize_with = "deserialize_i64_or_default")]
    pub open_time: i64,
    #[serde(deserialize_with = "deserialize_f64_or_default")]
    pub open: f64,
    #[serde(deserialize_with = "deserialize_f64_or_default")]
    pub high: f64,
    #[serde(deserialize_with = "deserialize_f64_or_default")]
    pub low: f64,
    #[serde(deserialize_with = "deserialize_f64_or_default")]
    pub close: f64,
    #[serde(deserialize_with = "deserialize_f64_or_default")]
    pub volume: f64,
    #[serde(deserialize_with = "deserialize_i64_or_default")]
    pub close_time: i64,
    #[serde(deserialize_with = "deserialize_f64_or_default")]
    pub quote_volume: f64,
    #[serde(deserialize_with = "deserialize_i64_or_default")]
    pub count: i64,
    #[serde(deserialize_with = "deserialize_f64_or_default")]
    pub taker_buy_volume: f64,
    #[serde(deserialize_with = "deserialize_f64_or_default")]
    pub taker_buy_quote_volume: f64,
    #[serde(deserialize_with = "deserialize_i64_or_default")]
    pub ignore: i64,
}

impl Kline {
    pub fn is_valid(&self) -> bool {
        self.open_time > 0 && self.close_time > 0 && self.close > 0.0
    }
}
