use core::array;
use std::array::TryFromSliceError;
use std::fmt::Debug;
use std::io::Read;
use std::{io, thread, time};

use serialport::SerialPort;
use thiserror::Error;

use crate::PoweneticsError::Protocol;

const POWENETICS_BAUD_RATE: u32 = 921600;
const POWENETICS_DATA_BITS: serialport::DataBits = serialport::DataBits::Eight;
const POWENETICS_SERIAL_PARITY: serialport::Parity = serialport::Parity::None;
const POWENETICS_STOP_BITS: serialport::StopBits = serialport::StopBits::One;
const POWENETICS_MEASUREMENT_PACKET_SIZE: usize = 69;
const POWENETICS_READY_MESSAGE: &str = "PMD is ready!";

pub const POWENETICS_USB_VID: u16 = 0x4d8;
pub const POWENETICS_USB_PID: u16 = 0xa;

pub const POWENETICS_CHANNELS: [&str; 13] = [
    "ATX 3.3V",
    "ATX 5V Standby",
    "ATX 12V",
    "ATX 5V",
    "EPS 12V #1",
    "ATX12VO 12V Standby",
    "EPS 12V #3",
    "EPS 12V #2",
    "PCIe 12V #3",
    "PCIe 12V #2",
    "PCIe Slot 3.3V",
    "PCIe Slot 12V",
    "PCIe 12V #1",
];

pub trait PoweneticsSubscriber {
    fn update(&mut self, p: &PoweneticsData) -> anyhow::Result<bool>;
}

