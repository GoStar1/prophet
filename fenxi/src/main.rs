mod analysis;
mod error;
mod loader;
mod models;
mod output;

use crate::analysis::FastScanner;
use crate::output::CsvWriter;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

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
    info!("Scanning {} symbols in parallel...", symbols.len());

    let pb = ProgressBar::new(symbols.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let counter = AtomicUsize::new(0);
    let scanner = FastScanner::new();

    // 并行扫描所有交易对
    let all_signals: Vec<_> = symbols
        .par_iter()
        .filter_map(|symbol| {
            let result = scanner.scan_symbol(data_path, symbol);
            counter.fetch_add(1, Ordering::Relaxed);
            pb.set_position(counter.load(Ordering::Relaxed) as u64);

            match result {
                Ok(signals) if !signals.is_empty() => Some(signals),
                _ => None,
            }
        })
        .flatten()
        .collect();

    pb.finish_with_message("Done!");

    // 写入结果
    let mut writer = CsvWriter::new(output_path)?;
    writer.write_signals(&all_signals)?;
    writer.flush()?;

    info!("Scan complete! Total signals: {}", all_signals.len());
    info!("Results saved to: {:?}", output_path);

    Ok(())
}
