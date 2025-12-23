use crate::error::Result;
use crate::models::{BuySignal, Kline, TradeResult};
use std::fs::{self, File};
use std::path::Path;

const BOLL_PERIOD: usize = 400;
const BOLL_STD_DEV: f64 = 2.0;
const HISTORY_CHECK_COUNT: usize = 50;
const HISTORY_THRESHOLD: usize = 25;
const COOLDOWN_MS: i64 = 2 * 24 * 60 * 60 * 1000; // 2天冷却期

#[derive(Debug, Clone)]
struct BollValue {
    upper: f64,
    middle: f64,
}

pub struct FastScanner;

impl FastScanner {
    pub fn new() -> Self {
        Self
    }

    pub fn scan_symbol(&self, data_path: &Path, symbol: &str) -> Result<Vec<BuySignal>> {
        // 一次性加载所有数据
        let klines_15m = Self::load_all_klines(data_path, symbol, "15m")?;
        let klines_30m = Self::load_all_klines(data_path, symbol, "30m")?;
        let klines_4h = Self::load_all_klines(data_path, symbol, "4h")?;

        if klines_15m.len() < BOLL_PERIOD
            || klines_30m.len() < BOLL_PERIOD
            || klines_4h.len() < BOLL_PERIOD
        {
            return Ok(Vec::new());
        }

        // 预计算布林带
        let boll_15m = Self::calc_all_boll(&klines_15m);
        let boll_30m = Self::calc_all_boll(&klines_30m);
        let boll_4h = Self::calc_all_boll(&klines_4h);

        let mut signals = Vec::new();

        // 用索引追踪30m、4h的位置
        let mut idx_30m = 0usize;
        let mut idx_4h = 0usize;
        let mut last_signal_time: Option<i64> = None; // 上次信号时间，用于冷却期

        for i in (BOLL_PERIOD - 1)..klines_15m.len() {
            let k15 = &klines_15m[i];
            let timestamp = k15.close_time;
            let price = k15.close;

            // 冷却期检查：2天内不再检测
            if let Some(last_time) = last_signal_time {
                if timestamp - last_time < COOLDOWN_MS {
                    continue;
                }
            }

            // 同步30m索引
            while idx_30m + 1 < boll_30m.len()
                && klines_30m[idx_30m + BOLL_PERIOD].close_time <= timestamp
            {
                idx_30m += 1;
            }

            // 同步4h索引
            while idx_4h + 1 < boll_4h.len()
                && klines_4h[idx_4h + BOLL_PERIOD].close_time <= timestamp
            {
                idx_4h += 1;
            }

            if idx_30m >= boll_30m.len() || idx_4h >= boll_4h.len() {
                continue;
            }

            let b15 = &boll_15m[i - BOLL_PERIOD + 1];
            let b30 = &boll_30m[idx_30m];
            let b4h = &boll_4h[idx_4h];

            // 条件1-3
            let cond1 = price > b15.upper;
            let cond2 = price > b30.middle;
            let cond3 = price > b4h.middle;

            if !cond1 || !cond2 || !cond3 {
                continue;
            }

            // 条件4: 15m最近50根中25根以上 < 上轨
            let start_4 = if i >= HISTORY_CHECK_COUNT { i - HISTORY_CHECK_COUNT + 1 } else { 0 };
            let count_below_upper = klines_15m[start_4..=i]
                .iter()
                .filter(|k| k.close < b15.upper)
                .count();
            let cond4 = count_below_upper >= HISTORY_THRESHOLD;

            if !cond4 {
                continue;
            }

            // 条件5: 4h成交量
            let (cond7, volume_ratio) = Self::check_4h_volume(&klines_4h, idx_4h + BOLL_PERIOD);

            if !cond7 {
                continue;
            }

            signals.push(BuySignal::new(
                timestamp,
                symbol.to_string(),
                price,
                b15.upper,
                b30.middle,
                b4h.middle,
                volume_ratio,
            ));
            last_signal_time = Some(timestamp); // 记录信号时间，开始2天冷却
        }

        Ok(signals)
    }

