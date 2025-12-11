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

    pub fn calculate(&self, klines: &[Kline]) -> Result<BollingerBands> {
        if klines.len() < self.period {
            return Err(AppError::InsufficientData {
                required: self.period,
                actual: klines.len(),
            });
        }

        // Use the most recent 'period' closing prices
        let closes: Vec<f64> = klines
            .iter()
            .skip(klines.len() - self.period)
            .map(|k| k.close)
            .collect();

        // Calculate Simple Moving Average (SMA)
        let middle = closes.iter().sum::<f64>() / self.period as f64;

        // Calculate Standard Deviation
        let variance = closes
            .iter()
            .map(|price| {
                let diff = price - middle;
                diff * diff
            })
            .sum::<f64>()
            / self.period as f64;

        let std_dev = variance.sqrt();

        // Calculate upper and lower bands
        let upper = middle + (std_dev * self.std_dev_multiplier);
        let lower = middle - (std_dev * self.std_dev_multiplier);

        Ok(BollingerBands {
            upper,
            middle,
            lower,
        })
    }

    /// 检查最近n根K线中，有多少根收盘价低于指定阈值
    pub fn count_below_threshold(&self, klines: &[Kline], threshold: f64, n: usize) -> usize {
        klines
            .iter()
            .rev()
            .take(n)
            .filter(|k| k.close < threshold)
            .count()
    }

    /// 检查历史条件：最近n根K线中，有threshold_count根以上低于阈值
    pub fn check_history_condition(
        &self,
        klines: &[Kline],
        threshold: f64,
        check_count: usize,
        threshold_count: usize,
    ) -> bool {
        let below_count = self.count_below_threshold(klines, threshold, check_count);
        below_count >= threshold_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_klines(closes: &[f64]) -> Vec<Kline> {
        closes
            .iter()
            .map(|&close| Kline {
                open_time: 0,
                open: close,
                high: close,
                low: close,
                close,
                volume: 0.0,
                close_time: 0,
            })
            .collect()
    }

    #[test]
    fn test_bollinger_calculation() {
        let closes: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        let klines = create_test_klines(&closes);

        let calc = BollingerCalculator::new(20, 2.0);
        let result = calc.calculate(&klines).unwrap();

        // SMA of 1..=20 is 10.5
        assert!((result.middle - 10.5).abs() < 0.001);
    }

    #[test]
    fn test_insufficient_data() {
        let closes: Vec<f64> = (1..=10).map(|x| x as f64).collect();
        let klines = create_test_klines(&closes);

        let calc = BollingerCalculator::new(20, 2.0);
        let result = calc.calculate(&klines);

        assert!(result.is_err());
    }

    #[test]
    fn test_count_below_threshold() {
        let closes: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let klines = create_test_klines(&closes);

        let calc = BollingerCalculator::new(5, 2.0);

        // 最近5根中，有多少低于7.0？应该是 6,7,8,9,10 -> 6 < 7, 所以是1根
        let count = calc.count_below_threshold(&klines, 7.0, 5);
        assert_eq!(count, 1);

        // 最近10根中，有多少低于5.5？应该是 1,2,3,4,5 -> 5根
        let count = calc.count_below_threshold(&klines, 5.5, 10);
        assert_eq!(count, 5);
    }
}
