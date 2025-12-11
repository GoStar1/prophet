use crate::config::EmailConfig;
use crate::error::{AppError, Result};
use crate::models::AnalyzedCoin;

use chrono::Local;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub struct EmailNotifier {
    config: EmailConfig,
    mailer: AsyncSmtpTransport<Tokio1Executor>,
}

impl EmailNotifier {
    pub fn new(config: EmailConfig) -> Result<Self> {
        let creds = Credentials::new(config.username.clone(), config.password.clone());

        let tls_params = TlsParameters::builder(config.smtp_server.clone())
            .dangerous_accept_invalid_certs(true)
            .build()
            .map_err(|e| AppError::EmailError(format!("TLS params error: {}", e)))?;

        let mailer = if config.smtp_port == 465 || config.smtp_port == 994 {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_server)
                .port(config.smtp_port)
                .tls(Tls::Wrapper(tls_params))
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_server)
                .port(config.smtp_port)
                .tls(Tls::Required(tls_params))
                .credentials(creds)
                .build()
        };

        Ok(Self { config, mailer })
    }

    /// 发送心跳邮件，确认系统还在运行
    pub async fn send_heartbeat(&self) -> Result<()> {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let subject = format!(
            "Prophet Heartbeat - System Running - {}",
            Local::now().format("%Y-%m-%d %H:%M")
        );

        let body = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <style>
        body {{ font-family: Arial, sans-serif; padding: 20px; }}
        .status {{ color: green; font-size: 24px; }}
    </style>
</head>
<body>
    <h2>Prophet System Heartbeat</h2>
    <p class="status">✓ System is running normally</p>
    <p>This is an automatic heartbeat notification sent because no coins met all 7 conditions in the last 100 analysis cycles.</p>
    <p>Generated at: {}</p>
</body>
</html>"#,
            timestamp
        );

        let from_addr = format!("Prophet <{}>", self.config.from);
        let to_addr = format!("<{}>", self.config.to);

        let email = Message::builder()
            .from(
                from_addr
                    .parse()
                    .map_err(|e| AppError::EmailError(format!("Invalid from address: {}", e)))?,
            )
            .to(to_addr
                .parse()
                .map_err(|e| AppError::EmailError(format!("Invalid to address: {}", e)))?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(body)
            .map_err(|e| AppError::EmailError(e.to_string()))?;

        self.mailer
            .send(email)
            .await
            .map_err(|e| AppError::EmailError(format!("Send failed: {}", e)))?;

        Ok(())
    }

    /// 发送v2版本的报警邮件（多时间周期+持仓量）
    pub async fn send_alert_v2(&self, coins: &[&AnalyzedCoin]) -> Result<()> {
        if coins.is_empty() {
            return Ok(());
        }

        let body = self.build_email_body_v2(coins);
        let subject = format!(
            "Prophet v2: {} coins meet ALL 7 conditions - {}",
            coins.len(),
            Local::now().format("%Y-%m-%d %H:%M")
        );

        let from_addr = format!("Prophet <{}>", self.config.from);
        let to_addr = format!("<{}>", self.config.to);

        let email = Message::builder()
            .from(
                from_addr
                    .parse()
                    .map_err(|e| AppError::EmailError(format!("Invalid from address: {}", e)))?,
            )
            .to(to_addr
                .parse()
                .map_err(|e| AppError::EmailError(format!("Invalid to address: {}", e)))?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(body)
            .map_err(|e| AppError::EmailError(e.to_string()))?;

        self.mailer
            .send(email)
            .await
            .map_err(|e| AppError::EmailError(format!("Send failed: {}", e)))?;

        Ok(())
    }

    fn build_email_body_v2(&self, coins: &[&AnalyzedCoin]) -> String {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut rows = String::new();
        for coin in coins {
            let oi_ratio = if coin.min_oi_3d > 0.0 {
                coin.current_oi / coin.min_oi_3d
            } else {
                0.0
            };

            // 条件状态显示
            let cond_status = format!(
                "{}{}{}{}{}{}{}",
                if coin.cond1_price_above_15m_upper { "1" } else { "-" },
                if coin.cond2_price_above_30m_middle { "2" } else { "-" },
                if coin.cond3_price_above_4h_middle { "3" } else { "-" },
                if coin.cond4_15m_history_below_upper { "4" } else { "-" },
                if coin.cond5_30m_history_below_middle { "5" } else { "-" },
                if coin.cond6_4h_history_below_middle { "6" } else { "-" },
                if coin.cond7_oi_condition { "7" } else { "-" },
            );

            rows.push_str(&format!(
                r#"<tr>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td style="color: green;">${:.4}</td>
                    <td>${:.4}</td>
                    <td>${:.4}</td>
                    <td>${:.4}</td>
                    <td>{:.2}x</td>
                    <td style="color: green;">{}</td>
                </tr>"#,
                coin.coin.market_cap_rank.unwrap_or(0),
                coin.coin.name,
                coin.coin.symbol.to_uppercase(),
                coin.current_price,
                coin.boll.boll_15m_upper,
                coin.boll.boll_30m_middle,
                coin.boll.boll_4h_middle,
                oi_ratio,
                cond_status
            ));
        }

        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <style>
        body {{ font-family: Arial, sans-serif; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #4CAF50; color: white; }}
        tr:nth-child(even) {{ background-color: #f2f2f2; }}
        .conditions {{ font-size: 12px; }}
    </style>
</head>
<body>
    <h2>Prophet v2 - Multi-Timeframe BOLL + OI Filter</h2>
    <p>Generated at: {}</p>

    <h3>Filter Conditions:</h3>
    <ol class="conditions">
        <li>Price > 15m BOLL Upper</li>
        <li>Price > 30m BOLL Middle</li>
        <li>Price > 4h BOLL Middle</li>
        <li>15m: 50 candles, 25+ below upper</li>
        <li>30m: 50 candles, 25+ below middle</li>
        <li>4h: 50 candles, 25+ below middle</li>
        <li>Current OI * 0.9 > 3-day Min OI</li>
    </ol>

    <h3>Matching Coins ({} found):</h3>
    <table>
        <tr>
            <th>Rank</th>
            <th>Name</th>
            <th>Symbol</th>
            <th>Price</th>
            <th>15m Upper</th>
            <th>30m Mid</th>
            <th>4h Mid</th>
            <th>OI Ratio</th>
            <th>Conds</th>
        </tr>
        {}
    </table>

    <p><strong>Total: {} coins meeting ALL 7 conditions</strong></p>
</body>
</html>"#,
            timestamp,
            coins.len(),
            rows,
            coins.len()
        )
    }
}
