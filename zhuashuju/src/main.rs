use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use clap::Parser;
use futures::{stream, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration as StdDuration;

/// Binance 合约历史数据下载器 (K线 + 持仓量)
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 输出目录
    #[arg(short, long, default_value = "data")]
    output: String,

    /// 交易量排名前N的币种
    #[arg(short, long, default_value_t = 250)]
    top: usize,

    /// 并发下载数
    #[arg(short, long, default_value_t = 50)]
    concurrent: usize,

    /// 开始日期 (YYYY-MM-DD)
    #[arg(long)]
    start_date: Option<String>,

    /// 结束日期 (YYYY-MM-DD)
    #[arg(long)]
    end_date: Option<String>,

    /// 只下载K线 (不下载持仓量)
    #[arg(long)]
    kline_only: bool,

    /// 只下载持仓量 (不下载K线)
    #[arg(long)]
    oi_only: bool,
}

/// Binance 24小时行情数据
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Ticker24h {
    symbol: String,
    #[serde(rename = "quoteVolume")]
    quote_volume: String,
}

/// 合约K线时间周期
const KLINE_INTERVALS: [&str; 3] = ["15m", "30m", "4h"];

/// 下载任务
#[derive(Clone)]
enum DownloadTask {
    /// K线任务 (按月下载)
    Kline {
        symbol: String,
        interval: String,
        year: i32,
        month: u32,
        output_dir: PathBuf,
    },
    /// 持仓量/Metrics 任务 (按日下载)
    Metrics {
        symbol: String,
        date: NaiveDate,
        output_dir: PathBuf,
    },
}

impl DownloadTask {
    fn url(&self) -> String {
        match self {
            DownloadTask::Kline { symbol, interval, year, month, .. } => {
                format!(
                    "https://data.binance.vision/data/futures/um/monthly/klines/{}/{}/{}-{}-{}-{:02}.zip",
                    symbol, interval, symbol, interval, year, month
                )
            }
            DownloadTask::Metrics { symbol, date, .. } => {
                format!(
                    "https://data.binance.vision/data/futures/um/daily/metrics/{}/{}-metrics-{}.zip",
                    symbol, symbol, date.format("%Y-%m-%d")
                )
            }
        }
    }

    fn output_path(&self) -> PathBuf {
        match self {
            DownloadTask::Kline { symbol, interval, year, month, output_dir } => {
                output_dir
                    .join("klines")
                    .join(symbol)
                    .join(interval)
                    .join(format!("{}-{}-{}-{:02}.csv", symbol, interval, year, month))
            }
            DownloadTask::Metrics { symbol, date, output_dir } => {
                output_dir
                    .join("metrics")
                    .join(symbol)
                    .join(format!("{}-metrics-{}.csv", symbol, date.format("%Y-%m-%d")))
            }
        }
    }
}

/// 创建优化的 HTTP 客户端
fn create_optimized_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(StdDuration::from_secs(10))
        .timeout(StdDuration::from_secs(30))
        .pool_max_idle_per_host(100)
        .tcp_nodelay(true)
        .tcp_keepalive(StdDuration::from_secs(60))
        .build()
        .context("创建 HTTP 客户端失败")
}

/// 创建 API 客户端
fn create_api_client() -> Result<Client> {
    Client::builder()
        .timeout(StdDuration::from_secs(30))
        .build()
        .context("创建 API 客户端失败")
}

/// 获取合约交易量前N的USDT永续合约
async fn get_futures_symbols(client: &Client, top_n: usize) -> Result<Vec<String>> {
    println!("📊 正在获取合约交易量前 {} 的 USDT 永续合约...", top_n);

    let url = "https://fapi.binance.com/fapi/v1/ticker/24hr";
    let tickers: Vec<Ticker24h> = client
        .get(url)
        .send()
        .await
        .context("请求 Binance Futures API 失败")?
        .json()
        .await
        .context("解析响应失败")?;

    let mut usdt_pairs: Vec<(String, f64)> = tickers
        .into_iter()
        .filter(|t| t.symbol.ends_with("USDT"))
        .filter_map(|t| {
            let volume: f64 = t.quote_volume.parse().ok()?;
            Some((t.symbol, volume))
        })
        .collect();

    usdt_pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let symbols: Vec<String> = usdt_pairs
        .into_iter()
        .take(top_n)
        .map(|(s, _)| s)
        .collect();

    println!("✅ 获取到 {} 个合约", symbols.len());

    println!("📈 交易量前10:");
    for (i, s) in symbols.iter().take(10).enumerate() {
        println!("   {}. {}", i + 1, s);
    }

    Ok(symbols)
}

