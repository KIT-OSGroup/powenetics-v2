use std::fmt::Debug;
use std::fs::File;
use std::path::Path;
use std::time;
use std::{fs, io};

use thiserror::Error;

use powenetics_v2::{PoweneticsData, PoweneticsSubscriber, POWENETICS_CHANNELS};

mod server;
pub use server::ServerSubscriber;

#[derive(Error, Debug)]
pub enum CsvError {
    #[error("I/O error")]
    Io(#[from] io::Error),
    #[error("CSV error")]
    Csv(#[from] csv::Error),
    #[error("CSV already exists and is not empty")]
    CsvExists,
}

pub struct CsvSubscriber<W: io::Write> {
    csv: csv::Writer<W>,
}

impl CsvSubscriber<File> {
    pub fn from_path(path: &Path) -> Result<CsvSubscriber<File>, CsvError> {
        if path.try_exists()? && fs::metadata(path)?.len() != 0 {
            return Err(CsvError::CsvExists);
        }

        let mut sub = CsvSubscriber {
            csv: csv::Writer::from_path(path)?,
        };
        sub.write_header()?;
        Ok(sub)
    }
}

impl<W: io::Write> CsvSubscriber<W> {
    fn write_header(&mut self) -> Result<(), CsvError> {
        self.csv.write_field("Timestamp")?;

        for ch in POWENETICS_CHANNELS {
            self.csv.write_field(format!("{} Voltage (mV)", ch))?;
            self.csv.write_field(format!("{} Current (mA)", ch))?;
            self.csv.write_field(format!("{} Energy (nJ)", ch))?;
        }

        self.csv.write_record(None::<&[u8]>)?;
        Ok(())
    }
}

impl<W: io::Write> PoweneticsSubscriber for CsvSubscriber<W> {
    fn update(&mut self, p: &PoweneticsData) -> anyhow::Result<bool> {
        self.csv.write_field(format!(
            "{:.5}",
            p.last_update()
                .duration_since(time::SystemTime::UNIX_EPOCH)?
                .as_secs_f64()
        ))?;

        for ch in p.channels() {
            self.csv.write_field(format!("{}", ch.voltage()))?;
            self.csv.write_field(format!("{}", ch.current()))?;
            self.csv.write_field(format!("{}", ch.energy()))?;
        }

        self.csv.write_record(None::<&[u8]>)?;

        Ok(false)
    }
}
