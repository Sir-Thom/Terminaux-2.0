use std::ffi::CString;
use nix::{
    unistd::{close, dup2, execvp, fork, setsid, ForkResult, Pid},
    pty::{openpty, Winsize},
    sys::wait::waitpid,
    libc::{ioctl, TIOCSCTTY, TIOCSWINSZ, winsize},
    sys::termios::{self, tcsetattr, SetArg},
    fcntl::{FcntlArg, OFlag}
};
use std::os::unix::io::{RawFd, FromRawFd};
use std::process::exit;
use std::io::{Read, Write, ErrorKind};
use std::sync::atomic::{AtomicI32, Ordering};
use crossterm::event::{read, Event, KeyCode};
use std::thread;
use std::sync::mpsc;

fn create_pty() -> (RawFd, RawFd) {
    let winsize = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let pty = openpty(Some(&winsize), None).expect("Failed to create PTY");
    (pty.master, pty.slave)
}

fn spawn_shell(master_fd: RawFd, slave_fd: RawFd) -> Result<Pid, String> {
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child, .. }) => Ok(child),
        Ok(ForkResult::Child) => {
            close(master_fd).expect("Failed to close master fd");

            setsid().expect("Failed to create new session");
            unsafe { ioctl(slave_fd, TIOCSCTTY as u64, 0) };

            dup2(slave_fd, 0).expect("Failed dup2 stdin");
            dup2(slave_fd, 1).expect("Failed dup2 stdout");
            dup2(slave_fd, 2).expect("Failed dup2 stderr");
            close(slave_fd).expect("Failed close slave fd");

            std::env::set_var("TERM", "xterm-256color");
            std::env::set_var("COLORTERM", "truecolor");
            let shell = CString::new("/bin/bash").unwrap();
            execvp(&shell, &[shell.clone()]).expect("Failed exec shell");
            exit(0);
        }
        Err(e) => Err(format!("Fork failed: {}", e)),
    }
}
use nix::sys::signal::{self, SigHandler, Signal};





fn handle_io(master_fd: RawFd) {
    // Set non-blocking mode safely
    let flags = nix::fcntl::fcntl(master_fd, FcntlArg::F_GETFL)
        .expect("Failed to get file status flags");

    nix::fcntl::fcntl(master_fd, FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK))
        .expect("Failed to set non-blocking mode");

    let (tx, rx) = mpsc::channel();
    let mut master = unsafe { std::fs::File::from_raw_fd(master_fd) };

    // Input thread
    thread::spawn(move || {
        loop {
            match read() {
                Ok(Event::Key(event)) => {
                    let bytes = match event.code {
                        KeyCode::Char(c) => vec![c as u8],
                        KeyCode::Enter => vec![b'\n'],
                        KeyCode::Backspace => vec![0x7f],
                        KeyCode::Left => vec![0x1b, b'[', b'D'],
                        KeyCode::Right => vec![0x1b, b'[', b'C'],
                        KeyCode::Up => vec![0x1b, b'[', b'A'],
                        KeyCode::Down => vec![0x1b, b'[', b'B'],
                        KeyCode::Tab => vec![b'\t'],
                        KeyCode::Esc => vec![0x1b],
                        _ => continue,
                    };
                    tx.send(bytes).expect("Failed to send input");
                }
                _ => {}
            }
        }
    });


    // Main I/O loop
    let mut buffer = [0; 4096];
    loop {
        // Handle output
        match master.read(&mut buffer) {
            Ok(n) if n > 0 => {
                let output = String::from_utf8_lossy(&buffer[..n]);
                // Handle terminal queries
                if output.contains("\x1b[6n") {  // CPR query
                    let pos = "\x1b[1;1R";  // Dummy cursor position
                    master.write_all(pos.as_bytes()).unwrap();
                } else {
                    print!("{}", output);
                    std::io::stdout().flush().unwrap();
                }
            }
            _ => {}
        }


        // Handle input
        if let Ok(bytes) = rx.try_recv() {
            master.write_all(&bytes).expect("Write failed");
        }

        // Prevent busy looping
        thread::sleep(std::time::Duration::from_millis(10));
    }
}


static MASTER_FD: AtomicI32 = AtomicI32::new(-1);

    fn handle_resize(master_fd: RawFd) {
        // Store the file descriptor in atomic storage
        MASTER_FD.store(master_fd, Ordering::Relaxed);

        unsafe {
            // Use a closure-like handler through raw pointer
            let handler = signal::SigHandler::Handler(resize_handler);
            signal::signal(signal::Signal::SIGWINCH, handler)
                .expect("Error setting SIGWINCH handler");
        }
    }

    // Actual signal handler function
    extern "C" fn resize_handler(_: i32) {
        let master_fd = MASTER_FD.load(Ordering::Relaxed);
        if master_fd == -1 {
            return;
        }

        // Get current window size
        let size = crossterm::terminal::size().unwrap();
        let winsize = winsize {
            ws_row: size.1 as u16,
            ws_col: size.0 as u16,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // Send new size to PTY
        unsafe {
            libc::ioctl(master_fd, TIOCSWINSZ, &winsize);
        }
    }

fn main() {
        let (master, slave) = create_pty();
        let child_pid = spawn_shell(master, slave).expect("Failed spawn shell");

        // Configure terminal settings
        let mut termios = termios::tcgetattr(master).expect("Failed get termios");
        termios::cfmakeraw(&mut termios);
        tcsetattr(master, SetArg::TCSANOW, &termios).expect("Failed set termios");

        handle_resize(master);
        handle_io(master);

        // Cleanup
        waitpid(child_pid, None).expect("Wait failed");
    }
