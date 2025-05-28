use std::ffi::CStr;
use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::process::{Child, Command};
use std::sync::Arc;
use std::{env, ptr};
use libc::c_int;
use nix::unistd::Pid;
use nix::fcntl::{self, FcntlArg, OFlag};
use nix::pty::{openpty, Winsize};
use nix::sys::ioctl;
use nix::sys::signal::{self, SigHandler, Signal};
use nix::sys::termios::{self, InputFlags, SetArg,Termios};
use nix::unistd::{self, Uid, User};
use polling::{Event, PollMode, Poller};
use signal_hook::low_level::{pipe as signal_pipe, unregister as unregister_signal};
use signal_hook::{consts as sigconsts, SigId};

// Add this at the top of unix.rs
use log::error;
use crate::terminal_emulator::event::{OnResize, WindowSize};
use crate::terminal_emulator::tty::{ChildEvent, EventedPty, EventedReadWrite, Options};
nix::ioctl_write_ptr!(tiocswinsz, 'T', 103, nix::pty::Winsize);
nix::ioctl_none!(tiocsctty, 'T', 98);

pub(crate) const PTY_READ_WRITE_TOKEN: usize = 0;

pub(crate) const PTY_CHILD_EVENT_TOKEN: usize = 1;

macro_rules! die {
    ($($arg:tt)*) => {{
        error!($($arg)*);
        std::process::exit(1);
    }}
}

fn set_controlling_terminal(fd: RawFd) -> std::result::Result<c_int, Error> {
    unsafe { tiocsctty(fd) }
        .map_err(|e| Error::new(ErrorKind::Other, e))
}

/// User information structure
struct Passwd<'a> {
    name: &'a str,
    dir: &'a str,
    shell: &'a str,
}

/// Get user information
fn get_pw_entry() -> Result<Passwd<'static>> {
    let user = User::from_uid(Uid::current())?
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "User not found"))?;

    Ok(Passwd {
        name: user.name.as_str(),
        dir: user.dir.as_str()?,
        shell: user.shell.as_str()?,
    })
}

pub struct Pty {
    child: Child,
    file: File,
    signals: UnixStream,
    sig_id: SigId,
}



impl Pty {
    pub fn new(config: &Options, window_size: WindowSize) -> Result<Self> {
        let winsize = window_size.to_winsize();
        let pty = openpty(Some(&winsize), None)?;
        let master = unsafe { OwnedFd::from_raw_fd(pty.master) };
        let slave = unsafe { OwnedFd::from_raw_fd(pty.slave) };

        // Remove the old placeholder implementation
        // and use the actual initialization code
        let (child, file, signals, sig_id) = setup_pty(config, master, slave)?;

        Ok(Pty {
            child,
            file,
            signals,
            sig_id
        })
    }
    pub fn child(&self) -> &Child {
        &self.child
    }

    pub fn file(&self) -> &File {
        &self.file
    }
}
// Remove the nested setup_pty definition and fix parameters
fn setup_pty(
    config: &Options,
    master: OwnedFd,
    slave: OwnedFd
) -> Result<(Child, File, UnixStream, SigId)> {
    let user = ShellUser::from_env()?;
    let mut builder = if let Some(shell) = &config.shell {
        let mut cmd = Command::new(&shell.program);
        cmd.args(&shell.args);
        cmd
    } else {
        default_shell_command(&user.shell, &user.user, &user.home)
    };

    builder
        .stdin(slave.try_clone()?)
        .stdout(slave.try_clone()?)
        .stderr(slave)
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("USER", &user.user)
        .env("HOME", &user.home)
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID");

    if let Some(wd) = &config.working_directory {
        builder.current_dir(wd);
    }

    unsafe {
        builder.pre_exec(move || {
            unistd::setsid()?;
            ioctl::ioctl(slave.as_raw_fd(), tiocsctty)?;

            unsafe {
                libc::close(slave.as_raw_fd());
                libc::close(master.as_raw_fd());
            }

            for sig in &[
                Signal::SIGCHLD,
                Signal::SIGHUP,
                Signal::SIGINT,
                Signal::SIGQUIT,
                Signal::SIGTERM,
            ] {
                signal::signal(*sig, SigHandler::SigDfl)?;
            }

            Ok(())
        });
    }

    let (sender, recv) = UnixStream::pair()?;
    let sig_id = signal_pipe::register(sigconsts::SIGCHLD, sender)?;
    recv.set_nonblocking(true)?;

    let child = builder.spawn().map_err(|e| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to spawn shell: {}", e),
        )
    })?;

    set_nonblocking(master.as_raw_fd())?;

    Ok((
        child,
        File::from(master),
        recv,
        sig_id
    ))
}

