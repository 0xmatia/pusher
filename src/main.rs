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

use std::fs;
use std::io::Write;
use std::os::unix::prelude::RawFd;
use std::thread::sleep;
use std::time::Duration;
use std::{env, io};
use std::path::{PathBuf, Path};
use anyhow::{Result, anyhow, bail};
use libc::{isatty, open, 
    O_RDWR, // read-write
    O_NONBLOCK, // non blocking
    O_NOCTTY // open as non-controlling terminal
};

use mio::{Poll, Events, Token, Interest};
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
    let (serial_fd, baud_rate, kernel_path) = parse_input()?;
    let mut serial_device = match SerialDevice::init(serial_fd, baud_rate) {
        Ok(device) => device,
        Err(err) => bail!("Error opening serial device: {}", err)
    };
    let mut stdin_device = match StdinDevice::init() {
        Ok(stdin) => stdin,
        Err(err) => bail!("Failed initializing stdin: {}", err)
    };
    run(&mut serial_device, &mut stdin_device, kernel_path)?;
    Ok(())
}


fn run(serial_device: &mut SerialDevice, stdin_device: &mut StdinDevice, kernel_path: PathBuf) -> Result<()> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);

    // register serial port and stdin for polling
    poll.registry().register(serial_device, SERIAL_TOKEN, Interest::READABLE)?;
    poll.registry().register(stdin_device, STDIN_TOKEN, Interest::READABLE)?;

    let mut num_breaks = 0;
    loop {
        poll.poll(&mut events, None)?;
        for event in &events {
            match event.token() {
                SERIAL_TOKEN => {
                    let output_bytes = serial_device.read_all()?;
                    print!("{}", String::from_utf8_lossy(&output_bytes));

                    num_breaks += output_bytes.iter().filter(|&byte| *byte == 3).count();
                    if num_breaks == 3 {
                        io::stdout().flush()?; 
                        println!("[PUSHER] Sending kernel!"); 
                        num_breaks = 0;
                        send_kernel(serial_device, &kernel_path)?;
                        let output_bytes = serial_device.read_all()?;
                        print!("{}", String::from_utf8_lossy(&output_bytes));
                    }
                },
                STDIN_TOKEN => {
                    // read from stdin and write to serial 
                    let byte = stdin_device.read()?;
                    let bytes_written = serial_device.write_byte(byte as u8)?;
                    if bytes_written != 1 {
                        dbg!("weird");
                    }
                },
                Token(_) => eprintln!("Unknown token.")
            }
        }
    }
}

fn send_kernel(serial_device: &mut SerialDevice, kernel_path: &PathBuf) -> Result<()> {
    // first, send the size of the kernel as the device expects it 
    let kernel_size = fs::metadata(kernel_path)?.len() as u32;
    println!("[PUSHER] Kernel size: {}", kernel_size);
    assert!(std::u32::MAX > kernel_size);

    for i in 0..=3 {
        let s = ((kernel_size >> 8 * i) & 0xff) as u8;
        serial_device.write_byte(s)?;
    }
    
    sleep(Duration::from_secs(1)); // Nasty hack, but sometimes read just returns nothing...
    serial_device.flush()?;

    // now read the response
    let mut res = serial_device.read_all()?;

    // if "OK" didn't arrive, poll for read events for 5 seconds
    if res != vec!['O' as u8, 'K' as u8] {
        dbg!("Didn't receive OK, so polling for 5 seconds");
        let mut event = Events::with_capacity(1);
        let mut poll = Poll::new()?;
        poll.registry().register(serial_device, SERIAL_TOKEN, Interest::READABLE)?;
        poll.poll(&mut event, Some(Duration::from_secs(5)))?;
        if event.is_empty() {
            bail!("Didn't receive OK after sending the kernel size, aborting now")
        }
        res.append(&mut serial_device.read_all()?);
    }

    dbg!(&res);
    if res != vec!['O' as u8, 'K' as u8] {
        bail!("Didn't receive OK after sending the kernel size, aborting now")
    }
    println!("[PUSHER] Got response: \"{}\", sending image now!", String::from_utf8_lossy(&res));

    // send image now!
    let kernel_image = fs::read(kernel_path)?;

    for byte in 0..kernel_size {
        serial_device.write_byte(kernel_image[byte as usize])?;
    }

    println!("[PUSHER] Done! booting now\n\n");
    Ok(())
}

/// Parse command line arguments.
/// Checks if device exists and is a tty and if the kernel image exists
///
/// # Usage:
/// pusher <tty_device> <kernel_to_push>
///
/// # Return
/// The tty device as a `TTYPort` and a path to the kernel image
fn parse_input() -> Result<(RawFd, u32, PathBuf)> {
    let supplied_arguments: Vec<String>  = env::args().collect();
    if supplied_arguments.len() != 4 {
        return Err(anyhow!("Usage: pusher <device> <baudrate> <kernel>"));
    }
    // check if the supplied device exists
    if !Path::new(&supplied_arguments[1]).exists() {
        return Err(anyhow!("Device doesn't exists"));
    }
    // check if device is a tty
    let fd;
    unsafe {
        fd = open(supplied_arguments[1].as_ptr() as *const i8, O_RDWR | O_NOCTTY | O_NONBLOCK);
        if -1 == fd {
            return Err(anyhow!(format!("Couldn't open device: {}", io::Error::last_os_error())));
        }
        if 0 == isatty(fd) {
            return Err(anyhow!("Supplied device is not a tty!"));
        }
    }
    // check the the binary to push exists
    if !Path::new(&supplied_arguments[3]).exists() {
        return Err(anyhow!(format!("{} doesn't exist", supplied_arguments[2])));
    }
    Ok((fd, supplied_arguments[2].parse::<u32>()?, PathBuf::from(&supplied_arguments[3])))
}               

