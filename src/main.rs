use prophet::analysis::BollingerCalculator;
use prophet::api::{BinanceClient, CoinGeckoClient};
use prophet::config::Settings;
use prophet::models::{AnalyzedCoin, CoinInfo, MultiTimeframeBoll};
use prophet::notification::EmailNotifier;

use std::time::Duration;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("prophet=info".parse().unwrap()),
        )
        .init();

    info!("Prophet v2 starting - Multi-timeframe BOLL + Open Interest Filter");

    // Load configuration
    let settings = Settings::load()?;
    info!("Configuration loaded successfully");

    // Initialize clients
    let coingecko = CoinGeckoClient::new(settings.coingecko.clone());
    let binance = BinanceClient::new(settings.binance.clone());
    let calculator = BollingerCalculator::new(
        settings.analysis.boll_period,
        settings.analysis.boll_std_dev,
    );
    let notifier = EmailNotifier::new(settings.email.clone())?;

    let interval = Duration::from_secs(settings.scheduler.interval_minutes * 60);

    // Fetch perpetual symbols once at startup
    info!("Fetching perpetual contract symbols...");
    match binance.fetch_perpetual_symbols().await {
        Ok(symbols) => info!("Found {} perpetual contracts", symbols.len()),
        Err(e) => {
            error!("Failed to fetch perpetual symbols: {}", e);
            return Err(e.into());
        }
    }

    // Counter for heartbeat email - if no email sent for 100 cycles, send a heartbeat
    let mut no_email_counter: u32 = 0;
    const HEARTBEAT_THRESHOLD: u32 = 100;

    loop {
        info!("Starting analysis cycle...");

        match run_analysis(&coingecko, &binance, &calculator, &notifier, &settings).await {
            Ok(count) => {
                info!(
                    "Analysis completed. Found {} coins meeting all 7 conditions",
                    count
                );

                if count > 0 {
                    // Email was sent, reset counter
                    no_email_counter = 0;
                } else {
                    // No email sent this cycle
                    no_email_counter += 1;
                    info!("No email counter: {}/{}", no_email_counter, HEARTBEAT_THRESHOLD);

                    if no_email_counter >= HEARTBEAT_THRESHOLD {
                        info!("Sending heartbeat email to confirm system is running...");
                        match notifier.send_heartbeat().await {
                            Ok(_) => {
                                info!("Heartbeat email sent successfully!");
                                no_email_counter = 0;
                            }
                            Err(e) => {
                                error!("Failed to send heartbeat email: {}", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Analysis failed: {}", e);
            }
        }

        info!(
            "Sleeping for {} minutes...",
            settings.scheduler.interval_minutes
        );
        tokio::time::sleep(interval).await;
    }
}

async fn run_analysis(
    coingecko: &CoinGeckoClient,
    binance: &BinanceClient,
    calculator: &BollingerCalculator,
    notifier: &EmailNotifier,
    settings: &Settings,
) -> anyhow::Result<usize> {
    // Step 1: Get top N coins from CoinGecko
    info!(
        "Fetching top {} coins from CoinGecko...",
        settings.coingecko.top_n
    );
    let coins = coingecko.get_top_coins(settings.coingecko.top_n).await?;
    info!("Retrieved {} coins", coins.len());

    // Step 2: Filter coins with perpetual contracts and analyze
    let mut analyzed_coins = Vec::new();
    let mut processed = 0;
    let mut skipped_no_perp = 0;
    let mut skipped_error = 0;

    for coin in &coins {
        let futures_symbol = format!("{}USDT", coin.symbol.to_uppercase());

        // Check if has perpetual contract
        if !binance.has_perpetual(&futures_symbol).await {
            skipped_no_perp += 1;
            continue;
        }

        // Analyze the coin
        match analyze_coin(coin, &futures_symbol, binance, calculator, settings).await {
            Ok(analyzed) => {
                if analyzed.meets_all_conditions() {
                    info!(
                        "MATCH: {} ({}) meets all 7 conditions!",
                        coin.name, futures_symbol
                    );
                }
                analyzed_coins.push(analyzed);
                processed += 1;
            }
            Err(e) => {
                warn!("Failed to analyze {}: {}", futures_symbol, e);
                skipped_error += 1;
            }
        }

        // Progress log every 20 coins
        if (processed + skipped_error) % 20 == 0 {
            info!(
                "Progress: {} analyzed, {} skipped (no perp: {}, error: {})",
                processed, skipped_no_perp + skipped_error, skipped_no_perp, skipped_error
            );
        }
    }

    info!(
        "Analysis complete: {} analyzed, {} skipped (no perp: {}, error: {})",
        processed,
        skipped_no_perp + skipped_error,
        skipped_no_perp,
        skipped_error
    );

    // Step 3: Filter coins meeting all conditions
    let matching: Vec<&AnalyzedCoin> = analyzed_coins
        .iter()
        .filter(|c| c.meets_all_conditions())
        .collect();

    info!(
        "Found {} coins meeting all 7 conditions (out of {} analyzed)",
        matching.len(),
        analyzed_coins.len()
    );

    // Print results to console
    if !matching.is_empty() {
        println!("\n========== COINS MEETING ALL 7 CONDITIONS ==========");
        println!(
            "{:<6} {:<15} {:<10} {:>12} {:>12} {:>12}",
            "Rank", "Name", "Symbol", "Price", "15m Upper", "OI Ratio"
        );
        println!("{}", "-".repeat(75));
        for coin in &matching {
            let oi_ratio = if coin.min_oi_3d > 0.0 {
                coin.current_oi / coin.min_oi_3d
            } else {
                0.0
            };
            println!(
                "{:<6} {:<15} {:<10} {:>12.4} {:>12.4} {:>12.2}",
                coin.coin.market_cap_rank.unwrap_or(0),
                &coin.coin.name[..coin.coin.name.len().min(14)],
                coin.coin.symbol.to_uppercase(),
                coin.current_price,
                coin.boll.boll_15m_upper,
                oi_ratio
            );
        }
        println!("=====================================================\n");

        // Send email notification
        info!("Sending email notification...");
        match notifier.send_alert_v2(&matching).await {
            Ok(_) => info!("Email sent successfully!"),
            Err(e) => error!("Email failed: {} (results printed above)", e),
        }
    } else {
        info!("No coins meeting all conditions, skipping notification");
    }

    Ok(matching.len())
}

async fn analyze_coin(
    coin: &CoinInfo,
    futures_symbol: &str,
    binance: &BinanceClient,
    calculator: &BollingerCalculator,
    settings: &Settings,
) -> anyhow::Result<AnalyzedCoin> {
    // Get all data
    let (klines_15m, klines_30m, klines_4h, current_oi, min_oi) =
        binance.get_analysis_data(futures_symbol).await?;

    // Calculate BOLL for each timeframe
    let boll_15m = calculator.calculate(&klines_15m)?;
    let boll_30m = calculator.calculate(&klines_30m)?;
    let boll_4h = calculator.calculate(&klines_4h)?;

    // Get current price from latest kline
    let current_price = klines_15m
        .last()
        .map(|k| k.close)
        .unwrap_or(coin.current_price);

    // Check all 7 conditions
    let cond1 = current_price > boll_15m.upper;
    let cond2 = current_price > boll_30m.middle;
    let cond3 = current_price > boll_4h.middle;

    let check_count = settings.analysis.history_check_count;
    let threshold = settings.analysis.history_threshold;

    let cond4 = calculator.check_history_condition(&klines_15m, boll_15m.upper, check_count, threshold);
    let cond5 = calculator.check_history_condition(&klines_30m, boll_30m.middle, check_count, threshold);
    let cond6 = calculator.check_history_condition(&klines_4h, boll_4h.middle, check_count, threshold);

    // OI condition: current_oi * oi_multiplier > min_oi_3d
    let cond7 = current_oi * settings.analysis.oi_multiplier > min_oi;

    Ok(AnalyzedCoin {
        coin: CoinInfo {
            futures_symbol: Some(futures_symbol.to_string()),
            ..coin.clone()
        },
        current_price,
        boll: MultiTimeframeBoll {
            boll_15m_upper: boll_15m.upper,
            boll_15m_middle: boll_15m.middle,
            boll_30m_upper: boll_30m.upper,
            boll_30m_middle: boll_30m.middle,
            boll_4h_upper: boll_4h.upper,
            boll_4h_middle: boll_4h.middle,
        },
        cond1_price_above_15m_upper: cond1,
        cond2_price_above_30m_middle: cond2,
        cond3_price_above_4h_middle: cond3,
        cond4_15m_history_below_upper: cond4,
        cond5_30m_history_below_middle: cond5,
        cond6_4h_history_below_middle: cond6,
        cond7_oi_condition: cond7,
        current_oi,
        min_oi_3d: min_oi,
    })
}