// Fix PathBuf to string conversion
fn get_pw_entry() -> Result<Passwd<'static>> {
    let user = User::from_uid(Uid::current())?
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "User not found"))?;

    Ok(Passwd {
        name: user.name.as_str(),
        dir: user.dir.to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid home directory"))?,
        shell: user.shell.to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid shell path"))?,
    })
}

// Remove duplicate implementations
struct ShellUser {
    user: String,
    home: String,
    shell: String,
}

impl ShellUser {
    fn from_env() -> Result<Self> {
        let pw = get_pw_entry()?;

        Ok(Self {
            user: env::var("USER").unwrap_or_else(|_| pw.name.to_owned()),
            home: env::var("HOME").unwrap_or_else(|_| pw.dir.to_owned()),
            shell: env::var("SHELL").unwrap_or_else(|_| pw.shell.to_owned()),
        })
    }
}


/// User information
struct ShellUser {
    user: String,
    home: String,
    shell: String,
}

impl ShellUser {
    fn from_env() -> Result<Self> {
        let pw = get_pw_entry();

        let user = env::var("USER")
            .or_else(|_| pw.as_ref().map(|p| p.name.to_string()))
            .map_err(|_| Error::new(ErrorKind::Other, "Failed to get username"))?;

        let home = env::var("HOME")
            .or_else(|_| pw.as_ref().map(|p| p.dir.to_string()))
            .map_err(|_| Error::new(ErrorKind::Other, "Failed to get home directory"))?;

        let shell = env::var("SHELL")
            .or_else(|_| pw.as_ref().map(|p| p.shell.to_string()))
            .map_err(|_| Error::new(ErrorKind::Other, "Failed to get shell"))?;

        Ok(Self { user, home, shell })
    }
}

#[cfg(not(target_os = "macos"))]
fn default_shell_command(shell: &str, _user: &str, _home: &str) -> Command {
    Command::new(shell)
}

#[cfg(target_os = "macos")]
fn default_shell_command(shell: &str, user: &str, home: &str) -> Command {
    let shell_name = shell.rsplit('/').next().unwrap();
    let mut login_command = Command::new("/usr/bin/login");
    let exec = format!("exec -a -{} {}", shell_name, shell);
    let has_home_hushlogin = Path::new(home).join(".hushlogin").exists();
    let flags = if has_home_hushlogin { "-qflp" } else { "-flp" };
    login_command.args([flags, user, "/bin/zsh", "-fc", &exec]);
    login_command
}

/// Create a new PTY
pub fn new(config: &Options, window_size: WindowSize, window_id: u64) -> Result<Pty> {
    let winsize = window_size.to_winsize();
    let pty = openpty(Some(&winsize), None)?;
    let master = unsafe { OwnedFd::from_raw_fd(pty.master) };
    let slave = unsafe { OwnedFd::from_raw_fd(pty.slave) };
    from_fd(config, window_id, master, slave)
}