/// 生成月份列表 (用于K线)
fn generate_months(start: Option<&String>, end: Option<&String>) -> Vec<(i32, u32)> {
    let today = Utc::now().naive_utc().date();

    // 默认从5年前开始
    let five_years_ago = NaiveDate::from_ymd_opt(today.year() - 5, today.month(), 1).unwrap();

    let start_date = start
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(five_years_ago);

    let end_date = end
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(today);

    let mut months = Vec::new();
    let mut current = start_date;

    while current <= end_date {
        if current.year() != today.year() || current.month() != today.month() {
            months.push((current.year(), current.month()));
        }
        current = if current.month() == 12 {
            NaiveDate::from_ymd_opt(current.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(current.year(), current.month() + 1, 1).unwrap()
        };
    }

    months
}

/// 生成日期列表 (用于持仓量)
fn generate_dates(start: Option<&String>, end: Option<&String>) -> Vec<NaiveDate> {
    let today = Utc::now().naive_utc().date();

    // 默认从5年前开始
    let five_years_ago = NaiveDate::from_ymd_opt(today.year() - 5, today.month(), 1).unwrap();

    let start_date = start
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(five_years_ago);

    let end_date = end
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(today);

    let mut dates = Vec::new();
    let mut current = start_date;

    // 跳过最近2天 (数据可能不完整)
    let cutoff = today - Duration::days(2);

    while current <= end_date && current <= cutoff {
        dates.push(current);
        current = current + Duration::days(1);
    }

    dates
}

/// 下载结果
#[derive(Clone, Copy)]
enum DownloadResult {
    Success,
    Skipped,
    NotFound,
    Failed,
}

/// 下载并解压单个文件
async fn download_and_extract(client: &Client, task: DownloadTask) -> DownloadResult {
    let output_path = task.output_path();

    if output_path.exists() {
        return DownloadResult::Skipped;
    }

    if let Some(parent) = output_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return DownloadResult::Failed;
        }
    }

    let response = match client.get(&task.url()).send().await {
        Ok(r) => r,
        Err(_) => return DownloadResult::Failed,
    };

    if !response.status().is_success() {
        return DownloadResult::NotFound;
    }

    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(_) => return DownloadResult::Failed,
    };

    let result = tokio::task::spawn_blocking(move || {
        let cursor = Cursor::new(bytes);
        let mut archive = match zip::ZipArchive::new(cursor) {
            Ok(a) => a,
            Err(_) => return DownloadResult::Failed,
        };

        for i in 0..archive.len() {
            let mut file = match archive.by_index(i) {
                Ok(f) => f,
                Err(_) => return DownloadResult::Failed,
            };

            let name = file.name().to_string();

            if name.ends_with(".csv") {
                let mut contents = Vec::new();
                if file.read_to_end(&mut contents).is_err() {
                    return DownloadResult::Failed;
                }

                let mut output_file = match File::create(&output_path) {
                    Ok(f) => f,
                    Err(_) => return DownloadResult::Failed,
                };

                if output_file.write_all(&contents).is_err() {
                    return DownloadResult::Failed;
                }
                break;
            }
        }

        DownloadResult::Success
    })
    .await;

    result.unwrap_or(DownloadResult::Failed)
}

/// 统计计数器
struct Stats {
    success: AtomicU64,
    skipped: AtomicU64,
    not_found: AtomicU64,
    failed: AtomicU64,
}

impl Stats {
    fn new() -> Self {
        Self {
            success: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            not_found: AtomicU64::new(0),
            failed: AtomicU64::new(0),
        }
    }

    fn record(&self, result: DownloadResult) {
        match result {
            DownloadResult::Success => self.success.fetch_add(1, Ordering::Relaxed),
            DownloadResult::Skipped => self.skipped.fetch_add(1, Ordering::Relaxed),
            DownloadResult::NotFound => self.not_found.fetch_add(1, Ordering::Relaxed),
            DownloadResult::Failed => self.failed.fetch_add(1, Ordering::Relaxed),
        };
    }

    fn get_counts(&self) -> (u64, u64, u64, u64) {
        (
            self.success.load(Ordering::Relaxed),
            self.skipped.load(Ordering::Relaxed),
            self.not_found.load(Ordering::Relaxed),
            self.failed.load(Ordering::Relaxed),
        )
    }
}

