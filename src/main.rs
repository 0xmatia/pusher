//! # Pusher by Elad Matia
//!
//! ## The problem
//! The purpose of this simple crate is to make the life of a kernel / embedded developer easier.
//! The main issue I encounterd while developing a simple kernel for the rpi3 was the repetetive
//! action of inserting the sdcard to my computer everytime I wanted to update the kernel.
//!
//! ## The solution
//! The solution was to write a very simple PIC that sits on the rpi and sends a signal over UART
//! signaling when it is ready to receive the kernel. The PIC relocated after loading so that the
//! kernel it receives can be written to the load address of the rpi. On the other side of the UART
//! this binary waits for the signal and sends the binary. Then, the PIC jumps to the newly pushed
//! kernel. This process will make your life much simpler when developing.

mod tty;

use anyhow::{anyhow, bail, Result};

use std::fs;
use std::io::{Write, stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, io};

use mio::{Events, Interest, Poll, Token};
use tty::{SerialDevice, StdinDevice};

const PUSHER_LOGO: &str = r#"
__________             .__                  
\______   \__ __  _____|  |__   ___________ 
 |     ___/  |  \/  ___/  |  \_/ __ \_  __ \
 |    |   |  |  /\___ \|   Y  \  ___/|  | \/
 |____|   |____//____  >___|  /\___  >__|   
                     \/     \/     \/       
"#;
const SERIAL_TOKEN: Token = Token(0);
const STDIN_TOKEN: Token = Token(1);

fn main() -> Result<()> {
    println!("{}\n[PUSHER] Pusher is waiting...", PUSHER_LOGO);
    let (serial_path, baud_rate, kernel_path) = parse_input()?;
    let mut serial_device = match SerialDevice::init(serial_path, baud_rate) {
        Ok(device) => device,
        Err(err) => bail!("Error opening serial device: {}", err),
    };
    let mut stdin_device = match StdinDevice::init() {
        Ok(stdin) => stdin,
        Err(err) => bail!("Failed initializing stdin: {}", err),
    };
    run(&mut serial_device, &mut stdin_device, kernel_path)?;
    Ok(())
}

/// Infinite loop that waits for 'loaders' and pushes kernels
fn run(
    serial_device: &mut SerialDevice,
    stdin_device: &mut StdinDevice,
    kernel_path: PathBuf,
) -> Result<()> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);

    // register serial port and stdin for polling
    poll.registry()
        .register(serial_device, SERIAL_TOKEN, Interest::READABLE)?;
    poll.registry()
        .register(stdin_device, STDIN_TOKEN, Interest::READABLE)?;

    let mut num_breaks = 0;
    loop {
        poll.poll(&mut events, None)?;
        for event in &events {
            match event.token() {
                SERIAL_TOKEN => loop {
                    let byte = match serial_device.read_byte() {
                        Ok(byte) => byte,
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            break;
                        }
                        Err(err) => {
                            return Err(err.into());
                        }
                    };
                    if byte == 3 {
                        num_breaks += 1;
                    }
                    if num_breaks == 3 {
                        io::stdout().flush()?;
                        println!("[PUSHER] Sending kernel!");
                        num_breaks = 0;
                        send_kernel(serial_device, &kernel_path, &mut poll)?;
                        continue;
                    }
                    print!("{}", byte as char);
                },
                STDIN_TOKEN => {
                    // TODO: read from stdin and write to serial. this is somewhat broken,
                    // as it expects enter for read to return something
                    let byte = stdin_device.read()?;
                    if byte == 3 as char {
                        continue;
                    }
                    let bytes_written = serial_device.write_byte(byte as u8)?;
                    if bytes_written != 1 {
                        dbg!("weird");
                    }
                }
                Token(_) => eprintln!("Unknown token."),
            }
        }
    }
}

/// Send the kernel image
///
/// # process
/// The process is sending 4 bytes representing the size of the image, waiting for "OK",
/// and then sending the image itself
fn send_kernel(
    serial_device: &mut SerialDevice,
    kernel_path: &PathBuf,
    poll: &mut Poll,
) -> Result<()> {
    // first, send the size of the kernel as the device expects it
    let kernel_size = fs::metadata(kernel_path)?.len() as u32;
    let mut res = Vec::new();
    println!("[PUSHER] kernel size: {}", kernel_size);
    assert!(std::u32::MAX > kernel_size);

    for i in 0..=3 {
        let s = ((kernel_size >> 8 * i) & 0xff) as u8;
        serial_device.write_byte(s)?;
    }

    serial_device.flush()?;
    let mut events = Events::with_capacity(1024);

    // poll twice in case the OK will come in delay
    for _ in 0..2 {
        poll.poll(&mut events, Some(Duration::from_secs(2)))?;
        for event in &events {
            match event.token() {
                SERIAL_TOKEN => loop {
                    res.push(match serial_device.read_byte() {
                        Ok(byte) => byte,
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            break;
                        }
                        Err(err) => {
                            return Err(err.into());
                        }
                    });
                },
                Token(_) => continue,
            }
        }
        if res == vec!['O' as u8, 'K' as u8] {
            break;
        }
    }

    if res != vec!['O' as u8, 'K' as u8] {
        dbg!("didn't receive ok: {}", &res);
        return Err(anyhow!("Didn't receive OK"));
    }

    println!(
        "[PUSHER] got response: \"{}\", sending image now!",
        String::from_utf8_lossy(&res)
    );
    // send image now!
    let kernel_image = fs::read(kernel_path)?;

    for i in 0..kernel_size {
        serial_device.write_byte(kernel_image[i as usize])?;
    }
    serial_device.flush()?;
    stdout().flush()?;
    Ok(())
}

/// Parse command line arguments.
/// Checks if device and kernel image exist
///
/// # Usage:
/// pusher <tty_device> <kernel_to_push>
///
/// # Return
/// The tty device as a `TTYPort` and a path to the kernel image
fn parse_input() -> Result<(String, u32, PathBuf)> {
    let supplied_arguments: Vec<String> = env::args().collect();
    if supplied_arguments.len() != 4 {
        return Err(anyhow!("Usage: pusher <device> <baudrate> <kernel>"));
    }
    // check if the supplied device exists
    if !Path::new(&supplied_arguments[1]).exists() {
        return Err(anyhow!("Device doesn't exists"));
    }

    // check the the binary to push exists
    if !Path::new(&supplied_arguments[3]).exists() {
        return Err(anyhow!(format!("{} doesn't exist", supplied_arguments[2])));
    }
    Ok((
        supplied_arguments[1].clone(),
        supplied_arguments[2].parse::<u32>()?,
        PathBuf::from(&supplied_arguments[3]),
    ))
}
