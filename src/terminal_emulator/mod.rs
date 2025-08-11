use crate::terminal_emulator::format_tracker::{FormatTag, FormatTracker};
use ansi::{AnsiParser, SelectGraphicRendition, TerminalOutput};
use buffer::TerminalBuffer;
use format_tracker::ColorRangeAdjustment;
use nix::{errno::Errno, ioctl_write_ptr_bad, unistd::ForkResult};
use std::os::fd::FromRawFd;
use std::{
    ffi::CStr,
    fmt,
    ops::Range,
    os::fd::{AsRawFd, OwnedFd},
};

mod ansi;
mod buffer;
pub(crate) mod format_tracker;

pub const TERMINAL_WIDTH: u16 = 80;
pub const TERMINAL_HEIGHT: u16 = 24;

#[derive(Eq, PartialEq)]
enum Mode {
    // Cursor keys mode
    // https://vt100.net/docs/vt100-ug/chapter3.html
    Decckm,
    // DEC Auto Wrap Mode
    Decawm,
    // DEC Text Cursor Enable Mode
    Dectcem,
    Unknown(Vec<u8>),
}

impl fmt::Debug for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Decckm => f.write_str("Decckm"),
            Mode::Decawm => f.write_str("Decawm"),
            Mode::Dectcem => f.write_str("Dectcem"),
            Mode::Unknown(params) => {
                let params_s = std::str::from_utf8(params)
                    .expect("parameter parsing should not allow non-utf8 characters here");
                f.write_fmt(format_args!("Unknown({})", params_s))
            }
        }
    }
}

fn char_to_ctrl_code(c: u8) -> u8 {
    // https://catern.com/posts/terminal_quirks.html
    // man ascii
    c & 0b0001_1111
}

