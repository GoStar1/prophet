use crate::error::Result;
use crate::models::Kline;
use std::collections::VecDeque;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub struct KlineLoader {
    symbol: String,
    interval: String,
    base_path: PathBuf,
    files: Vec<PathBuf>,
    current_file_idx: usize,
    current_reader: Option<csv::Reader<File>>,
    buffer: VecDeque<Kline>,
    window_size: usize,
}

impl KlineLoader {
    pub fn new(base_path: &Path, symbol: &str, interval: &str, window_size: usize) -> Result<Self> {
        let dir = base_path.join("klines").join(symbol).join(interval);
        let mut files = Self::scan_files(&dir)?;
        files.sort();

        Ok(Self {
            symbol: symbol.to_string(),
            interval: interval.to_string(),
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

    fn read_next_kline(&mut self) -> Result<Option<Kline>> {
        loop {
            if let Some(ref mut reader) = self.current_reader {
                let mut record = csv::StringRecord::new();
                if reader.read_record(&mut record)? {
                    let kline: Kline = record.deserialize(None)?;
                    if kline.is_valid() {
                        return Ok(Some(kline));
                    }
                    continue;
                }
            }
            if !self.open_next_file()? {
                return Ok(None);
            }
        }
    }

    pub fn fill_initial_buffer(&mut self) -> Result<()> {
        while self.buffer.len() < self.window_size {
            match self.read_next_kline()? {
                Some(kline) => self.buffer.push_back(kline),
                None => break,
            }
        }
        Ok(())
    }

    pub fn advance(&mut self) -> Result<Option<&Kline>> {
        if let Some(kline) = self.read_next_kline()? {
            self.buffer.push_back(kline);
            if self.buffer.len() > self.window_size {
                self.buffer.pop_front();
            }
        } else {
            return Ok(None);
        }
        Ok(self.buffer.back())
    }

    pub fn current(&self) -> Option<&Kline> {
        self.buffer.back()
    }

    pub fn window(&self) -> &VecDeque<Kline> {
        &self.buffer
    }

    pub fn as_slice(&self) -> Vec<&Kline> {
        self.buffer.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn has_sufficient_data(&self, required: usize) -> bool {
        self.buffer.len() >= required
    }

    pub fn sync_to(&mut self, target_time: i64) -> Result<()> {
        while let Some(kline) = self.buffer.back() {
            if kline.close_time <= target_time {
                break;
            }
            if let Some(next) = self.read_next_kline()? {
                self.buffer.push_back(next);
                if self.buffer.len() > self.window_size {
                    self.buffer.pop_front();
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    pub fn advance_until(&mut self, target_time: i64) -> Result<bool> {
        loop {
            if let Some(kline) = self.buffer.back() {
                if kline.close_time >= target_time {
                    return Ok(true);
                }
            }
            if let Some(kline) = self.read_next_kline()? {
                self.buffer.push_back(kline);
                if self.buffer.len() > self.window_size {
                    self.buffer.pop_front();
                }
            } else {
                return Ok(false);
            }
        }
    }

    pub fn get_klines_at_time(&self, target_time: i64) -> Vec<&Kline> {
        self.buffer
            .iter()
            .filter(|k| k.close_time <= target_time)
            .collect()
    }
}
