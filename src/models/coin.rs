use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinInfo {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub current_price: f64,
    pub market_cap: f64,
    pub market_cap_rank: Option<u32>,
    #[serde(skip)]
    pub binance_symbol: Option<String>,
    #[serde(skip)]
    pub futures_symbol: Option<String>,
}

/// 多时间周期布林带数据
#[derive(Debug, Clone)]
pub struct MultiTimeframeBoll {
    pub boll_15m_upper: f64,
    pub boll_15m_middle: f64,
    pub boll_30m_upper: f64,
    pub boll_30m_middle: f64,
    pub boll_4h_upper: f64,
    pub boll_4h_middle: f64,
}

/// 分析结果
#[derive(Debug, Clone)]
pub struct AnalyzedCoin {
    pub coin: CoinInfo,
    pub current_price: f64,
    pub boll: MultiTimeframeBoll,
    // 6个条件的结果
    pub cond1_price_above_15m_upper: bool,
    pub cond2_price_above_30m_middle: bool,
    pub cond3_price_above_4h_middle: bool,
    pub cond4_15m_history_below_upper: bool,
    pub cond5_30m_history_below_middle: bool,
    pub cond6_oi_condition: bool,
    // 持仓量数据
    pub current_oi: f64,
    pub min_oi_3d: f64,
}

impl AnalyzedCoin {
    /// 检查是否满足所有6个条件
    pub fn meets_all_conditions(&self) -> bool {
        self.cond1_price_above_15m_upper
            && self.cond2_price_above_30m_middle
            && self.cond3_price_above_4h_middle
            && self.cond4_15m_history_below_upper
            && self.cond5_30m_history_below_middle
            && self.cond6_oi_condition
    }
}
