use crate::config::BinanceConfig;
use crate::error::{AppError, Result};
use crate::models::{Kline, OpenInterest, OpenInterestHist};

use super::RateLimiter;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Debug, Deserialize)]
struct ExchangeInfo {
    symbols: Vec<SymbolInfo>,
}

#[derive(Debug, Deserialize)]
struct SymbolInfo {
    symbol: String,
    #[serde(rename = "contractType")]
    contract_type: String,
    status: String,
}

pub struct BinanceClient {
    client: reqwest::Client,
    config: BinanceConfig,
    rate_limiter: Arc<RateLimiter>,
    semaphore: Arc<Semaphore>,
    perpetual_symbols: Arc<tokio::sync::RwLock<HashSet<String>>>,
}

impl BinanceClient {
    pub fn new(config: BinanceConfig) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            config,
            rate_limiter: Arc::new(RateLimiter::new(300)),
            semaphore: Arc::new(Semaphore::new(5)),
            perpetual_symbols: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
        }
    }

    /// 获取所有永续合约交易对
    pub async fn fetch_perpetual_symbols(&self) -> Result<HashSet<String>> {
        let url = format!("{}/fapi/v1/exchangeInfo", self.config.futures_base_url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::BinanceApi(format!("exchangeInfo failed: {text}")));
        }

        let info: ExchangeInfo = response.json().await?;
        let symbols: HashSet<String> = info
            .symbols
            .into_iter()
            .filter(|s| s.contract_type == "PERPETUAL" && s.status == "TRADING")
            .map(|s| s.symbol)
            .collect();

        // Cache the symbols
        let mut cache = self.perpetual_symbols.write().await;
        *cache = symbols.clone();

        Ok(symbols)
    }

    /// 检查是否有永续合约
    pub async fn has_perpetual(&self, symbol: &str) -> bool {
        let cache = self.perpetual_symbols.read().await;
        if cache.is_empty() {
            drop(cache);
            if let Ok(symbols) = self.fetch_perpetual_symbols().await {
                return symbols.contains(symbol);
            }
            return false;
        }
        cache.contains(symbol)
    }

    /// 获取期货K线数据
    pub async fn get_futures_klines(&self, symbol: &str, interval: &str) -> Result<Vec<Kline>> {
        let url = format!(
            "{}/fapi/v1/klines?symbol={}&interval={}&limit={}",
            self.config.futures_base_url, symbol, interval, self.config.kline_limit
        );

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

            if status.as_u16() == 400 {
                return Err(AppError::SymbolNotFound(symbol.to_string()));
            }

            return Err(AppError::BinanceApi(format!("Status {status}: {text}")));
        }

        let data: Vec<Vec<serde_json::Value>> = response.json().await?;
        let mut klines = Vec::with_capacity(data.len());

        for item in &data {
            klines.push(Kline::from_binance_response(item)?);
        }

        Ok(klines)
    }

    /// 获取当前持仓量
    pub async fn get_open_interest(&self, symbol: &str) -> Result<OpenInterest> {
        let url = format!(
            "{}/fapi/v1/openInterest?symbol={}",
            self.config.futures_base_url, symbol
        );

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::BinanceApi(format!("openInterest failed: {text}")));
        }

        let oi: OpenInterest = response.json().await?;
        Ok(oi)
    }

    /// 获取历史持仓量 (3天)
    pub async fn get_open_interest_hist(&self, symbol: &str) -> Result<Vec<OpenInterestHist>> {
        // 使用5分钟间隔获取3天数据
        // 3天 = 3 * 24 * 60 / 5 = 864 条数据
        let url = format!(
            "{}/futures/data/openInterestHist?symbol={}&period=5m&limit=500",
            self.config.futures_base_url, symbol
        );

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::BinanceApi(format!(
                "openInterestHist failed: {text}"
            )));
        }

        let hist: Vec<OpenInterestHist> = response.json().await?;
        Ok(hist)
    }

    /// 获取多时间周期K线数据
    pub async fn get_multi_timeframe_klines(
        &self,
        symbol: &str,
    ) -> Result<(Vec<Kline>, Vec<Kline>, Vec<Kline>)> {
        let _permit = self.semaphore.acquire().await.unwrap();

        // 依次获取三个时间周期的K线
        self.rate_limiter.acquire().await;
        let klines_15m = self.get_futures_klines(symbol, "15m").await?;

        self.rate_limiter.acquire().await;
        let klines_30m = self.get_futures_klines(symbol, "30m").await?;

        self.rate_limiter.acquire().await;
        let klines_4h = self.get_futures_klines(symbol, "4h").await?;

        Ok((klines_15m, klines_30m, klines_4h))
    }

    /// 获取完整分析数据（K线 + 持仓量）
    pub async fn get_analysis_data(
        &self,
        symbol: &str,
    ) -> Result<(Vec<Kline>, Vec<Kline>, Vec<Kline>, f64, f64)> {
        // 获取K线数据
        let (klines_15m, klines_30m, klines_4h) = self.get_multi_timeframe_klines(symbol).await?;

        // 获取持仓量数据
        self.rate_limiter.acquire().await;
        let current_oi = self.get_open_interest(symbol).await?;

        self.rate_limiter.acquire().await;
        let hist_oi = self.get_open_interest_hist(symbol).await?;

        // 计算3天最低持仓量
        let min_oi = hist_oi
            .iter()
            .map(|o| o.sum_open_interest_f64())
            .fold(f64::MAX, f64::min);

        Ok((
            klines_15m,
            klines_30m,
            klines_4h,
            current_oi.open_interest_f64(),
            min_oi,
        ))
    }
}