    /// 扫描交易对并返回完整的交易记录（含卖出点）
    pub fn scan_symbol_trades(&self, data_path: &Path, symbol: &str) -> Result<Vec<TradeResult>> {
        let klines_15m = Self::load_all_klines(data_path, symbol, "15m")?;
        let klines_30m = Self::load_all_klines(data_path, symbol, "30m")?;
        let klines_4h = Self::load_all_klines(data_path, symbol, "4h")?;

        if klines_15m.len() < BOLL_PERIOD
            || klines_30m.len() < BOLL_PERIOD
            || klines_4h.len() < BOLL_PERIOD
        {
            return Ok(Vec::new());
        }

        let boll_15m = Self::calc_all_boll(&klines_15m);
        let boll_30m = Self::calc_all_boll(&klines_30m);
        let boll_4h = Self::calc_all_boll(&klines_4h);

        let mut trades = Vec::new();

        let mut idx_30m = 0usize;
        let mut idx_4h = 0usize;
        let mut last_signal_time: Option<i64> = None;

        for i in (BOLL_PERIOD - 1)..klines_15m.len() {
            let k15 = &klines_15m[i];
            let timestamp = k15.close_time;
            let price = k15.close;

            // 冷却期检查
            if let Some(last_time) = last_signal_time {
                if timestamp - last_time < COOLDOWN_MS {
                    continue;
                }
            }

            // 同步30m索引
            while idx_30m + 1 < boll_30m.len()
                && klines_30m[idx_30m + BOLL_PERIOD].close_time <= timestamp
            {
                idx_30m += 1;
            }

            // 同步4h索引
            while idx_4h + 1 < boll_4h.len()
                && klines_4h[idx_4h + BOLL_PERIOD].close_time <= timestamp
            {
                idx_4h += 1;
            }

            if idx_30m >= boll_30m.len() || idx_4h >= boll_4h.len() {
                continue;
            }

            let b15 = &boll_15m[i - BOLL_PERIOD + 1];
            let b30 = &boll_30m[idx_30m];
            let b4h = &boll_4h[idx_4h];

            // 条件1-3
            if price <= b15.upper || price <= b30.middle || price <= b4h.middle {
                continue;
            }

            // 条件4
            let start_4 = if i >= HISTORY_CHECK_COUNT { i - HISTORY_CHECK_COUNT + 1 } else { 0 };
            let count_below_upper = klines_15m[start_4..=i]
                .iter()
                .filter(|k| k.close < b15.upper)
                .count();
            if count_below_upper < HISTORY_THRESHOLD {
                continue;
            }

            // 条件5: 4h成交量
            let (cond7, _) = Self::check_4h_volume(&klines_4h, idx_4h + BOLL_PERIOD);
            if !cond7 {
                continue;
            }

            // 买入点确认：以下一根K线收盘价作为买入价
            if i + 1 >= klines_15m.len() {
                continue; // 没有下一根K线，跳过
            }

            let buy_k = &klines_15m[i + 1];
            let buy_time = buy_k.close_time;
            let buy_price = buy_k.close;

            // 从买入K线的下一根开始找卖出点
            for j in (i + 2)..klines_15m.len() {
                let sell_k = &klines_15m[j];
                let sell_boll_idx = j - BOLL_PERIOD + 1;

                if sell_boll_idx >= boll_15m.len() {
                    break;
                }

                let sell_boll = &boll_15m[sell_boll_idx];

                // 收盘价跌破布林上轨，卖出
                if sell_k.close < sell_boll.upper {
                    trades.push(TradeResult::new(
                        symbol.to_string(),
                        buy_time,
                        buy_price,
                        sell_k.close_time,
                        sell_k.close,
                    ));
                    last_signal_time = Some(sell_k.close_time); // 卖出后开始冷却
                    break;
                }
            }
        }

        Ok(trades)
    }

    fn load_all_klines(data_path: &Path, symbol: &str, interval: &str) -> Result<Vec<Kline>> {
        let dir = data_path.join("klines").join(symbol).join(interval);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut files: Vec<_> = fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "csv").unwrap_or(false))
            .collect();
        files.sort();

        let mut all_klines = Vec::new();
        for file in files {
            let f = File::open(&file)?;
            let mut rdr = csv::Reader::from_reader(f);
            for result in rdr.deserialize::<Kline>() {
                if let Ok(k) = result {
                    if k.is_valid() {
                        all_klines.push(k);
                    }
                }
            }
        }
        Ok(all_klines)
    }


    fn calc_all_boll(klines: &[Kline]) -> Vec<BollValue> {
        if klines.len() < BOLL_PERIOD {
            return Vec::new();
        }

        let mut results = Vec::with_capacity(klines.len() - BOLL_PERIOD + 1);

        // 使用滑动窗口增量计算
        let mut sum: f64 = klines[..BOLL_PERIOD].iter().map(|k| k.close).sum();
        let mut sum_sq: f64 = klines[..BOLL_PERIOD].iter().map(|k| k.close * k.close).sum();

        for i in (BOLL_PERIOD - 1)..klines.len() {
            if i > BOLL_PERIOD - 1 {
                let old = klines[i - BOLL_PERIOD].close;
                let new = klines[i].close;
                sum += new - old;
                sum_sq += new * new - old * old;
            }

            let mean = sum / BOLL_PERIOD as f64;
            let variance = (sum_sq / BOLL_PERIOD as f64) - mean * mean;
            let std_dev = variance.max(0.0).sqrt();

            results.push(BollValue {
                upper: mean + std_dev * BOLL_STD_DEV,
                middle: mean,
            });
        }

        results
    }

    fn check_4h_volume(klines: &[Kline], current_idx: usize) -> (bool, f64) {
        if current_idx < 6 || current_idx >= klines.len() {
            return (false, 0.0);
        }

        let latest_vol = klines[current_idx].volume;
        let sum_6: f64 = klines[(current_idx - 6)..current_idx]
            .iter()
            .map(|k| k.volume)
            .sum();

        let ratio = if sum_6 > 0.0 {
            latest_vol * 2.0 / sum_6
        } else {
            0.0
        };

        (latest_vol * 2.0 > sum_6, ratio)
    }
}

impl Default for FastScanner {
    fn default() -> Self {
        Self::new()
    }
}
