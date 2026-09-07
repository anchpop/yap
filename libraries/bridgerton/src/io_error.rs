//! `std::io::Error` cannot be reconstructed across languages. On both platforms
//! it crosses as a value carrying its portable category, optional OS code, and
//! diagnostic message.
use crate::bridge;

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Clone, Copy, Debug)]
pub enum IoErrorKind {
    NotFound,
    PermissionDenied,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    NotConnected,
    AddrInUse,
    AddrNotAvailable,
    BrokenPipe,
    AlreadyExists,
    WouldBlock,
    InvalidInput,
    InvalidData,
    TimedOut,
    WriteZero,
    Interrupted,
    Unsupported,
    UnexpectedEof,
    OutOfMemory,
    Other,
}

#[bridge(transparent)]
#[derive(bridgerton::serde::Serialize, bridgerton::serde::Deserialize)]
#[serde(crate = "bridgerton::serde")]
#[derive(Clone, Debug)]
pub struct IoError {
    pub kind: IoErrorKind,
    pub message: String,
    pub os_code: Option<i32>,
}

impl From<std::io::Error> for IoError {
    fn from(error: std::io::Error) -> Self {
        use std::io::ErrorKind;
        let kind = match error.kind() {
            ErrorKind::NotFound => IoErrorKind::NotFound,
            ErrorKind::PermissionDenied => IoErrorKind::PermissionDenied,
            ErrorKind::ConnectionRefused => IoErrorKind::ConnectionRefused,
            ErrorKind::ConnectionReset => IoErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted => IoErrorKind::ConnectionAborted,
            ErrorKind::NotConnected => IoErrorKind::NotConnected,
            ErrorKind::AddrInUse => IoErrorKind::AddrInUse,
            ErrorKind::AddrNotAvailable => IoErrorKind::AddrNotAvailable,
            ErrorKind::BrokenPipe => IoErrorKind::BrokenPipe,
            ErrorKind::AlreadyExists => IoErrorKind::AlreadyExists,
            ErrorKind::WouldBlock => IoErrorKind::WouldBlock,
            ErrorKind::InvalidInput => IoErrorKind::InvalidInput,
            ErrorKind::InvalidData => IoErrorKind::InvalidData,
            ErrorKind::TimedOut => IoErrorKind::TimedOut,
            ErrorKind::WriteZero => IoErrorKind::WriteZero,
            ErrorKind::Interrupted => IoErrorKind::Interrupted,
            ErrorKind::Unsupported => IoErrorKind::Unsupported,
            ErrorKind::UnexpectedEof => IoErrorKind::UnexpectedEof,
            ErrorKind::OutOfMemory => IoErrorKind::OutOfMemory,
            _ => IoErrorKind::Other,
        };
        Self {
            kind,
            message: error.to_string(),
            os_code: error.raw_os_error(),
        }
    }
}