#[derive(Error, Debug)]
pub enum PoweneticsError {
    #[error("Serial port error")]
    SerialPort(#[from] serialport::Error),
    #[error("I/O error")]
    Io(#[from] io::Error),
    #[error("System time error")]
    SystemTime(#[from] time::SystemTimeError),
    #[error("{message}")]
    TryFromSlice {
        #[source]
        err: TryFromSliceError,
        message: &'static str,
    },
    #[error(transparent)]
    Subscriber(anyhow::Error),

    #[error("Unable to change measurement configuration after measurement has already started")]
    MeasurementAlreadyStarted,
    #[error("Invalid channel requested")]
    InvalidChannel,
    #[error("No power on channel, cannot calibrate")]
    NoPowerOnChannel,
    #[error("No listeners specified")]
    NoSubscribers,
    #[error("Powenetics protocol error, unplug and reconnect device. Reason: {message}")]
    Protocol { message: String },
}

pub struct Channel {
    name: String,
    id: u8,
    voltage: u16,
    current: u32,
    energy: u64,
    last_update: time::SystemTime,
}

impl Channel {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn voltage(&self) -> u16 {
        self.voltage
    }

    pub fn current(&self) -> u32 {
        self.current
    }

    pub fn power(&self) -> u32 {
        self.voltage as u32 * self.current
    }

    pub fn energy(&self) -> u64 {
        self.energy
    }

    fn update_energy(&mut self, time: time::SystemTime) -> Result<(), PoweneticsError> {
        if self.last_update != time::SystemTime::UNIX_EPOCH {
            let duration = time.duration_since(self.last_update)?;

            self.energy += (self.power() as u64) * (duration.as_millis() as u64);
        }

        self.last_update = time;
        Ok(())
    }

    pub fn reset_energy(&mut self) {
        self.energy = 0;
    }
}

pub struct Powenetics {
    subscriptions: Vec<Box<dyn PoweneticsSubscriber>>,
    data: PoweneticsData,
    port: Box<dyn SerialPort>,
    started: bool,
}

pub struct PoweneticsData {
    channels: [Channel; POWENETICS_CHANNELS.len()],
    last_update: time::SystemTime,
}

pub fn new(path: &str) -> Result<Powenetics, PoweneticsError> {
    let port = serialport::new(path, POWENETICS_BAUD_RATE)
        .parity(POWENETICS_SERIAL_PARITY)
        .data_bits(POWENETICS_DATA_BITS)
        .stop_bits(POWENETICS_STOP_BITS)
        .timeout(time::Duration::from_millis(5))
        .open()?;

    let channels = array::from_fn(|i| Channel {
        name: String::from(POWENETICS_CHANNELS[i]),
        id: i as u8,
        voltage: 0,
        current: 0,
        energy: 0,
        last_update: time::SystemTime::UNIX_EPOCH,
    });

    let powenetics = Powenetics {
        port,
        started: false,
        data: PoweneticsData {
            channels,
            last_update: time::SystemTime::UNIX_EPOCH,
        },
        subscriptions: vec![],
    };
    Ok(powenetics)
}

impl PoweneticsData {
    pub fn channel_by_id(&self, id: usize) -> Result<&Channel, PoweneticsError> {
        if id > self.channels.len() {
            return Err(PoweneticsError::InvalidChannel);
        }

        Ok(&self.channels[id])
    }

    pub fn channel_by_name(&self, name: &str) -> Result<&Channel, PoweneticsError> {
        for (i, n) in POWENETICS_CHANNELS.iter().enumerate() {
            if *n == name {
                return Ok(&self.channels[i]);
            }
        }

        Err(PoweneticsError::InvalidChannel)
    }

    pub fn channels(&self) -> &[Channel; POWENETICS_CHANNELS.len()] {
        &self.channels
    }

    pub fn last_update(&self) -> time::SystemTime {
        self.last_update
    }
}

impl Powenetics {
    pub fn calibrate(&mut self, channel: &Channel, reference: u32) -> Result<(), PoweneticsError> {
        if self.started {
            return Err(PoweneticsError::MeasurementAlreadyStarted);
        }

        self.port.write_all(&[0xCA])?;
        self.port.write_all(&[channel.id])?;
        self.port.write_all(&reference.to_be_bytes()[1..])?;
        self.port.flush()?;

        thread::sleep(time::Duration::from_millis(1));

        let bytes_to_read = self.port.bytes_to_read()?;
        if bytes_to_read != 0 {
            if bytes_to_read == 2 {
                let mut buf = [0; 2];

                self.port.read_exact(&mut buf)?;

                if buf == [0xCA, 0xAC] {
                    return Err(PoweneticsError::NoPowerOnChannel);
                } else {
                    return Err(Protocol {
                        message: format!(
                            "expected [0xCA, 0xAC], received [{:#04X}, {:#04X}]",
                            buf[0], buf[1]
                        ),
                    });
                }
            } else {
                return Err(Protocol {
                    message: format!("expected 2 bytes, received {}", bytes_to_read),
                });
            }
        }

        Ok(())
    }

    pub fn reset_calibration(&mut self) -> Result<(), PoweneticsError> {
        if self.started {
            return Err(PoweneticsError::MeasurementAlreadyStarted);
        }

        self.port.write_all(&[0xCA, 0xAC, 0xBD, 0x00])?;
        self.port.flush()?;

        Ok(())
    }

    fn finalize_calibration(&mut self) -> Result<(), PoweneticsError> {
        if self.started {
            return Err(PoweneticsError::MeasurementAlreadyStarted);
        }

        self.port.write_all(&[0xCA, 0xAC, 0xBD, 0x01])?;
        self.port.flush()?;

        thread::sleep(time::Duration::from_millis(1));

        Ok(())
    }

    pub fn start_measurement(&mut self) -> Result<(), PoweneticsError> {
        if self.started {
            return Err(PoweneticsError::MeasurementAlreadyStarted);
        }

        self.finalize_calibration()?;

        let bytes_to_read = self.port.bytes_to_read()?;
        if bytes_to_read != 0 {
            let mut buf = vec![0; bytes_to_read as usize];

            self.port.read_exact(&mut buf)?;

            if !String::from_utf8_lossy(&buf).starts_with(POWENETICS_READY_MESSAGE) {
                return Err(Protocol {
                    message: format!(
                        "expected \"{}\", received {:?}",
                        POWENETICS_READY_MESSAGE, buf
                    ),
                });
            }
        }

        self.port.write_all(&[0xCA, 0xAC, 0xBD, 0x90])?;
        self.port.flush()?;

        self.started = true;
        self.wait()?;

        Ok(())
    }

    fn wait(&mut self) -> Result<(), PoweneticsError> {
        if self.subscriptions.is_empty() {
            return Err(PoweneticsError::NoSubscribers);
        }

        let mut sequence = 1;

        loop {
            let mut buf = [0; POWENETICS_MEASUREMENT_PACKET_SIZE];

            self.port.read_exact(&mut buf)?;
            self.data.last_update = time::SystemTime::now();

            if buf[..2] != [0xCA, 0xAC] {
                return Err(Protocol {
                    message: format!(
                        "expected [0xCA, 0xAC], received [{:#04X}, {:#04X}]",
                        buf[0], buf[1]
                    ),
                });
            }

            let sequence_received = u16::from_be_bytes(buf[2..4].try_into().map_err(|err| {
                PoweneticsError::TryFromSlice {
                    err,
                    message: "Failed to parse sequence",
                }
            })?);

            if sequence != sequence_received {
                return Err(Protocol {
                    message: format!(
                        "expected sequence {}, received {}",
                        sequence, sequence_received
                    ),
                });
            }

            (sequence, _) = sequence.overflowing_add(1);

            for (i, channel) in self.data.channels.iter_mut().enumerate() {
                let offset = 4 + i * 5;
                let voltage =
                    u16::from_be_bytes(buf[offset..offset + 2].try_into().map_err(|err| {
                        PoweneticsError::TryFromSlice {
                            err,
                            message: "Failed to parse voltage",
                        }
                    })?);

                let current_bytes: [u8; 4] = [0, buf[offset + 2], buf[offset + 3], buf[offset + 4]];
                let current = u32::from_be_bytes(current_bytes);

                channel.voltage = voltage;
                channel.current = current;
                channel.update_energy(self.data.last_update)?;
            }

            let mut stop = false;

            for sub in &mut self.subscriptions {
                stop = sub
                    .update(&self.data)
                    .map_err(|e| PoweneticsError::Subscriber(e))?
                    || stop;
            }

            if stop {
                break;
            }
        }

        Ok(())
    }

    pub fn subscribe(&mut self, cb: Box<dyn PoweneticsSubscriber>) {
        self.subscriptions.push(cb);
    }

    pub fn data(&self) -> &PoweneticsData {
        &self.data
    }
}
