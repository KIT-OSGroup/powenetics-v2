# powenetics-v2

Library and CLI application for power and energy measurements using the [Powenetics v2](https://hwbusters.com/psus/powenetics-v2-power-measurements-device-review/) device. Written in Rust. Tested only on Linux. Should work on Windows, too (I guess).

## Usage

### TL;DR
```
$ cargo build
$ target/debug/powenetics-v2 --csv power.csv /dev/ttyACM0
```

### Detailed Usage
Connect the Powenetics v2 device via USB. 
Powenetics v2 communicates via Serial-over-USB, so you will have to figure out the right serial port. 
Running `powenetics-v2` without any arguments will print a list of available serial ports where the right one is typically (probably not on Windows) marked as "USB".
Depending on your Linux distribution, it might be necessary to configure the appropriate file permissions for accessing the port.
Physical replugging is required in case any error occurs as there is no other way to reset the device. 

```
$ powenetics-v2 --help
Powenetics v2 command line tool

Usage: powenetics-v2 [OPTIONS] [PORT]

Arguments:
  [PORT]  Serial port name or path (run without arguments for list of available ports)

Options:
      --csv <path>            Write measurement data to CSV file
      --csv-server <address>  Provide CSV data over TCP
      --average <N>           Downsample by taking the average of N samples
  -h, --help                  Print help
```

With `--csv-server`, you can receive the measurement data from another system, for example:

```sh
# on the host with the Powenetics device
powenetics-v2 --csv-server '[::]:1234' /dev/ttyACM0

# on the client
nc powenetics-host 1234
```

## Output

Currently only CSV output is supported, either to a file or via TCP. 
Output consists of voltage (mV), current (mA), and accumulated energy (nJ) for each channel. 
Powenetics v2 provides ~1000 updates per second.
Measurement data is provided for the following channels (in this order):

* ATX 3.3V
* ATX 5V Standby
* ATX 12V
* ATX 5V
* EPS 12V #1
* ATX12VO 12V Standby
* EPS 12V #3
* EPS 12V #2
* PCIe 12V #3
* PCIe 12V #2
* PCIe Slot 3.3V
* PCIe Slot 12V
* PCIe 12V #1
