use crate::error::Result;
use crate::models::BuySignal;
use std::fs::File;
use std::path::Path;

pub struct CsvWriter {
    writer: csv::Writer<File>,
}

impl CsvWriter {
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let writer = csv::Writer::from_writer(file);
        Ok(Self { writer })
    }

    pub fn write_signal(&mut self, signal: &BuySignal) -> Result<()> {
        self.writer.serialize(signal)?;
        Ok(())
    }

    pub fn write_signals(&mut self, signals: &[BuySignal]) -> Result<()> {
        for signal in signals {
            self.write_signal(signal)?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}
