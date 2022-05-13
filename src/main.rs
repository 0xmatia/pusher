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
mod errors;

use std::{env, io};
use std::path::{PathBuf, Path};
use anyhow::{Result, anyhow};
use libc::{isatty, open, 
    O_RDWR, // read-write
    O_NONBLOCK, // non blocking
    O_NOCTTY // open as non-controlling terminal
};

use tty::TTYPort;
use errors::PusherErrors::{
    IOError,
};

fn main() -> Result<()> {
    let (device, kernel_path) = parse_input()?;
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
fn parse_input() -> Result<(TTYPort, PathBuf)> {
    let supplied_arguments: Vec<String>  = env::args().collect();
    if supplied_arguments.len() != 3 {
        return Err(anyhow!("Usage: pusher <device> <kernel>"));
    }
    // check if the supplied device exists
    if !Path::new(&supplied_arguments[1]).exists() {
        return Err(IOError("Device doesn't exists".into()).into());
    }
    // check if device is a tty
    let fd;
    unsafe {
        fd = open(supplied_arguments[1].as_ptr() as *const i8, O_RDWR | O_NOCTTY | O_NONBLOCK);
        if -1 == fd {
            return Err(IOError(format!("Couldn't open device: {}", io::Error::last_os_error())
                    .into()).into());
        }
        if 0 == isatty(fd) {
            return Err(IOError("Supplied device is not a tty!".into()).into());
        }
    }
    // check the the binary to push exists
    if !Path::new(&supplied_arguments[2]).exists() {
        return Err(IOError(format!("{} doesn't exist", supplied_arguments[2]).into()).into());
    }
    Ok((TTYPort::new(fd), PathBuf::from(&supplied_arguments[2])))
}               

