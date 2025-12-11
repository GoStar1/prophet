use config::{Config, ConfigError, File};
use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub coingecko: CoinGeckoConfig,
    pub binance: BinanceConfig,
    pub analysis: AnalysisConfig,
    #[serde(skip)]
    pub email: EmailConfig,
    pub scheduler: SchedulerConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CoinGeckoConfig {
    pub base_url: String,
    pub top_n: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BinanceConfig {
    pub spot_base_url: String,
    pub futures_base_url: String,
    pub kline_limit: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AnalysisConfig {
    pub boll_period: usize,
    pub boll_std_dev: f64,
    pub history_check_count: usize, // 50
    pub history_threshold: usize,   // 25
    pub oi_multiplier: f64,         // 持仓量乘数，如 0.9
}

#[derive(Debug, Clone, Default)]
pub struct EmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SchedulerConfig {
    pub interval_minutes: u64,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let config = Config::builder()
            .add_source(File::with_name("config/default").required(false))
            .build()?;

        let mut settings: Settings = config.try_deserialize()?;

        // Load email config from environment variables
        settings.email = EmailConfig {
            smtp_server: env::var("EMAIL_SMTP_SERVER")
                .unwrap_or_else(|_| "smtp.163.com".to_string()),
            smtp_port: env::var("EMAIL_SMTP_PORT")
                .unwrap_or_else(|_| "994".to_string())
                .parse()
                .unwrap_or(994),
            username: env::var("EMAIL_USERNAME")
                .map_err(|_| ConfigError::NotFound("EMAIL_USERNAME".into()))?,
            password: env::var("EMAIL_PASSWORD")
                .map_err(|_| ConfigError::NotFound("EMAIL_PASSWORD".into()))?,
            from: env::var("EMAIL_FROM").map_err(|_| ConfigError::NotFound("EMAIL_FROM".into()))?,
            to: env::var("EMAIL_TO").map_err(|_| ConfigError::NotFound("EMAIL_TO".into()))?,
        };

        Ok(settings)
    }
}
