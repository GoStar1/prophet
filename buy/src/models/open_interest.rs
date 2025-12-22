use serde::Deserialize;

/// 当前持仓量
#[derive(Debug, Clone, Deserialize)]
pub struct OpenInterest {
    pub symbol: String,
    #[serde(rename = "openInterest")]
    pub open_interest: String,
    pub time: i64,
}

impl OpenInterest {
    pub fn open_interest_f64(&self) -> f64 {
        self.open_interest.parse().unwrap_or(0.0)
    }
}

/// 历史持仓量
#[derive(Debug, Clone, Deserialize)]
pub struct OpenInterestHist {
    pub symbol: String,
    #[serde(rename = "sumOpenInterest")]
    pub sum_open_interest: String,
    #[serde(rename = "sumOpenInterestValue")]
    pub sum_open_interest_value: String,
    pub timestamp: i64,
}

impl OpenInterestHist {
    pub fn sum_open_interest_f64(&self) -> f64 {
        self.sum_open_interest.parse().unwrap_or(0.0)
    }
}
