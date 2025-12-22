use crate::error::Result;
use crate::models::Metrics;
use std::collections::VecDeque;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub struct MetricsLoader {
    symbol: String,
    base_path: PathBuf,
    files: Vec<PathBuf>,
    current_file_idx: usize,
    current_reader: Option<csv::Reader<File>>,
    buffer: VecDeque<Metrics>,
    window_size: usize,
}

impl MetricsLoader {
    pub fn new(base_path: &Path, symbol: &str, window_size: usize) -> Result<Self> {
        let dir = base_path.join("metrics").join(symbol);
        let mut files = Self::scan_files(&dir)?;
        files.sort();

        Ok(Self {
            symbol: symbol.to_string(),
            base_path: base_path.to_path_buf(),
            files,
            current_file_idx: 0,
            current_reader: None,
            buffer: VecDeque::with_capacity(window_size + 100),
            window_size,
        })
    }

    fn scan_files(dir: &Path) -> Result<Vec<PathBuf>> {
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "csv").unwrap_or(false) {
                files.push(path);
            }
        }
        Ok(files)
    }

    fn open_next_file(&mut self) -> Result<bool> {
        if self.current_file_idx >= self.files.len() {
            return Ok(false);
        }
        let file = File::open(&self.files[self.current_file_idx])?;
        self.current_reader = Some(csv::Reader::from_reader(file));
        self.current_file_idx += 1;
        Ok(true)
    }

    fn read_next_metrics(&mut self) -> Result<Option<Metrics>> {
        loop {
            if let Some(ref mut reader) = self.current_reader {
                let mut record = csv::StringRecord::new();
                if reader.read_record(&mut record)? {
                    let metrics: Metrics = record.deserialize(None)?;
                    return Ok(Some(metrics));
                }
            }
            if !self.open_next_file()? {
                return Ok(None);
            }
        }
    }

    pub fn fill_initial_buffer(&mut self) -> Result<()> {
        while self.buffer.len() < self.window_size {
            match self.read_next_metrics()? {
                Some(m) => self.buffer.push_back(m),
                None => break,
            }
        }
        Ok(())
    }

    pub fn advance_until(&mut self, target_time: i64) -> Result<bool> {
        loop {
            if let Some(m) = self.buffer.back() {
                if m.timestamp_ms() >= target_time {
                    return Ok(true);
                }
            }
            if let Some(m) = self.read_next_metrics()? {
                self.buffer.push_back(m);
                if self.buffer.len() > self.window_size {
                    self.buffer.pop_front();
                }
            } else {
                return Ok(false);
            }
        }
    }

    pub fn get_current_oi(&self, target_time: i64) -> Option<f64> {
        self.buffer
            .iter()
            .rev()
            .find(|m| m.timestamp_ms() <= target_time)
            .map(|m| m.sum_open_interest)
    }

    pub fn get_min_oi_3days(&self, target_time: i64) -> Option<f64> {
        let three_days_ms = 3 * 24 * 60 * 60 * 1000_i64;
        let start_time = target_time - three_days_ms;

        self.buffer
            .iter()
            .filter(|m| {
                let t = m.timestamp_ms();
                t >= start_time && t <= target_time
            })
            .map(|m| m.sum_open_interest)
            .fold(None, |min, oi| match min {
                None => Some(oi),
                Some(m) if oi < m => Some(oi),
                _ => min,
            })
    }

    pub fn has_data(&self) -> bool {
        !self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}
