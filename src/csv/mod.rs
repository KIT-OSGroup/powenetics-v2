use std::fmt::Debug;
use std::fs::File;
use std::path::Path;
use std::time;
use std::{fs, io};

use thiserror::Error;

use powenetics_v2::{Powenetics, PoweneticsData, PoweneticsSubscriber};

#[derive(Error, Debug)]
pub enum CsvError {
    #[error("I/O error")]
    Io(#[from] io::Error),
    #[error("CSV error")]
    Csv(#[from] csv::Error),
    #[error("CSV already exists and is not empty")]
    CsvExists,
}

struct CsvSubscriber {
    csv: csv::Writer<File>,
}

impl PoweneticsSubscriber for CsvSubscriber {
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

pub(crate) fn subscribe_csv(p: &mut Powenetics, path: &Path) -> Result<(), CsvError> {
    if path.try_exists()? && fs::metadata(path)?.len() != 0 {
        return Err(CsvError::CsvExists);
    }

    let mut sub = CsvSubscriber {
        csv: csv::Writer::from_path(path)?,
    };

    sub.csv.write_field("Timestamp")?;

    for ch in p.data().channels() {
        sub.csv.write_field(format!("{} Voltage (mV)", ch.name()))?;
        sub.csv.write_field(format!("{} Current (mA)", ch.name()))?;
        sub.csv.write_field(format!("{} Energy (nJ)", ch.name()))?;
    }

    sub.csv.write_record(None::<&[u8]>)?;

    p.subscribe(Box::new(sub));

    Ok(())
}
