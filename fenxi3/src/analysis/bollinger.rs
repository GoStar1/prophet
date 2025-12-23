use crate::error::{AppError, Result};
use crate::models::Kline;

#[derive(Debug, Clone)]
pub struct BollingerBands {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

pub struct BollingerCalculator {
    period: usize,
    std_dev_multiplier: f64,
}

impl BollingerCalculator {
    pub fn new(period: usize, std_dev_multiplier: f64) -> Self {
        Self {
            period,
            std_dev_multiplier,
        }
    }

    pub fn calculate(&self, klines: &[&Kline]) -> Result<BollingerBands> {
        if klines.len() < self.period {
            return Err(AppError::InsufficientData {
                required: self.period,
                actual: klines.len(),
            });
        }

        let closes: Vec<f64> = klines
            .iter()
            .skip(klines.len() - self.period)
            .map(|k| k.close)
            .collect();

        let middle = closes.iter().sum::<f64>() / self.period as f64;

        let variance = closes
            .iter()
            .map(|price| {
                let diff = price - middle;
                diff * diff
            })
            .sum::<f64>()
            / self.period as f64;

        let std_dev = variance.sqrt();

        let upper = middle + (std_dev * self.std_dev_multiplier);
        let lower = middle - (std_dev * self.std_dev_multiplier);

        Ok(BollingerBands {
            upper,
            middle,
            lower,
        })
    }

    pub fn count_below_threshold(&self, klines: &[&Kline], threshold: f64, n: usize) -> usize {
        klines
            .iter()
            .rev()
            .take(n)
            .filter(|k| k.close < threshold)
            .count()
    }

    pub fn check_history_condition(
        &self,
        klines: &[&Kline],
        threshold: f64,
        check_count: usize,
        threshold_count: usize,
    ) -> bool {
        let below_count = self.count_below_threshold(klines, threshold, check_count);
        below_count >= threshold_count
    }
}

pub fn check_4h_volume_condition(klines: &[&Kline]) -> (bool, f64) {
    if klines.len() < 7 {
        return (false, 0.0);
    }
    let recent: Vec<&&Kline> = klines.iter().rev().take(7).collect();
    let latest_volume = recent[0].volume;
    let sum_of_6: f64 = recent[1..7].iter().map(|k| k.volume).sum();
    let ratio = if sum_of_6 > 0.0 {
        latest_volume * 2.0 / sum_of_6
    } else {
        0.0
    };
    (latest_volume * 2.0 > sum_of_6, ratio)
}