pub fn from_fd(config: &Options, window_id: u64, master: OwnedFd, slave: OwnedFd) -> Result<Pty> {
    let master_fd = master.as_raw_fd();
    let slave_fd = slave.as_raw_fd();

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if let Ok(mut termios) = termios::tcgetattr(master_fd) {
        termios.input_flags.set(InputFlags::IUTF8, true);
        let _ = termios::tcsetattr(master_fd, SetArg::TCSANOW, &termios);
    }

    let user = ShellUser::from_env()?;

    let mut builder = if let Some(shell) = config.shell.as_ref() {
        let mut cmd = Command::new(&shell.program);
        cmd.args(shell.args.as_slice());
        cmd
    } else {
        default_shell_command(&user.shell, &user.user, &user.home)
    };

    builder.stdin(slave.try_clone()?);
    builder.stderr(slave.try_clone()?);
    builder.stdout(slave);

    let window_id = window_id.to_string();
    builder.env("TERMINAUX_WINDOW_ID", &window_id);
    builder.env("USER", user.user);
    builder.env("HOME", user.home);
    builder.env("WINDOWID", window_id);
    for (key, value) in &config.env {
        builder.env(key, value);
    }

    builder.env_remove("XDG_ACTIVATION_TOKEN");
    builder.env_remove("DESKTOP_STARTUP_ID");

    let working_directory = config.working_directory.clone();
    unsafe {
        builder.pre_exec(move || {
            unistd::setsid().map_err(|e| Error::new(ErrorKind::Other, e))?;

            if let Some(working_directory) = working_directory.as_ref() {
                let _ = env::set_current_dir(working_directory);
            }

            set_controlling_terminal(slave_fd)?;

            unsafe {
                nix::libc::close(slave_fd);
                nix::libc::close(master_fd);
            }

            for sig in &[Signal::SIGCHLD, Signal::SIGHUP, Signal::SIGINT,
                Signal::SIGQUIT, Signal::SIGTERM, Signal::SIGALRM] {
                signal::signal(*sig, SigHandler::SigDfl)?;
            }

            Ok(())
        });
    }

    let (signals, sig_id) = {
        let (sender, recv) = UnixStream::pair()?;
        let sig_id = signal_pipe::register(sigconsts::SIGCHLD, sender)?;
        recv.set_nonblocking(true)?;
        (recv, sig_id)
    };

    match builder.spawn() {
        Ok(child) => {
            set_nonblocking(master_fd)?;
            Ok(Pty { child, file: File::from(master), signals, sig_id })
        },
        Err(err) => Err(Error::new(
            err.kind(),
            format!("Failed to spawn command '{}': {}", builder.get_program().to_string_lossy(), err),
        )),
    }
}



impl Drop for Pty {
    fn drop(&mut self) {
        // Convert child PID to nix's Pid type
        let pid = Pid::from_raw(self.child.id() as i32);

        // Send SIGHUP to child process
        let _ = signal::kill(pid, Signal::SIGHUP);

        // Clean up signal handler
        unregister_signal(self.sig_id);

        // Wait for child to exit
        let _ = self.child.wait();
    }
}

// Rest of EventedReadWrite, EventedPty, and OnResize implementations remain similar
// but use nix where appropriate for Winsize and ioctl calls...

impl OnResize for Pty {
    fn on_resize(&mut self, window_size: WindowSize) {
        let win = window_size.to_winsize();
        unsafe { tiocswinsz(self.file.as_raw_fd(), &win) }
            .unwrap_or_else(|e| die!("ioctl TIOCSWINSZ failed: {}", e));
    }
}
/// Winsize conversion
pub trait ToWinsize {
    fn to_winsize(self) -> Winsize;
}

impl ToWinsize for WindowSize {
    fn to_winsize(self) -> Winsize {
        Winsize {
            ws_row: self.num_lines as u16,
            ws_col: self.num_cols as u16,
            ws_xpixel: (self.num_cols * self.cell_width) as u16,
            ws_ypixel: (self.num_lines * self.cell_height) as u16,
        }
    }
}

/// Set non-blocking
fn set_nonblocking(fd: RawFd) -> Result<()> {
    let flags = fcntl::fcntl(fd, FcntlArg::F_GETFL)?;
    let new_flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
    fcntl::fcntl(fd, FcntlArg::F_SETFL(new_flags))?;
    Ok(())
}

