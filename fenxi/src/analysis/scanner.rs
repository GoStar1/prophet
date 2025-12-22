use crate::analysis::bollinger::{check_4h_volume_condition, BollingerCalculator};
use crate::error::Result;
use crate::loader::{KlineLoader, MetricsLoader};
use crate::models::BuySignal;

const BOLL_PERIOD: usize = 400;
const BOLL_STD_DEV: f64 = 2.0;
const HISTORY_CHECK_COUNT: usize = 50;
const HISTORY_THRESHOLD: usize = 25;
const OI_MULTIPLIER: f64 = 0.91;

pub struct Scanner {
    calculator: BollingerCalculator,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            calculator: BollingerCalculator::new(BOLL_PERIOD, BOLL_STD_DEV),
        }
    }

    pub fn scan_symbol(
        &self,
        symbol: &str,
        kline_15m: &mut KlineLoader,
        kline_30m: &mut KlineLoader,
        kline_4h: &mut KlineLoader,
        metrics: &mut MetricsLoader,
    ) -> Result<Vec<BuySignal>> {
        let mut signals = Vec::new();

        kline_15m.fill_initial_buffer()?;
        kline_30m.fill_initial_buffer()?;
        kline_4h.fill_initial_buffer()?;
        metrics.fill_initial_buffer()?;

        let has_metrics = metrics.has_data();

        loop {
            let current_15m = match kline_15m.current() {
                Some(k) => k.clone(),
                None => break,
            };

            let timestamp = current_15m.close_time;
            let price = current_15m.close;

            kline_30m.advance_until(timestamp)?;
            kline_4h.advance_until(timestamp)?;
            if has_metrics {
                metrics.advance_until(timestamp)?;
            }

            let klines_15m: Vec<_> = kline_15m
                .window()
                .iter()
                .filter(|k| k.close_time <= timestamp)
                .collect();
            let klines_30m: Vec<_> = kline_30m
                .window()
                .iter()
                .filter(|k| k.close_time <= timestamp)
                .collect();
            let klines_4h: Vec<_> = kline_4h
                .window()
                .iter()
                .filter(|k| k.close_time <= timestamp)
                .collect();

            if klines_15m.len() < BOLL_PERIOD
                || klines_30m.len() < BOLL_PERIOD
                || klines_4h.len() < BOLL_PERIOD
            {
                if kline_15m.advance()?.is_none() {
                    break;
                }
                continue;
            }

            let boll_15m = self.calculator.calculate(&klines_15m)?;
            let boll_30m = self.calculator.calculate(&klines_30m)?;
            let boll_4h = self.calculator.calculate(&klines_4h)?;

            let cond1 = price > boll_15m.upper;
            let cond2 = price > boll_30m.middle;
            let cond3 = price > boll_4h.middle;

            let cond4 = self.calculator.check_history_condition(
                &klines_15m,
                boll_15m.upper,
                HISTORY_CHECK_COUNT,
                HISTORY_THRESHOLD,
            );

            let cond5 = self.calculator.check_history_condition(
                &klines_30m,
                boll_30m.middle,
                HISTORY_CHECK_COUNT,
                HISTORY_THRESHOLD,
            );

            let (current_oi, min_oi_3d, cond6) = if has_metrics {
                let current_oi = metrics.get_current_oi(timestamp).unwrap_or(0.0);
                let min_oi_3d = metrics.get_min_oi_3days(timestamp).unwrap_or(0.0);
                let cond = current_oi * OI_MULTIPLIER > min_oi_3d;
                (current_oi, min_oi_3d, cond)
            } else {
                (0.0, 0.0, true)
            };

            let (cond7, volume_ratio) = check_4h_volume_condition(&klines_4h);

            if cond1 && cond2 && cond3 && cond4 && cond5 && cond6 && cond7 {
                let signal = BuySignal::new(
                    timestamp,
                    symbol.to_string(),
                    price,
                    boll_15m.upper,
                    boll_30m.middle,
                    boll_4h.middle,
                    current_oi,
                    min_oi_3d,
                    volume_ratio,
                );
                signals.push(signal);
            }

            if kline_15m.advance()?.is_none() {
                break;
            }
        }

        Ok(signals)
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}
