use std::error::Error;
use std::fmt;

/// An error that can occur when creating an OpenGL context.
#[derive(Debug, Clone)]
pub struct OpenGlError(pub String);

/// An error that can occur when making an OpenGL context current or
/// not-current.
#[derive(Debug, Clone, Copy)]
pub struct MakeCurrentError;

/// An error that can occur when swapping the OpenGL buffers.
#[derive(Debug, Clone, Copy)]
pub struct SwapBuffersError;

/// An error that can occur during the creation or lifetime of a window.
#[derive(Debug)]
#[non_exhaustive]
pub enum WindowError {
    /// [`WindowFactory`](crate::WindowFactory) returned an error.
    Factory(Box<dyn Error + Send + Sync>),

    /// A platform-specific error occurred.
    Platform(String),

    /// The parent window handle that was passed is invalid.
    InvalidParent,
}

/// An error that can occur when waking up a event loop from another thread.
#[derive(Debug)]
pub struct WakeupError;

impl Error for WindowError {}
impl fmt::Display for WindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowError::Factory(err) => write!(f, "{}", err),
            WindowError::Platform(err) => write!(f, "platform error: {}", err),
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

impl Error for SwapBuffersError {}
impl fmt::Display for SwapBuffersError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to swap opengl buffers")
    }
}

impl Error for MakeCurrentError {}
impl fmt::Display for MakeCurrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to make opengl context current")
    }
}

impl Error for OpenGlError {}
impl fmt::Display for OpenGlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to create opengl context: {}", self.0)
    }
}
