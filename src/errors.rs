//! This modules describes all the error types in the pusher crate.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PusherErrors {
    #[error("{0}")]
    /// IO related error
    IOError(String)
}


