use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use serialport::SerialPortType;

use powenetics_v2::{POWENETICS_USB_PID, POWENETICS_USB_VID};

mod csv;

/// Powenetics v2 command line tool
#[derive(Parser)]
struct Cli {
    /// Write measurement data to CSV file
    #[arg(long, value_name = "path")]
    csv: Option<PathBuf>,
    /// Serial port name or path (run without arguments for list of available ports)
    port: Option<String>,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    if args.port.is_none() {
        println!("Usage: see --help");

        println!("Available serial ports:");

        let ports = serialport::available_ports()?;
        let mut have_port = false;

        for port in &ports {
            match &port.port_type {
                SerialPortType::UsbPort(usb) => {
                    if usb.vid != POWENETICS_USB_VID || usb.pid != POWENETICS_USB_PID {
                        continue;
                    }

                    have_port = true;
                    print!("{} (USB)", port.port_name);
                }
                _ => {
                    // this may or may not be a Powenetics device
                    have_port = true;
                    println!("{} {:?}", port.port_name, port.port_type);
                }
            }
        }

        if !have_port {
            println!("No ports available. Make sure that your Powenetics device is plugged in.");
        }

        return Ok(());
    }

    let mut p = powenetics_v2::new(&*args.port.unwrap())?;

    if args.csv.is_some() {
        csv::subscribe_csv(&mut p, &args.csv.unwrap())?;
    }

    p.start_measurement()?;

    Ok(())
}
