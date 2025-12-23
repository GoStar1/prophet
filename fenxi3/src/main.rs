mod analysis;
mod error;
mod loader;
mod models;
mod output;

use crate::analysis::FastScanner;
use crate::models::TradeResult;
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

fn print_statistics(trades: &[TradeResult]) {
    if trades.is_empty() {
        println!("\n没有完成的交易记录");
        return;
    }

    let total = trades.len();
    let wins: Vec<_> = trades.iter().filter(|t| t.profit_pct > 0.0).collect();
    let losses: Vec<_> = trades.iter().filter(|t| t.profit_pct <= 0.0).collect();

    let win_count = wins.len();
    let loss_count = losses.len();
    let win_rate = win_count as f64 / total as f64 * 100.0;

    let total_profit: f64 = trades.iter().map(|t| t.profit_pct).sum();
    let avg_profit = total_profit / total as f64;

    let max_profit = trades
        .iter()
        .map(|t| t.profit_pct)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_loss = trades
        .iter()
        .map(|t| t.profit_pct)
        .fold(f64::INFINITY, f64::min);

    let avg_hold_hours: f64 = trades.iter().map(|t| t.hold_hours).sum::<f64>() / total as f64;

    // 计算累计收益（假设每次投入相同本金）
    let cumulative_return: f64 = trades
        .iter()
        .fold(1.0, |acc, t| acc * (1.0 + t.profit_pct / 100.0));

    println!("\n========== 回测统计结果 ==========");
    println!("总交易次数: {}", total);
    println!("盈利次数: {} | 亏损次数: {}", win_count, loss_count);
    println!("胜率: {:.2}%", win_rate);
    println!("----------------------------------");
    println!("平均盈亏: {:.2}%", avg_profit);
    println!("累计收益: {:.2}% (复利计算)", (cumulative_return - 1.0) * 100.0);
    println!("----------------------------------");
    println!("最大单笔盈利: {:.2}%", max_profit);
    println!("最大单笔亏损: {:.2}%", max_loss);
    println!("平均持仓时间: {:.1} 小时", avg_hold_hours);
    println!("==================================\n");
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("fenxi=info".parse().unwrap()),
        )
        .init();

    let data_path = Path::new("data");
    let output_path = Path::new("output/trades.csv");

    info!("Starting backtest scanner...");
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

    // 并行扫描所有交易对，获取交易记录
    let all_trades: Vec<TradeResult> = symbols
        .par_iter()
        .filter_map(|symbol| {
            let result = scanner.scan_symbol_trades(data_path, symbol);
            counter.fetch_add(1, Ordering::Relaxed);
            pb.set_position(counter.load(Ordering::Relaxed) as u64);

            match result {
                Ok(trades) if !trades.is_empty() => Some(trades),
                _ => None,
            }
        })
        .flatten()
        .collect();

    pb.finish_with_message("Done!");

    // 写入交易记录
    let mut writer = CsvWriter::new(output_path)?;
    writer.write_trades(&all_trades)?;
    writer.flush()?;

    info!("Backtest complete! Total trades: {}", all_trades.len());
    info!("Results saved to: {:?}", output_path);

    // 打印统计结果
    print_statistics(&all_trades);

    Ok(())
}
