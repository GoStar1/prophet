use crate::error::Result;
use crate::models::{BuySignal, Kline, Metrics};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::path::Path;

const BOLL_PERIOD: usize = 400;
const BOLL_STD_DEV: f64 = 2.0;
const HISTORY_CHECK_COUNT: usize = 50;
const HISTORY_THRESHOLD: usize = 25;
const OI_MULTIPLIER: f64 = 0.91;
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
        let metrics = Self::load_all_metrics(data_path, symbol)?;

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

        let has_metrics = !metrics.is_empty();
        let mut signals = Vec::new();

        // 用索引追踪30m、4h和metrics的位置
        let mut idx_30m = 0usize;
        let mut idx_4h = 0usize;
        let mut metrics_start = 0usize;
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

            // 条件5: 30m最近50根中25根以上 < 中轨
            let start_5 = if idx_30m + BOLL_PERIOD >= HISTORY_CHECK_COUNT {
                idx_30m + BOLL_PERIOD - HISTORY_CHECK_COUNT + 1
            } else {
                0
            };
            let end_5 = idx_30m + BOLL_PERIOD;
            let count_below_middle = klines_30m[start_5..=end_5.min(klines_30m.len() - 1)]
                .iter()
                .filter(|k| k.close < b30.middle)
                .count();
            let cond5 = count_below_middle >= HISTORY_THRESHOLD;

            if !cond5 {
                continue;
            }

            // 条件6: 持仓量
            let (current_oi, min_oi_3d, cond6) = if has_metrics {
                Self::check_oi_condition(&metrics, timestamp, &mut metrics_start)
            } else {
                (0.0, 0.0, true)
            };

            if !cond6 {
                continue;
            }

            // 条件7: 4h成交量
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
                current_oi,
                min_oi_3d,
                volume_ratio,
            ));
            last_signal_time = Some(timestamp); // 记录信号时间，开始2天冷却
        }

        Ok(signals)
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

    fn load_all_metrics(data_path: &Path, symbol: &str) -> Result<Vec<Metrics>> {
        let dir = data_path.join("metrics").join(symbol);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut files: Vec<_> = fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "csv").unwrap_or(false))
            .collect();
        files.sort();

        let mut all_metrics = Vec::new();
        for file in files {
            let f = File::open(&file)?;
            let mut rdr = csv::Reader::from_reader(f);
            for result in rdr.deserialize::<Metrics>() {
                if let Ok(m) = result {
                    all_metrics.push(m);
                }
            }
        }
        Ok(all_metrics)
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

    fn check_oi_condition(
        metrics: &[Metrics],
        timestamp: i64,
        start_idx: &mut usize,
    ) -> (f64, f64, bool) {
        let three_days_ms = 3 * 24 * 60 * 60 * 1000_i64;
        let start_time = timestamp - three_days_ms;

        // 快进到接近当前时间的位置
        while *start_idx + 1 < metrics.len() && metrics[*start_idx + 1].timestamp_ms() <= timestamp
        {
            *start_idx += 1;
        }

        let current_oi = if *start_idx < metrics.len() && metrics[*start_idx].timestamp_ms() <= timestamp {
            metrics[*start_idx].sum_open_interest
        } else {
            return (0.0, 0.0, true);
        };

        // 找3天内最低持仓量
        let mut min_oi = f64::MAX;
        let mut j = *start_idx;
        while j > 0 && metrics[j].timestamp_ms() >= start_time {
            min_oi = min_oi.min(metrics[j].sum_open_interest);
            j -= 1;
        }
        if j < metrics.len() && metrics[j].timestamp_ms() >= start_time {
            min_oi = min_oi.min(metrics[j].sum_open_interest);
        }

        if min_oi == f64::MAX {
            return (current_oi, 0.0, true);
        }

        (current_oi, min_oi, current_oi * OI_MULTIPLIER > min_oi)
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
