use std::io::Error;
use libc::close;

/// Represents a serial / UART port 
pub struct TTYPort {
    fd: i32 
}

impl TTYPort {
    /// Create a new TTYPort, using the supplied fd.
    pub fn new(fd: i32) -> Self {
        TTYPort { fd }
    }
}

impl Drop for TTYPort {
    /// Implement drop the  close the fd automatically when the object goes out of scope.
    fn drop(&mut self) {
        unsafe {
            if 0 != close(self.fd) {
                eprintln!("Failed closing tty: {}", Error::last_os_error());
            }
        }
    }
}
