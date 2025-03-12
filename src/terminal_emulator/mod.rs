use nix::{errno::Errno, unistd::ForkResult};
use std::{
    ffi::CStr,
    ops::Range,
    os::fd::{AsRawFd, OwnedFd},
};
use std::os::fd::FromRawFd;
use ansi::{AnsiParser, SelectGraphicRendition, TerminalOutput};

mod ansi;

/// Spawn a shell in a child process and return the file descriptor used for I/O
fn spawn_shell() -> OwnedFd {
    unsafe {
        let res = nix::pty::forkpty(None, None).unwrap();
        match res.fork_result {
            ForkResult::Parent { .. } => (),
            ForkResult::Child => {
                let shell_name = CStr::from_bytes_with_nul(b"bash\0")
                    .expect("Should always have null terminator");
                let args: &[&[u8]] = &[b"bash\0",];

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
        OwnedFd::from_raw_fd(res.master)
    }
}

fn update_cursor(incoming: &[u8], cursor: &mut CursorState) {
    for c in incoming {
        match c {
            b'\n' => {
                cursor.x = 0;
                cursor.y += 1;
            }
            _ => {
                cursor.x += 1;
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
        .take(cursor_pos.y)
        .fold(0, |acc, item| acc + item.len() + 1);
    line_start + cursor_pos.x
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

fn delete_items_from_vec<T>(mut to_delete: Vec<usize>, vec: &mut Vec<T>) {
    to_delete.sort();
    for idx in to_delete.iter().rev() {
        vec.remove(*idx);
    }
}

struct ColorRangeAdjustment {
    // If a range adjustment results in a 0 width element we need to delete it
    should_delete: bool,
    // If a range was split we need to insert a new one
    to_insert: Option<FormatTag>,
}

/// if a and b overlap like
/// a:  [         ]
/// b:      [  ]
fn range_fully_conatins(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start <= b.start && a.end >= b.end
}

/// if a and b overlap like
/// a:     [      ]
/// b:  [     ]
fn range_starts_overlapping(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start > b.start && a.end > b.end
}

/// if a and b overlap like
/// a: [      ]
/// b:    [      ]
fn range_ends_overlapping(a: &Range<usize>, b: &Range<usize>) -> bool {
    range_starts_overlapping(b, a)
}

fn adjust_existing_format_range(
    existing_elem: &mut FormatTag,
    range: &Range<usize>,
) -> ColorRangeAdjustment {
    let mut ret = ColorRangeAdjustment {
        should_delete: false,
        to_insert: None,
    };

    let existing_range = existing_elem.start..existing_elem.end;
    if range_fully_conatins(range, &existing_range) {
        ret.should_delete = true;
    } else if range_fully_conatins(&existing_range, range) {
        if existing_elem.start == range.start {
            ret.should_delete = true;
        }

        if range.end != existing_elem.end {
            ret.to_insert = Some(FormatTag {
                start: range.end,
                end: existing_elem.end,
                color: existing_elem.color,
                bold: existing_elem.bold,
            });
        }

        existing_elem.end = range.start;
    } else if range_starts_overlapping(range, &existing_range) {
        existing_elem.end = range.start;
        if existing_elem.start == existing_elem.end {
            ret.should_delete = true;
        }
    } else if range_ends_overlapping(range, &existing_range) {
        existing_elem.start = range.end;
        if existing_elem.start == existing_elem.end {
            ret.should_delete = true;
        }
    } else {
        panic!(
            "Unhandled case {}-{}, {}-{}",
            existing_elem.start, existing_elem.end, range.start, range.end
        );
    }

    ret
}

fn adjust_existing_format_ranges(existing: &mut Vec<FormatTag>, range: &Range<usize>) {
    let mut effected_infos = existing
        .iter_mut()
        .enumerate()
        .filter(|(_i, item)| ranges_overlap(item.start..item.end, range.clone()))
        .collect::<Vec<_>>();

    let mut to_delete = Vec::new();
    let mut to_push = Vec::new();
    for info in &mut effected_infos {
        let adjustment = adjust_existing_format_range(info.1, range);
        if adjustment.should_delete {
            to_delete.push(info.0);
        }
        if let Some(item) = adjustment.to_insert {
            to_push.push(item);
        }
    }

    delete_items_from_vec(to_delete, existing);
    existing.extend(to_push);
}

#[derive(Clone)]
pub struct CursorState {
    pub x: usize,
    pub y: usize,
    pub(crate) bold: bool,
    pub(crate) color: TerminalColor,

}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalColor {
    Default,
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
    BackgroundRgb(u8, u8, u8),
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
            SelectGraphicRendition::ForegroundBrightBlack => Some(TerminalColor::ForegroundBrightBlack),
            SelectGraphicRendition::ForegroundBrightRed => Some(TerminalColor::ForegroundBrightRed),
            SelectGraphicRendition::ForegroundBrightGreen => Some(TerminalColor::ForegroundBrightGreen),
            SelectGraphicRendition::ForegroundBrightYellow => Some(TerminalColor::ForegroundBrightYellow),
            SelectGraphicRendition::ForegroundBrightBlue => Some(TerminalColor::ForegroundBrightBlue),
            SelectGraphicRendition::ForegroundBrightMagenta => Some(TerminalColor::ForegroundBrightMagenta),
            SelectGraphicRendition::ForegroundBrightCyan => Some(TerminalColor::ForegroundBrightCyan),
            SelectGraphicRendition::ForegroundBrightWhite => Some(TerminalColor::ForegroundBrightWhite),
            SelectGraphicRendition::ForegroundTrueColor(r, g, b) => Some(TerminalColor::ForegroundRgb(r, g, b)),
            SelectGraphicRendition::BackgroundBlack => Some(TerminalColor::BackgroundBlack),
            SelectGraphicRendition::BackgroundRed => Some(TerminalColor::BackgroundRed),
            SelectGraphicRendition::BackgroundGreen => Some(TerminalColor::BackgroundGreen),
            SelectGraphicRendition::BackgroundYellow => Some(TerminalColor::BackgroundYellow),
            SelectGraphicRendition::BackgroundBlue => Some(TerminalColor::BackgroundBlue),
            SelectGraphicRendition::BackgroundMagenta => Some(TerminalColor::BackgroundMagenta),
            SelectGraphicRendition::BackgroundCyan => Some(TerminalColor::BackgroundCyan),
            SelectGraphicRendition::BackgroundWhite => Some(TerminalColor::BackgroundWhite),
            SelectGraphicRendition::BackgroundBrightBlack => Some(TerminalColor::BackgroundBrightBlack),
            SelectGraphicRendition::BackgroundBrightRed => Some(TerminalColor::BackgroundBrightRed),
            SelectGraphicRendition::BackgroundBrightGreen => Some(TerminalColor::BackgroundBrightGreen),
            SelectGraphicRendition::BackgroundBrightYellow => Some(TerminalColor::BackgroundBrightYellow),
            SelectGraphicRendition::BackgroundBrightBlue => Some(TerminalColor::BackgroundBrightBlue),
            SelectGraphicRendition::BackgroundBrightMagenta => Some(TerminalColor::BackgroundBrightMagenta),
            SelectGraphicRendition::BackgroundBrightCyan => Some(TerminalColor::BackgroundBrightCyan),
            SelectGraphicRendition::BackgroundBrightWhite => Some(TerminalColor::BackgroundBrightWhite),
            SelectGraphicRendition::BackgroundTrueColor(r, g, b) => Some(TerminalColor::BackgroundRgb(r, g, b)),
            _ => None,
        }
    }


    fn index_to_rgb(&self, index: u32) -> (u8, u8, u8) {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatTag {
    pub start: usize,
    pub end: usize,
    pub color: TerminalColor,
    pub bold: bool,
}

struct FormatTracker {
    color_info: Vec<FormatTag>,
}

impl FormatTracker {
    fn new() -> FormatTracker {
        FormatTracker {
            color_info: vec![FormatTag {
                start: 0,
                end: usize::MAX,
                color: TerminalColor::Default,
                bold: false,
            }],
        }
    }

    fn push_range(&mut self, cursor: &CursorState, range: Range<usize>) {
        adjust_existing_format_ranges(&mut self.color_info, &range);

        self.color_info.push(FormatTag {
            start: range.start,
            end: range.end,
            color: cursor.color,
            bold: cursor.bold,
        });

        // FIXME: Insertion sort
        // FIXME: Merge adjacent
        self.color_info.sort_by(|a, b| a.start.cmp(&b.start));
    }

    fn tags(&self) -> Vec<FormatTag> {
        self.color_info.clone()
    }
}

pub struct TerminalEmulator {
    output_buf: AnsiParser,
    buf: Vec<u8>,
    color_tracker: FormatTracker,
    cursor_pos: CursorState,
    fd: OwnedFd,
}

impl TerminalEmulator {
    pub fn new() -> TerminalEmulator {
        let fd = spawn_shell();
        set_nonblock(&fd);

        TerminalEmulator {
            output_buf: AnsiParser::new(),
            buf: Vec::new(),
            color_tracker: FormatTracker::new(),
            cursor_pos: CursorState {
                x: 0,
                y: 0,
                bold: false,
                color: TerminalColor::Default,
            },
            fd,
        }
    }

    pub fn write(&mut self, mut to_write: &[u8]) {
        while !to_write.is_empty() {
            let written = nix::unistd::write(self.fd.as_raw_fd(), to_write).unwrap();
            to_write = &to_write[written..];
        }
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
                let parsed = self.output_buf.push(incoming);
                for segment in parsed {
                    match segment {
                        TerminalOutput::Data(data) => {
                            let output_start = cursor_to_buffer_position(&self.cursor_pos, &self.buf);
                            insert_data_at_position(&data, output_start, &mut self.buf);
                            self.color_tracker
                                .push_range(&self.cursor_pos, output_start..output_start + data.len());
                            update_cursor(&data, &mut self.cursor_pos);
                        }
                        TerminalOutput::SetCursorPos { x, y } => {
                            if let Some(x) = x {
                                self.cursor_pos.x = x - 1;
                            }
                            if let Some(y) = y {
                                self.cursor_pos.y = y - 1;
                            }
                        }
                        TerminalOutput::ClearForwards => {
                            let buf_pos = cursor_to_buffer_position(&self.cursor_pos, &self.buf);
                            self.color_tracker
                                .push_range(&self.cursor_pos, buf_pos..usize::MAX);
                            self.buf = self.buf[..buf_pos].to_vec();
                        }
                        TerminalOutput::ClearBackwards => {
                            let buf_pos = cursor_to_buffer_position(&self.cursor_pos, &self.buf);
                            self.buf = self.buf[buf_pos..].to_vec();
                        }
                        TerminalOutput::ClearAll => {
                            self.color_tracker
                                .push_range(&self.cursor_pos, 0..usize::MAX);
                            self.buf.clear();
                        }
                        TerminalOutput::Sgr(sgr) => {
                            if let Some(color) = TerminalColor::from_sgr(sgr) {
                                self.cursor_pos.color = color;
                            } else if sgr == SelectGraphicRendition::Reset {
                                self.cursor_pos.color = TerminalColor::Default;
                                self.cursor_pos.bold = false;
                            } else if sgr == SelectGraphicRendition::Bold {
                                self.cursor_pos.bold = true;
                            } else {
                                println!("Unhandled sgr: {:?}", sgr);
                            }
                        }
                        TerminalOutput::Invalid => {}
                    }
                }
            }

            if let Err(e) = ret {
                if e != Errno::EAGAIN {
                    println!("Failed to read: {e}");
                }
            }
        }


    pub fn data(&self) -> &[u8] {
        &self.buf
    }

    pub fn format_data(&self) -> Vec<FormatTag> {
        self.color_tracker.tags()
    }

    pub fn cursor_pos(&self) -> CursorState {
        self.cursor_pos.clone()
    }
}