/// 执行下载任务
async fn run_downloads(
    client: Arc<Client>,
    tasks: Vec<DownloadTask>,
    concurrent: usize,
    label: &str,
) -> Result<(u64, u64, u64)> {
    let total = tasks.len();
    if total == 0 {
        return Ok((0, 0, 0));
    }

    println!("\n📥 {} - 下载 {} 个文件 (并发: {})", label, total, concurrent);

    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:50.cyan/blue}] {pos}/{len} ({percent}%) | {per_sec} | ETA: {eta}")?
            .progress_chars("━━░"),
    );
    pb.enable_steady_tick(StdDuration::from_millis(100));

    let stats = Arc::new(Stats::new());

    stream::iter(tasks)
        .map(|task| {
            let client = client.clone();
            let stats = stats.clone();
            let pb = pb.clone();

            async move {
                let result = download_and_extract(&client, task).await;
                stats.record(result);
                pb.inc(1);
            }
        })
        .buffer_unordered(concurrent)
        .collect::<Vec<_>>()
        .await;

    pb.finish();

    let (success, skipped, not_found, failed) = stats.get_counts();
    println!(
        "   ✅ 新下载: {} | ⏭️ 已存在: {} | 📭 不可用: {} | ❌ 失败: {}",
        success, skipped, not_found, failed
    );

    Ok((success, skipped + not_found, failed))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║          Binance 合约历史数据下载器 (高速版)                  ║");
    println!("║          K线: 15m, 30m, 4h (月) | 持仓量: 5min (日)           ║");
    println!("╚═══════════════════════════════════════════════════════════════╝\n");

    let download_client = Arc::new(create_optimized_client()?);
    let api_client = create_api_client()?;

    let output_dir = PathBuf::from(&args.output);

    let futures_symbols = get_futures_symbols(&api_client, args.top).await?;

    if futures_symbols.is_empty() {
        println!("⚠️  没有找到合约");
        return Ok(());
    }

    let mut total_success = 0u64;
    let mut total_skip = 0u64;
    let mut total_fail = 0u64;

    let start_time = std::time::Instant::now();

    // ========== 合约K线 (月度) ==========
    if !args.oi_only {
        let months = generate_months(args.start_date.as_ref(), args.end_date.as_ref());

        if months.is_empty() {
            println!("⚠️  没有可下载的K线月份");
        } else {
            if let (Some(first), Some(last)) = (months.first(), months.last()) {
                println!("📅 K线时间范围: {}-{:02} 到 {}-{:02}", first.0, first.1, last.0, last.1);
            }

            let mut kline_tasks = Vec::new();
            for symbol in &futures_symbols {
                for interval in KLINE_INTERVALS {
                    for (year, month) in &months {
                        kline_tasks.push(DownloadTask::Kline {
                            symbol: symbol.clone(),
                            interval: interval.to_string(),
                            year: *year,
                            month: *month,
                            output_dir: output_dir.clone(),
                        });
                    }
                }
            }

            println!(
                "\n📋 K线: {} 合约 × {} 周期 × {} 月 = {} 文件",
                futures_symbols.len(),
                KLINE_INTERVALS.len(),
                months.len(),
                kline_tasks.len()
            );

            let (s, sk, f) = run_downloads(
                download_client.clone(),
                kline_tasks,
                args.concurrent,
                "合约K线",
            )
            .await?;
            total_success += s;
            total_skip += sk;
            total_fail += f;
        }
    }

    // ========== 持仓量/Metrics (日度) ==========
    if !args.kline_only {
        let dates = generate_dates(args.start_date.as_ref(), args.end_date.as_ref());

        if dates.is_empty() {
            println!("⚠️  没有可下载的持仓量日期");
        } else {
            if let (Some(first), Some(last)) = (dates.first(), dates.last()) {
                println!("📅 持仓量时间范围: {} 到 {} ({} 天)", first, last, dates.len());
            }

            let mut metrics_tasks = Vec::new();
            for symbol in &futures_symbols {
                for date in &dates {
                    metrics_tasks.push(DownloadTask::Metrics {
                        symbol: symbol.clone(),
                        date: *date,
                        output_dir: output_dir.clone(),
                    });
                }
            }

            println!(
                "\n📋 持仓量: {} 合约 × {} 天 = {} 文件",
                futures_symbols.len(),
                dates.len(),
                metrics_tasks.len()
            );

            let (s, sk, f) =
                run_downloads(download_client.clone(), metrics_tasks, args.concurrent, "持仓量/Metrics").await?;
            total_success += s;
            total_skip += sk;
            total_fail += f;
        }
    }

    let elapsed = start_time.elapsed();

    println!("\n{}", "═".repeat(60));
    println!("📊 下载完成!");
    println!("   ⏱️  总耗时: {:.1}s", elapsed.as_secs_f64());
    println!("   ✅ 新下载: {}", total_success);
    println!("   ⏭️  跳过: {}", total_skip);
    println!("   ❌ 失败: {}", total_fail);

    if total_success > 0 {
        println!(
            "   🚀 平均速度: {:.1} 文件/秒",
            total_success as f64 / elapsed.as_secs_f64()
        );
    }

    println!("\n📁 数据目录: {}/", args.output);
    println!("   ├── klines/    # 合约K线 (15m, 30m, 4h)");
    println!("   └── metrics/   # 持仓量+多空比 (5min精度)");
    println!("\n💡 metrics 包含: 持仓量、多空比、大户持仓比等");

    Ok(())
}
