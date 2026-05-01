use std::error::Error;
use std::fmt;

/// An error that can occur when making an OpenGL context current or
/// not-current.
#[derive(Debug)]
pub struct MakeCurrentError;

/// An error that can occur when swapping the OpenGL buffers.
#[derive(Debug)]
pub struct SwapBuffersError;

/// An error that can occur during the creation or lifetime of a window.
#[derive(Debug)]
#[non_exhaustive]
pub enum WindowError {
    /// A platform-specific error occurred.
    Platform(String),

    /// Failed to create an OpenGL context
    OpenGl(String),

    /// The parent window passed was invalid.
    InvalidParent,
}

/// An error that can occur when waking up a event loop from another thread.
#[derive(Debug)]
pub struct WakeupError;

impl Error for WindowError {}
impl fmt::Display for WindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowError::Platform(err) => write!(f, "platform error: {}", err),
            WindowError::OpenGl(err) => write!(f, "failed to create opengl context: {}", err),
            WindowError::InvalidParent => write!(f, "invalid parent window handle"),
        }
    }
}

impl Error for WakeupError {}
impl fmt::Display for WakeupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to wake up window, possibly because it's closed")
    }
}

impl Error for MakeCurrentError {}
impl fmt::Display for MakeCurrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to make opengl context current")
    }
}

impl Error for SwapBuffersError {}
impl fmt::Display for SwapBuffersError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to swap opengl buffers")
    }
}