#[derive(Eq, PartialEq, Debug)]
enum TerminalInputPayload {
    Single(u8),
    Many(&'static [u8]),
}

pub enum TerminalInput {
    // Normal keypress
    Ascii(u8),
    // Normal keypress with ctrl
    Ctrl(u8),
    Enter,
    Backspace,
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    ArrowDown,
    Home,
    End,
}

impl TerminalInput {
    fn to_payload(&self, decckm_mode: bool) -> TerminalInputPayload {
        match self {
            TerminalInput::Ascii(c) => TerminalInputPayload::Single(*c),
            TerminalInput::Ctrl(c) => TerminalInputPayload::Single(char_to_ctrl_code(*c)),
            TerminalInput::Enter => TerminalInputPayload::Single(b'\n'),
            // Hard to tie back, but check default VERASE in terminfo definition
            TerminalInput::Backspace => TerminalInputPayload::Single(0x7f),
            // https://vt100.net/docs/vt100-ug/chapter3.html
            // Table 3-6
            TerminalInput::ArrowRight => match decckm_mode {
                true => TerminalInputPayload::Many(b"\x1bOC"),
                false => TerminalInputPayload::Many(b"\x1b[C"),
            },
            TerminalInput::ArrowLeft => match decckm_mode {
                true => TerminalInputPayload::Many(b"\x1bOD"),
                false => TerminalInputPayload::Many(b"\x1b[D"),
            },
            TerminalInput::ArrowUp => match decckm_mode {
                true => TerminalInputPayload::Many(b"\x1bOA"),
                false => TerminalInputPayload::Many(b"\x1b[A"),
            },
            TerminalInput::ArrowDown => match decckm_mode {
                true => TerminalInputPayload::Many(b"\x1bOB"),
                false => TerminalInputPayload::Many(b"\x1b[B"),
            },
            TerminalInput::Home => match decckm_mode {
                true => TerminalInputPayload::Many(b"\x1bOH"),
                false => TerminalInputPayload::Many(b"\x1b[H"),
            },
            TerminalInput::End => match decckm_mode {
                true => TerminalInputPayload::Many(b"\x1bOF"),
                false => TerminalInputPayload::Many(b"\x1b[F"),
            },
        }
    }
}

/// Spawn a shell in a child process and return the file descriptor used for I/O
fn spawn_shell() -> OwnedFd {
    unsafe {
        let res = nix::pty::forkpty(None, None).unwrap();
        match res.fork_result {
            ForkResult::Parent { .. } => (),
            ForkResult::Child => {
                let shell_name = CStr::from_bytes_with_nul(b"bash\0")
                    .expect("Should always have null terminator");
                let args: &[&[u8]] = &[b"bash\0"];

                let args: Vec<&'static CStr> = args
                    .iter()
                    .map(|v| {
                        CStr::from_bytes_with_nul(v).expect("Should always have null terminator")
                    })
                    .collect::<Vec<_>>();

                // Temporary workaround to avoid rendering issues
                std::env::remove_var("PROMPT_COMMAND");
                std::env::set_var("PS1", "$ ");
                nix::unistd::execvp(shell_name, &args).unwrap();
                // Should never run
                std::process::exit(1);
            }
        }
        res.master
    }
}

fn update_cursor(incoming: &[u8], cursor: &mut CursorState) {
    for c in incoming {
        match c {
            b'\n' => {
                cursor.pos.x = 0;
                cursor.pos.y += 1;
            }
            _ => {
                cursor.pos.x += 1;
            }
        }
    }
}

fn set_nonblock(fd: &OwnedFd) {
    let flags = nix::fcntl::fcntl(fd.as_raw_fd(), nix::fcntl::FcntlArg::F_GETFL).unwrap();
    let mut flags =
        nix::fcntl::OFlag::from_bits(flags & nix::fcntl::OFlag::O_ACCMODE.bits()).unwrap();
    flags.set(nix::fcntl::OFlag::O_NONBLOCK, true);

    nix::fcntl::fcntl(fd.as_raw_fd(), nix::fcntl::FcntlArg::F_SETFL(flags)).unwrap();
}

pub fn cursor_to_buffer_position(cursor_pos: &CursorState, buf: &[u8]) -> usize {
    let line_start = buf
        .split(|b| *b == b'\n')
        .take(cursor_pos.pos.y)
        .fold(0, |acc, item| acc + item.len() + 1);
    line_start + cursor_pos.pos.x
}

/// Inserts data at position in buf, extending if necessary
fn insert_data_at_position(data: &[u8], pos: usize, buf: &mut Vec<u8>) {
    assert!(
        pos <= buf.len(),
        "assume pos is never more than 1 past the end of the buffer"
    );

    if pos >= buf.len() {
        assert_eq!(pos, buf.len());
        buf.extend_from_slice(data);
        return;
    }

    let amount_that_fits = buf.len() - pos;
    let (data_to_copy, data_to_push): (&[u8], &[u8]) = if amount_that_fits > data.len() {
        (&data, &[])
    } else {
        data.split_at(amount_that_fits)
    };

    buf[pos..pos + data_to_copy.len()].copy_from_slice(data_to_copy);
    buf.extend_from_slice(data_to_push);
}

fn split_format_data_for_scrollback(
    tags: Vec<FormatTag>,
    scrollback_split: usize,
) -> TerminalData<Vec<FormatTag>> {
    let scrollback_tags = tags
        .iter()
        .filter(|tag| tag.start < scrollback_split)
        .cloned()
        .map(|mut tag| {
            tag.end = tag.end.min(scrollback_split);
            tag
        })
        .collect();

    let canvas_tags = tags
        .into_iter()
        .filter(|tag| tag.end > scrollback_split)
        .map(|mut tag| {
            tag.start = tag.start.saturating_sub(scrollback_split);
            if tag.end != usize::MAX {
                tag.end -= scrollback_split;
            }
            tag
        })
        .collect();

    TerminalData {
        scrollback: scrollback_tags,
        visible: canvas_tags,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CursorPos {
    pub x: usize,
    pub y: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlinkMode {
    NoBlink,
    SlowBlink,
    RapidBlink,
}
#[derive(Clone)]
pub struct CursorState {
    pos: CursorPos,
    pub(crate) blink_mode: BlinkMode,
    pub(crate) visible: bool,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub foreground_color: TerminalColor,
    pub background_color: TerminalColor,
}
impl Default for CursorState {
    fn default() -> Self {
        CursorState {
            pos: CursorPos { x: 0, y: 0 },
            foreground_color: TerminalColor::Default,
            background_color: TerminalColor::Default,
            bold: false,
            italic: false,
            blink_mode: BlinkMode::NoBlink,

            visible: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalColor {
    Default,
    Faint,           // 2
    Italic,          // 3
    Underline,       // 4
    BlinkSlow,       // 5
    BlinkRapid,      // 6
    Reverse,         // 7
    Conceal,         // 8
    Reveal,          // 28 (companion to 8)
    NotItalic,       // 23
    NotUnderline,    // 24
    NormalIntensity, // 22
    ForegroundBlack,
    ForegroundRed,
    ForegroundGreen,
    ForegroundYellow,
    ForegroundBlue,
    ForegroundMagenta,
    ForegroundCyan,
    ForegroundWhite,
    ForegroundBrightBlack,
    ForegroundBrightRed,
    ForegroundBrightGreen,
    ForegroundBrightYellow,
    ForegroundBrightBlue,
    ForegroundBrightMagenta,
    ForegroundBrightCyan,
    ForegroundBrightWhite,
    ForegroundRgb(u8, u8, u8),
    BackgroundBlack,
    BackgroundRed,
    BackgroundGreen,
    BackgroundYellow,
    BackgroundBlue,
    BackgroundMagenta,
    BackgroundCyan,
    BackgroundWhite,
    BackgroundBrightBlack,
    BackgroundBrightRed,
    BackgroundBrightGreen,
    BackgroundBrightYellow,
    BackgroundBrightBlue,
    BackgroundBrightMagenta,
    BackgroundBrightCyan,
    BackgroundBrightWhite,
    BackgroundTrueColor(u8, u8, u8),
    Foreground8Bit(u8),
    Background8Bit(u8),
}

impl TerminalColor {
    fn from_sgr(sgr: SelectGraphicRendition) -> Option<TerminalColor> {
        match sgr {
            SelectGraphicRendition::ForegroundBlack => Some(TerminalColor::ForegroundBlack),
            SelectGraphicRendition::ForegroundRed => Some(TerminalColor::ForegroundRed),
            SelectGraphicRendition::ForegroundGreen => Some(TerminalColor::ForegroundGreen),
            SelectGraphicRendition::ForegroundYellow => Some(TerminalColor::ForegroundYellow),
            SelectGraphicRendition::ForegroundBlue => Some(TerminalColor::ForegroundBlue),
            SelectGraphicRendition::ForegroundMagenta => Some(TerminalColor::ForegroundMagenta),
            SelectGraphicRendition::ForegroundCyan => Some(TerminalColor::ForegroundCyan),
            SelectGraphicRendition::ForegroundWhite => Some(TerminalColor::ForegroundWhite),
            SelectGraphicRendition::ForegroundBrightBlack => {
                Some(TerminalColor::ForegroundBrightBlack)
            }
            SelectGraphicRendition::ForegroundBrightRed => Some(TerminalColor::ForegroundBrightRed),
            SelectGraphicRendition::ForegroundBrightGreen => {
                Some(TerminalColor::ForegroundBrightGreen)
            }
            SelectGraphicRendition::ForegroundBrightYellow => {
                Some(TerminalColor::ForegroundBrightYellow)
            }
            SelectGraphicRendition::ForegroundBrightBlue => {
                Some(TerminalColor::ForegroundBrightBlue)
            }
            SelectGraphicRendition::ForegroundBrightMagenta => {
                Some(TerminalColor::ForegroundBrightMagenta)
            }
            SelectGraphicRendition::ForegroundBrightCyan => {
                Some(TerminalColor::ForegroundBrightCyan)
            }
            SelectGraphicRendition::ForegroundBrightWhite => {
                Some(TerminalColor::ForegroundBrightWhite)
            }
            SelectGraphicRendition::ForegroundTrueColor(r, g, b) => {
                Some(TerminalColor::ForegroundRgb(r, g, b))
            }
            SelectGraphicRendition::BackgroundBlack => Some(TerminalColor::BackgroundBlack),
            SelectGraphicRendition::BackgroundRed => Some(TerminalColor::BackgroundRed),
            SelectGraphicRendition::BackgroundGreen => Some(TerminalColor::BackgroundGreen),
            SelectGraphicRendition::BackgroundYellow => Some(TerminalColor::BackgroundYellow),
            SelectGraphicRendition::BackgroundBlue => Some(TerminalColor::BackgroundBlue),
            SelectGraphicRendition::BackgroundMagenta => Some(TerminalColor::BackgroundMagenta),
            SelectGraphicRendition::BackgroundCyan => Some(TerminalColor::BackgroundCyan),
            SelectGraphicRendition::BackgroundWhite => Some(TerminalColor::BackgroundWhite),
            SelectGraphicRendition::BackgroundBrightBlack => {
                Some(TerminalColor::BackgroundBrightBlack)
            }
            SelectGraphicRendition::BackgroundBrightRed => Some(TerminalColor::BackgroundBrightRed),
            SelectGraphicRendition::BackgroundBrightGreen => {
                Some(TerminalColor::BackgroundBrightGreen)
            }
            SelectGraphicRendition::BackgroundBrightYellow => {
                Some(TerminalColor::BackgroundBrightYellow)
            }
            SelectGraphicRendition::BackgroundBrightBlue => {
                Some(TerminalColor::BackgroundBrightBlue)
            }
            SelectGraphicRendition::BackgroundBrightMagenta => {
                Some(TerminalColor::BackgroundBrightMagenta)
            }
            SelectGraphicRendition::BackgroundBrightCyan => {
                Some(TerminalColor::BackgroundBrightCyan)
            }
            SelectGraphicRendition::BackgroundBrightWhite => {
                Some(TerminalColor::BackgroundBrightWhite)
            }
            SelectGraphicRendition::BackgroundTrueColor(r, g, b) => {
                Some(TerminalColor::BackgroundTrueColor(r, g, b))
            }
            SelectGraphicRendition::Foreground8Bit(n) => Some(TerminalColor::Foreground8Bit(n)),
            SelectGraphicRendition::Background8Bit(n) => Some(TerminalColor::Background8Bit(n)),
            SelectGraphicRendition::BlinkSlow => Some(TerminalColor::BlinkSlow),
            SelectGraphicRendition::BlinkRapid => Some(TerminalColor::BlinkRapid),
            _ => None,
        }
    }

    pub fn index_to_rgb(&self, index: u32) -> (u8, u8, u8) {
        if index >= 16 && index <= 231 {
            // Convert index to RGB in the 6x6x6 color cube
            let index = index - 16;
            let r = ((index / 36) % 6) * 51;
            let g = ((index / 6) % 6) * 51;
            let b = (index % 6) * 51;
            (r as u8, g as u8, b as u8)
        } else if index >= 232 && index <= 255 {
            // Grayscale range
            let gray = 8 + (index - 232) * 10;
            (gray as u8, gray as u8, gray as u8)
        } else {
            // Default to white for invalid indices
            (255, 255, 255)
        }
    }
}

fn ranges_overlap(a: Range<usize>, b: Range<usize>) -> bool {
    if a.end <= b.start {
        return false;
    }

    if a.start >= b.end {
        return false;
    }

    true
}

ioctl_write_ptr_bad!(set_window_size, nix::libc::TIOCSWINSZ, nix::pty::Winsize);

pub struct TerminalData<T> {
    pub scrollback: T,
    pub visible: T,
}
pub struct TerminalEmulator {
    output_buf: AnsiParser,
    buf: TerminalBuffer,
    decckm_mode: bool,
    format_tracker: FormatTracker,
    pub(crate) cursor_state: CursorState,
    fd: OwnedFd,
}

impl TerminalEmulator {
    pub fn new() -> TerminalEmulator {
        let fd = spawn_shell();
        set_nonblock(&fd);
        let win_size = nix::pty::Winsize {
            ws_row: TERMINAL_HEIGHT,
            ws_col: TERMINAL_WIDTH,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        unsafe {
            set_window_size(fd.as_raw_fd(), &win_size).unwrap();
        }

        TerminalEmulator {
            output_buf: AnsiParser::new(),
            buf: TerminalBuffer::new(TERMINAL_WIDTH as usize, TERMINAL_HEIGHT as usize),
            format_tracker: FormatTracker::new(),
            cursor_state: CursorState::default(),
            decckm_mode: false,
            fd,
        }
    }

    pub fn write(&mut self, to_write: TerminalInput) {
        match to_write.to_payload(self.decckm_mode) {
            TerminalInputPayload::Single(c) => {
                let mut written = 0;
                while written == 0 {
                    written = nix::unistd::write(self.fd.as_raw_fd(), &[c]).unwrap();
                }
            }
            TerminalInputPayload::Many(mut to_write) => {
                while !to_write.is_empty() {
                    let written = nix::unistd::write(self.fd.as_raw_fd(), to_write).unwrap();
                    to_write = &to_write[written..];
                }
            }
        };
    }

    pub fn read(&mut self) {
        let mut buf = vec![0u8; 4096];
        let mut ret = Ok(0);
        while ret.is_ok() {
            ret = nix::unistd::read(self.fd.as_raw_fd(), &mut buf);
            let Ok(read_size) = ret else {
                break;
            };

            let incoming = &buf[0..read_size];
            debug!("Incoming data: {:?}", std::str::from_utf8(incoming));
            let parsed = self.output_buf.push(incoming);
            for segment in parsed {
                match segment {
                    TerminalOutput::Data(data) => {
                        let response = self.buf.insert_data(&self.cursor_state.pos, &data);
                        self.format_tracker
                            .push_range_adjustment(response.insertion_range);
                        self.format_tracker
                            .push_range(&self.cursor_state, response.written_range);
                        self.cursor_state.pos = response.new_cursor_pos;
                    }
                    TerminalOutput::SetCursorVisibility(visible) => {
                        self.cursor_state.visible = visible;
                    }
                    TerminalOutput::SetCursorPos { x, y } => {
                        if let Some(x) = x {
                            self.cursor_state.pos.x = x - 1;
                        }
                        if let Some(y) = y {
                            self.cursor_state.pos.y = y - 1;
                        }
                    }
                    TerminalOutput::InsertLines(num_lines) => {
                        let response = self.buf.insert_lines(&self.cursor_state.pos, num_lines);
                        self.format_tracker.delete_range(response.deleted_range);
                        self.format_tracker
                            .push_range_adjustment(response.inserted_range);
                    }
                    TerminalOutput::ClearForwards => {
                        if let Some(buf_pos) = self.buf.clear_forwards(&self.cursor_state.pos) {
                            self.format_tracker
                                .push_range(&self.cursor_state, buf_pos..usize::MAX);
                        }
                    }
                    TerminalOutput::SetCursorPosRel { x, y } => {
                        if let Some(x) = x {
                            let x: i64 = x.into();
                            let current_x: i64 = self
                                .cursor_state
                                .pos
                                .x
                                .try_into()
                                .expect("x position larger than i64 can handle");
                            self.cursor_state.pos.x = (current_x + x).max(0) as usize;
                        }
                        if let Some(y) = y {
                            let y: i64 = y.into();
                            let current_y: i64 = self
                                .cursor_state
                                .pos
                                .y
                                .try_into()
                                .expect("y position larger than i64 can handle");
                            self.cursor_state.pos.y = (current_y + y).max(0) as usize;
                        }
                    }

                    TerminalOutput::CarriageReturn => {
                        self.cursor_state.pos.x = 0;
                    }
                    TerminalOutput::Newline => {
                        self.cursor_state.pos.y += 1;
                    }
                    TerminalOutput::Backspace => {
                        if self.cursor_state.pos.x >= 1 {
                            self.cursor_state.pos.x -= 1;
                        }
                    }
                    TerminalOutput::Delete(num_chars) => {
                        let deleted_buf_range =
                            self.buf.delete_forwards(&self.cursor_state.pos, num_chars);
                        if let Some(range) = deleted_buf_range {
                            self.format_tracker.delete_range(range);
                        }
                    }
                    TerminalOutput::InsertSpaces(num_spaces) => {
                        let response = self.buf.insert_spaces(&self.cursor_state.pos, num_spaces);

                        self.format_tracker
                            .push_range_adjustment(response.insertion_range);
                    }
                    TerminalOutput::ClearLineForwards => {
                        if let Some(range) = self.buf.clear_line_forwards(&self.cursor_state.pos) {
                            self.format_tracker.delete_range(range);
                        }
                    }

                    TerminalOutput::ClearAll => {
                        self.format_tracker
                            .push_range(&self.cursor_state, 0..usize::MAX);
                        self.buf.clear_all();
                    }
                    TerminalOutput::Sgr(sgr) => {
                        if let Some(color) = TerminalColor::from_sgr(sgr) {
                            // Handle foreground/background colors separately
                            match &color {
                                TerminalColor::ForegroundBlack
                                | TerminalColor::ForegroundRed
                                | TerminalColor::ForegroundGreen
                                | TerminalColor::ForegroundYellow
                                | TerminalColor::ForegroundBlue
                                | TerminalColor::ForegroundMagenta
                                | TerminalColor::ForegroundCyan
                                | TerminalColor::ForegroundWhite
                                | TerminalColor::ForegroundBrightBlack
                                | TerminalColor::ForegroundBrightRed
                                | TerminalColor::ForegroundBrightGreen
                                | TerminalColor::ForegroundBrightYellow
                                | TerminalColor::ForegroundBrightBlue
                                | TerminalColor::ForegroundBrightMagenta
                                | TerminalColor::ForegroundBrightCyan
                                | TerminalColor::ForegroundBrightWhite
                                | TerminalColor::ForegroundRgb(_, _, _)
                                | TerminalColor::Foreground8Bit(_) => {
                                    self.cursor_state.foreground_color = color;
                                }
                                TerminalColor::BackgroundBlack
                                | TerminalColor::BackgroundRed
                                | TerminalColor::BackgroundGreen
                                | TerminalColor::BackgroundYellow
                                | TerminalColor::BackgroundBlue
                                | TerminalColor::BackgroundMagenta
                                | TerminalColor::BackgroundCyan
                                | TerminalColor::BackgroundWhite
                                | TerminalColor::BackgroundBrightBlack
                                | TerminalColor::BackgroundBrightRed
                                | TerminalColor::BackgroundBrightGreen
                                | TerminalColor::BackgroundBrightYellow
                                | TerminalColor::BackgroundBrightBlue
                                | TerminalColor::BackgroundBrightMagenta
                                | TerminalColor::BackgroundBrightCyan
                                | TerminalColor::BackgroundBrightWhite
                                | TerminalColor::BackgroundTrueColor(_, _, _)
                                | TerminalColor::Background8Bit(_) => {
                                    self.cursor_state.background_color = color;
                                }
                                // Set foreground color
                                _ => {
                                    self.cursor_state.foreground_color = color;
                                }

                                _ => {
                                    // Handle other attributes
                                    if sgr == SelectGraphicRendition::Reset {
                                        self.cursor_state.foreground_color =
                                            self.cursor_state.foreground_color;
                                        self.cursor_state.background_color = TerminalColor::Default;
                                        self.cursor_state.bold = false;
                                        self.cursor_state.italic = false;
                                        self.cursor_state.blink_mode = BlinkMode::NoBlink;
                                    } else if sgr == SelectGraphicRendition::Bold {
                                        self.cursor_state.bold = true;
                                    } else if sgr == SelectGraphicRendition::Italic {
                                        self.cursor_state.italic = true;
                                    } else if sgr == SelectGraphicRendition::BlinkSlow {
                                        self.cursor_state.blink_mode = BlinkMode::SlowBlink;
                                    } else if sgr == SelectGraphicRendition::BlinkRapid {
                                        self.cursor_state.blink_mode = BlinkMode::RapidBlink;
                                    }
                                }
                            }
                        } else {
                            // Handle cases where from_sgr returns None
                            if sgr == SelectGraphicRendition::Reset {
                                self.cursor_state.foreground_color = TerminalColor::Default;
                                self.cursor_state.background_color = TerminalColor::Default;
                                self.cursor_state.bold = false;
                                self.cursor_state.italic = false;
                                self.cursor_state.blink_mode = BlinkMode::NoBlink;
                            } else if sgr == SelectGraphicRendition::Bold {
                                self.cursor_state.bold = true;
                            } else if sgr == SelectGraphicRendition::Italic {
                                self.cursor_state.italic = true;
                            } else if sgr == SelectGraphicRendition::BlinkSlow {
                                self.cursor_state.blink_mode = BlinkMode::SlowBlink;
                            } else if sgr == SelectGraphicRendition::BlinkRapid {
                                self.cursor_state.blink_mode = BlinkMode::RapidBlink;
                            } else {
                                warn!("Unhandled sgr: {:?}", sgr);
                            }
                        }
                    }
                    TerminalOutput::SetMode(mode) => match mode {
                        Mode::Decckm => {
                            self.decckm_mode = true;
                        }
                        Mode::Dectcem => {
                            self.cursor_state.visible = true;
                        }
                        Mode::Decawm => {
                            self.buf.set_auto_wrap(true);
                        }
                        _ => {
                            warn!("unhandled set mode: {mode:?}");
                        }
                    },
                    TerminalOutput::ResetMode(mode) => match mode {
                        Mode::Decckm => {
                            self.decckm_mode = false;
                        }
                        Mode::Dectcem => {
                            self.cursor_state.visible = false;
                        }
                        Mode::Decawm => {
                            self.buf.set_auto_wrap(false);
                        }
                        _ => {
                            warn!("unhandled reset mode: {mode:?}");
                        }
                    },
                    TerminalOutput::Invalid => {}
                }
            }
        }

        if let Err(e) = ret {
            if e != Errno::EAGAIN {
                error!("Failed to read: {e}");
            }
        }
    }

    pub fn data(&self) -> TerminalData<&[u8]> {
        self.buf.data()
    }

    pub fn format_data(&self) -> TerminalData<Vec<FormatTag>> {
        let offset = self.buf.data().scrollback.len();
        split_format_data_for_scrollback(self.format_tracker.tags(), offset)
    }
    pub fn cursor_pos(&self) -> CursorPos {
        self.cursor_state.pos.clone()
    }
    pub fn set_win_size(&mut self, width_chars: usize, height_chars: usize) {
        let response = self
            .buf
            .set_win_size(width_chars, height_chars, &self.cursor_state.pos);
        self.cursor_state.pos = response.new_cursor_pos;

        if response.changed {
            let win_size = nix::pty::Winsize {
                ws_row: height_chars as u16,
                ws_col: width_chars as u16,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };

            unsafe {
                set_window_size(self.fd.as_raw_fd(), &win_size).unwrap();
            }
        }
    }
}
