use crate::sys;
use std::error::Error as StdError;
use std::ffi::CStr;
use std::fmt;

/// Error produced by the safe wrappers around `libultrahdr`.
#[derive(Debug, Clone)]
pub struct Error {
    /// Error code returned by `libultrahdr`.
    pub code: sys::uhdr_codec_err_t,
    /// Optional human-readable detail string when provided by the library.
    pub detail: Option<String>,
}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn alloc() -> Self {
        Self {
            code: sys::uhdr_codec_err_t::UHDR_CODEC_MEM_ERROR,
            detail: Some("allocation failed".to_string()),
        }
    }

    pub(crate) fn invalid_param(msg: impl Into<String>) -> Self {
        Self {
            code: sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_PARAM,
            detail: Some(msg.into()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(detail) = &self.detail {
            write!(f, "{:?}: {}", self.code, detail)
        } else {
            write!(f, "{:?}", self.code)
        }
    }
}

impl StdError for Error {}

pub(crate) fn check(info: sys::uhdr_error_info_t) -> Result<()> {
    if info.error_code == sys::uhdr_codec_err_t::UHDR_CODEC_OK {
        return Ok(());
    }
    let detail = if info.has_detail != 0 && info.detail[0] != 0 {
        // SAFETY: detail is a char buffer owned by the struct and expected to be null-terminated.
        let s = unsafe { CStr::from_ptr(info.detail.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    };
    Err(Error {
        code: info.error_code,
        detail,
    })
}
