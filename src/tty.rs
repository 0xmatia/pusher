use std::io::{self, Read, stdin, Write, ErrorKind};
use std::os::unix::prelude::{RawFd, AsRawFd};
use anyhow::Result;
use mio::unix::SourceFd;
use mio_serial::{SerialStream, SerialPortBuilderExt};
use termios::*;
use mio::{event, Registry, Token, Interest};

/// Represents a serial / UART port
pub struct SerialDevice(SerialStream);

pub struct StdinDevice(RawFd);

impl SerialDevice {
    pub fn init(serial_path: String, baudrate: u32) -> io::Result<Self> {
        let mut dev = mio_serial::new(serial_path, baudrate).open_native_async()?;
        dev.set_exclusive(true)?;
        Ok(Self(dev))
    }

    /// Read until EOF from device, return vector of bytes read.
    pub fn read_byte(&mut self) -> Result<u8, io::Error> {
        let mut buffer = [0u8; 1];
        //let mut buffer = Vec::new();
        //self.0.read_to_end(&mut buffer)?;
           match self.0.read(&mut buffer) {
               Ok(count) => {
                   if count == 1 {
                        return Ok(buffer[0]);
                   }
                    return Err(io::Error::new(ErrorKind::Other, "Device disconnected?"));
                }
               Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return Err(e);
               }
               Err(e) => {
                    println!("Quitting due to read error: {}", e);
                    return Err(e);
               }
           }
    }

    /// Flush
    pub fn flush(&mut self) -> Result<(), io::Error> {
        self.0.flush()?;
        Ok(())
    }

    /// Write one byte to serial device, and flush
    pub fn write_byte(&mut self, byte: u8) -> io::Result<usize> {
        let bytes_written = self.0.write(&[byte])?;
        Ok(bytes_written)
    }
}

/// Implement event source for SerialDevice to be able to register it 
/// in the Registry and Poll
impl event::Source for SerialDevice {
   fn register(&mut self, registry: &Registry, token: Token, interests: Interest)
        -> io::Result<()>
    {
        self.0.register(registry, token, interests)
    }

    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest)
        -> io::Result<()>
    {
        self.0.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        self.0.deregister(registry)
    }
}

impl StdinDevice { 
    /// Setup stdin for serial communication:
    /// - Turn terminal echo off. Unless the "otherside" returns the output, nothing will be shown.
    /// - Turn off canonical mode. This means read doesn't wait for NL to proceed.
    pub fn init() -> io::Result<Self> {
        let mut termios = Termios::from_fd(stdin().as_raw_fd())?;

        // disable canonical mode and turn echo off
        termios.c_lflag &= !(ECHO | ICANON);

        tcsetattr(stdin().as_raw_fd(), TCSANOW, &termios)?;
        Ok(Self(stdin().as_raw_fd()))
    }

    /// Read from stdin one byte.
    pub fn read(&mut self) -> Result<char, io::Error> {
        let mut buffer = [0u8, 1];
        stdin().lock().read(&mut buffer)?;
        // print!("{:?}", buffer);
        Ok(buffer[0] as char)
    }
}

/// Implement event source for StdinDevice to be able to register it 
/// in the Registry and Poll
impl event::Source for StdinDevice {
   fn register(&mut self, registry: &Registry, token: Token, interests: Interest)
        -> io::Result<()>
    {
        SourceFd(&self.0).register(registry, token, interests)
    }

    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest)
        -> io::Result<()>
    {
        SourceFd(&self.0).reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.0).deregister(registry)
    }
}

