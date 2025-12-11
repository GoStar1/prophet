use crate::config::CoinGeckoConfig;
use crate::error::{AppError, Result};
use crate::models::CoinInfo;

use super::RateLimiter;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};

pub struct CoinGeckoClient {
    client: reqwest::Client,
    config: CoinGeckoConfig,
    rate_limiter: RateLimiter,
}

impl CoinGeckoClient {
    pub fn new(config: CoinGeckoConfig) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            config,
            rate_limiter: RateLimiter::new(30), // CoinGecko free tier: 30 req/min
        }
    }

    pub async fn get_top_coins(&self, n: usize) -> Result<Vec<CoinInfo>> {
        let mut all_coins = Vec::new();
        let per_page = 250;
        let pages = (n + per_page - 1) / per_page;

        for page in 1..=pages {
            self.rate_limiter.acquire().await;

            let url = format!(
                "{}/coins/markets?vs_currency=usd&order=market_cap_desc&per_page={}&page={}",
                self.config.base_url, per_page, page
            );

            let response = self.client.get(&url).send().await?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                return Err(AppError::CoinGeckoApi(format!(
                    "Status {}: {}",
                    status, text
                )));
            }

            let coins: Vec<CoinInfo> = response.json().await?;
            all_coins.extend(coins);

            if all_coins.len() >= n {
                break;
            }
        }

        all_coins.truncate(n);
        Ok(all_coins)
    }

    pub fn to_binance_symbol(coin: &CoinInfo) -> String {
        format!("{}USDT", coin.symbol.to_uppercase())
    }
}
