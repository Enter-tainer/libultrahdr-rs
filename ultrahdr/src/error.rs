use std::fmt;

/// Errors produced by the ultrahdr crate.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Invalid parameter passed to an API function.
    InvalidParam(String),
    /// A codec operation failed.
    CodecError(String),
    /// Memory allocation failed.
    MemError(String),
    /// Feature not supported.
    UnsupportedFeature(String),
    /// JPEG encoding/decoding error.
    JpegError(String),
    /// Metadata parsing error.
    MetadataError(String),
}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidParam(msg) => write!(f, "invalid parameter: {msg}"),
            Error::CodecError(msg) => write!(f, "codec error: {msg}"),
            Error::MemError(msg) => write!(f, "memory error: {msg}"),
            Error::UnsupportedFeature(msg) => write!(f, "unsupported feature: {msg}"),
            Error::JpegError(msg) => write!(f, "JPEG error: {msg}"),
            Error::MetadataError(msg) => write!(f, "metadata error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_includes_message() {
        let e = Error::InvalidParam("bad width".into());
        let msg = format!("{e}");
        assert!(msg.contains("bad width"));
    }

    #[test]
    fn error_implements_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(Error::InvalidParam("test".into()));
        assert!(e.to_string().contains("test"));
    }
}
