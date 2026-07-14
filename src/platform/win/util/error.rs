use crate::platform::win::util::widestr::WideString;
use crate::{OpenGlError, WindowError};
use std::fmt::Display;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Diagnostics::Debug::*;
use windows_sys::core::PWSTR;

/// A generic Windows API error, queried via [`GetLastError`].
#[derive(Debug)]
pub struct Win32Error {
    /// The error code returned by the Windows API.
    pub code: u32,
    /// The context/function where the error occurred, if available.
    pub context: Option<String>,
}

impl Win32Error {
    /// Creates a new error by querying the last error from the Windows API.
    pub fn last_error() -> Self {
        Self {
            code: unsafe { GetLastError() },
            context: None,
        }
    }

    /// Returns the error message associated with the error code, if available.
    pub fn message(&self) -> Option<String> {
        unsafe {
            let mut buffer = null_mut::<u16>();
            let chars = FormatMessageW(
                FORMAT_MESSAGE_ALLOCATE_BUFFER
                    | FORMAT_MESSAGE_FROM_SYSTEM
                    | FORMAT_MESSAGE_IGNORE_INSERTS,
                null_mut(),
                self.code,
                0,
                &mut buffer as *mut PWSTR as *mut _,
                0,
                null_mut(),
            );

            if chars == 0 || buffer.is_null() {
                None
            } else {
                Some(WideString::from_ptr(buffer, chars as usize).to_string_lossy())
            }
        }
    }

    /// Sets the context for the error, which can be useful for debugging.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

impl Display for Win32Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(context) = &self.context {
            write!(f, "{}: ", context)?;
        }

        if let Some(message) = self.message() {
            write!(f, "{}", message)?;
        } else {
            write!(f, "win32 error")?;
        }

        write!(f, " ({})", self.code)
    }
}

impl From<Win32Error> for WindowError {
    fn from(err: Win32Error) -> Self {
        Self::Platform(err.to_string())
    }
}

impl From<Win32Error> for OpenGlError {
    fn from(err: Win32Error) -> Self {
        Self::Platform(err.to_string())
    }
}
