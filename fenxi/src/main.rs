mod analysis;
mod error;
mod loader;
mod models;
mod output;

use crate::analysis::Scanner;
use crate::loader::{KlineLoader, MetricsLoader};
use crate::output::CsvWriter;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tracing::{error, info, warn};

const KLINE_WINDOW_SIZE: usize = 450;
const METRICS_WINDOW_SIZE: usize = 864;

fn get_symbols(data_path: &Path) -> Vec<String> {
    let klines_path = data_path.join("klines");
    let metrics_path = data_path.join("metrics");

    let kline_symbols: HashSet<String> = fs::read_dir(&klines_path)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let metrics_symbols: HashSet<String> = fs::read_dir(&metrics_path)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut symbols: Vec<String> = kline_symbols
        .iter()
        .filter(|s| {
            let has_15m = klines_path.join(s).join("15m").exists();
            let has_30m = klines_path.join(s).join("30m").exists();
            let has_4h = klines_path.join(s).join("4h").exists();
            has_15m && has_30m && has_4h
        })
        .cloned()
        .collect();

    symbols.sort();

    info!(
        "Found {} symbols with K-line data, {} with metrics data",
        kline_symbols.len(),
        metrics_symbols.len()
    );

    symbols
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("fenxi=info".parse().unwrap()),
        )
        .init();

    let data_path = Path::new("data");
    let output_path = Path::new("output/signals.csv");

    info!("Starting buy signal scanner...");
    info!("Data path: {:?}", data_path);
    info!("Output path: {:?}", output_path);

    let symbols = get_symbols(data_path);
    info!("Scanning {} symbols...", symbols.len());

    let mut writer = CsvWriter::new(output_path)?;
    let scanner = Scanner::new();

    let pb = ProgressBar::new(symbols.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut total_signals = 0;

    for symbol in &symbols {
        pb.set_message(symbol.clone());

        let result = (|| -> anyhow::Result<Vec<models::BuySignal>> {
            let mut kline_15m = KlineLoader::new(data_path, symbol, "15m", KLINE_WINDOW_SIZE)?;
            let mut kline_30m = KlineLoader::new(data_path, symbol, "30m", KLINE_WINDOW_SIZE)?;
            let mut kline_4h = KlineLoader::new(data_path, symbol, "4h", KLINE_WINDOW_SIZE)?;
            let mut metrics = MetricsLoader::new(data_path, symbol, METRICS_WINDOW_SIZE)?;

            let signals =
                scanner.scan_symbol(symbol, &mut kline_15m, &mut kline_30m, &mut kline_4h, &mut metrics)?;
            Ok(signals)
        })();

        match result {
            Ok(signals) => {
                if !signals.is_empty() {
                    writer.write_signals(&signals)?;
                    total_signals += signals.len();
                    info!("{}: Found {} signals", symbol, signals.len());
                }
            }
            Err(e) => {
                warn!("{}: Error - {}", symbol, e);
            }
        }

        pb.inc(1);
    }

    writer.flush()?;
    pb.finish_with_message("Done!");

    info!("Scan complete! Total signals: {}", total_signals);
    info!("Results saved to: {:?}", output_path);

    Ok(())
}
