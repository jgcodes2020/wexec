use std::{error::Error, fmt::Display};

/// Indicates that the main event loop has shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventLoopShutdown;

impl Display for EventLoopShutdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("wexec's event loop has shut down")
    }
}

impl Error for EventLoopShutdown {}