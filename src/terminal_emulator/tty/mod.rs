pub(crate) mod unix;
// tty/mod.rs
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, io};

use polling::{Event, PollMode, Poller};

pub use unix::*;

pub use self::unix::*;

/// Configuration for the `Pty` interface
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Options {
    /// Shell configuration
    pub shell: Option<Shell>,
    /// Working directory
    pub working_directory: Option<PathBuf>,
    /// Environment variables
    pub env: HashMap<String, String>,
}

/// Shell configuration
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Shell {
    pub program: String,
    pub args: Vec<String>,
}

impl Shell {
    pub fn new(program: String, args: Vec<String>) -> Self {
        Self { program, args }
    }
}

/// Unified interface for PTY I/O
pub trait EventedReadWrite {
    type Reader: io::Read;
    type Writer: io::Write;

    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        interest: Event,
        poll_opts: PollMode,
    ) -> io::Result<()>;

    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        interest: Event,
        poll_opts: PollMode,
    ) -> io::Result<()>;

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()>;

    fn reader(&mut self) -> &mut Self::Reader;
    fn writer(&mut self) -> &mut Self::Writer;
}

/// Child process events
#[derive(Debug, PartialEq, Eq)]
pub enum ChildEvent {
    Exited(Option<i32>),
}

/// PTY interface extension for child process events
pub trait EventedPty: EventedReadWrite {
    fn next_child_event(&mut self) -> Option<ChildEvent>;
}

/// Terminal environment setup
pub fn setup_env() {
    // Set default TERM if not already configured
    if env::var("TERM").is_err() {
        env::set_var("TERM", "xterm-256color");
    }

    // Advertise truecolor support
    env::set_var("COLORTERM", "truecolor");
}