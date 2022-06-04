use std::fs::File;
use std::io::{self, Read, stdin, Write};
use std::os::unix::prelude::{RawFd, FromRawFd, AsRawFd};
use std::thread::sleep;
use std::time::Duration;
use anyhow::Result;
use mio::unix::SourceFd;
use termios::*;
use mio::{event, Registry, Token, Interest};

/// Represents a serial / UART port
pub struct SerialDevice {
    device: File,
    _baudrate: u32
}

pub struct StdinDevice(RawFd);

impl SerialDevice {
    pub fn init(serial_fd: RawFd, baudrate: u32) -> io::Result<Self> {
        let mut termios = Termios::from_fd(serial_fd)?;
        termios.c_cc[VTIME] = 0;
        termios.c_cc[VMIN] = 0;

        termios.c_iflag = 0;
        termios.c_oflag = 0;
        termios.c_cflag = CS8 | CREAD | CLOCAL; // 8n1
        termios.c_lflag = !ICANON;

        // set baudrate
        cfsetspeed(&mut termios, baudrate)?;
        tcsetattr(serial_fd, TCSANOW, &termios)?;
        Ok(Self {
            device: unsafe { File::from_raw_fd(serial_fd) },
           _baudrate: baudrate
        } )
    }

    /// Read until EOF from device, return vector of bytes read.
    pub fn read_all(&mut self) -> Result<Vec<u8>, io::Error> {
        let mut buffer = Vec::new();
        self.device.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    /// Flush
    pub fn flush(&mut self) -> Result<(), io::Error> {
        self.device.flush()?;
        Ok(())
    }

    /// Write one byte to serial device, and flush
    pub fn write_byte(&mut self, byte: u8) -> io::Result<usize> {
        let bytes_written = self.device.write(&[byte])?;
        sleep(Duration::from_millis(2));
        Ok(bytes_written)
    }
}

/// Implement event source for SerialDevice to be able to register it 
/// in the Registry and Poll
impl event::Source for SerialDevice {
   fn register(&mut self, registry: &Registry, token: Token, interests: Interest)
        -> io::Result<()>
    {
        SourceFd(&self.device.as_raw_fd()).register(registry, token, interests)
    }

    fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest)
        -> io::Result<()>
    {
        SourceFd(&self.device.as_raw_fd()).reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.device.as_raw_fd()).deregister(registry)
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
        let mut buffer = Vec::new();
        buffer.resize(1, 0);
        stdin().lock().read_exact(&mut buffer)?;
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

