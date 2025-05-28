use nix::{errno::Errno, ioctl_write_ptr_bad, unistd::ForkResult};
use std::{
    ffi::CStr,
    ops::Range,
    os::fd::{AsRawFd, OwnedFd},
};
use std::os::fd::FromRawFd;
use ansi::{AnsiParser, SelectGraphicRendition, TerminalOutput};

mod ansi;

pub const TERMINAL_WIDTH: u16 = 80;
pub const TERMINAL_HEIGHT: u16 = 24;
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
/// Calculate the indexes of the start and end of each line in the buffer given an input width.
/// Ranges do not include newlines. If a newline appears past the width, it does not result in an
/// extra line
///
/// Example
/// ```
/// let ranges = calc_line_ranges(b"12\n1234\n12345", 4);
/// assert_eq!(ranges, [0..2, 3..7, 8..11, 12..13]);
/// ```
fn calc_line_ranges(buf: &[u8], width: usize) -> Vec<Range<usize>> {
    let mut ret = vec![];
    let mut bytes_since_newline = 0;

    let mut current_start = 0;

    for (i, c) in buf.iter().enumerate() {
        if *c == b'\n' {
            ret.push(current_start..i);
            current_start = i + 1;
            bytes_since_newline = 0;
            continue;
        }

        assert!(bytes_since_newline <= width);
        if bytes_since_newline == width {
            ret.push(current_start..i);
            current_start = i;
            bytes_since_newline = 0;
            continue;
        }

        bytes_since_newline += 1;
    }

    if buf.len() > current_start {
        ret.push(current_start..buf.len());
    }
    ret
}

fn buf_to_cursor_pos(buf: &[u8], width: usize, height: usize, buf_pos: usize) -> CursorPos {
    let new_line_ranges = calc_line_ranges(buf, width);
    let new_visible_line_ranges = line_ranges_to_visible_line_ranges(&new_line_ranges, height);
    let (new_cursor_y, new_cursor_line) = new_visible_line_ranges
        .iter()
        .enumerate()
        .find(|(_i, r)| r.end >= buf_pos)
        .unwrap();
    let new_cursor_x = buf_pos - new_cursor_line.start;
    CursorPos {
        x: new_cursor_x,
        y: new_cursor_y,
    }
}
fn line_ranges_to_visible_line_ranges(
    line_ranges: &[Range<usize>],
    height: usize,
) -> &[Range<usize>] {
    if line_ranges.is_empty() {
        return line_ranges;
    }
    let last_line_idx = line_ranges.len();
    let first_visible_line = last_line_idx.saturating_sub(height);
    &line_ranges[first_visible_line..]
}


fn unwrapped_line_end_pos(buf: &[u8], start_pos: usize) -> usize {
    buf.iter()
        .enumerate()
        .skip(start_pos)
        .find_map(|(i, c)| match *c {
            b'\n' => Some(i),
            _ => None,
        })
        .unwrap_or(buf.len())
}

fn pad_buffer_for_write(
    buf: &mut Vec<u8>,
    width: usize,
    cursor_pos: &CursorPos,
    height: usize,
    write_len: usize,
) -> usize {
    let mut visible_line_ranges = {
        // Calculate in block scope to avoid accidental usage of scrollback line ranges later
        let line_ranges = calc_line_ranges(buf, width);
        line_ranges_to_visible_line_ranges(&line_ranges, height).to_vec()
    };

    for _ in visible_line_ranges.len()..cursor_pos.y + 1 {
        buf.push(b'\n');
        let newline_pos = buf.len() - 1;
        visible_line_ranges.push(newline_pos..newline_pos);
    }

    let line_range = &visible_line_ranges[cursor_pos.y];

    let desired_start = line_range.start + cursor_pos.x;
    let desired_end = desired_start + write_len;

    // NOTE: We only want to pad if we hit an early newline. If we wrapped because we hit the edge
    // of the screen we can just keep writing and the wrapping will stay as is. This is an
    // important distinction because in the no-newline case we want to make sure we overwrite
    // whatever was in the buffer before
    let actual_end = unwrapped_line_end_pos(buf, line_range.start);

    let number_of_spaces = if desired_end > actual_end {
        desired_end - actual_end
    } else {
        0
    };

    for i in 0..number_of_spaces {
        buf.insert(actual_end + i, b' ');
    }

    desired_start
}


pub struct TerminalData<T> {
    pub scrollback: T,
    pub visible: T,
}
struct TerminalBufferInsertResponse {
    written_range: Range<usize>,
    new_cursor_pos: CursorPos,
}

struct TerminalBuffer {
    buf: Vec<u8>,
    width: usize,
    height: usize,
}

impl TerminalBuffer {
    fn new(width: usize, height: usize) -> TerminalBuffer {
        TerminalBuffer {
            buf: vec![],
            width,
            height,
        }
    }

    fn insert_data(&mut self, cursor_pos: &CursorPos, data: &[u8]) -> TerminalBufferInsertResponse {
        let write_idx = pad_buffer_for_write(
            &mut self.buf,
            self.width,
            cursor_pos,
            self.height,

            data.len(),
        );
        let write_range = write_idx..write_idx + data.len();
        self.buf[write_range.clone()].copy_from_slice(data);
        let new_cursor_pos = buf_to_cursor_pos(&self.buf, self.width, self.height, write_range.end);
        TerminalBufferInsertResponse {
            written_range: write_range,
            new_cursor_pos,
        }
    }

    fn clear_forwards(&mut self, cursor_pos: &CursorPos) -> Option<usize> {
        let line_ranges = calc_line_ranges(&self.buf, self.width);

        let line_range = line_ranges.get(cursor_pos.y)?;

        if cursor_pos.x > line_range.end {
            return None;
        }

        let clear_pos = line_range.start + cursor_pos.x;
        self.buf.truncate(clear_pos);
        Some(clear_pos)
    }
    fn append_newline_at_line_end(&mut self, pos: &CursorPos) {
        let line_ranges = calc_line_ranges(&self.buf, self.buf.len());
        let Some(line_range) = line_ranges.get(pos.y) else {
            return;
        };

        let newline_pos = self
            .buf
            .iter()
            .enumerate()
            .skip(line_range.start)
            .find(|(_i, b)| **b == b'\n')
            .map(|(i, _b)| i);

        if newline_pos.is_none() {
            self.buf.push(b'\n');
        }
    }

    fn clear_all(&mut self) {
        self.buf.clear();
    }

    fn data(&self) -> TerminalData<&[u8]> {
        let line_ranges = calc_line_ranges(&self.buf, self.width);
        let visible_line_ranges = line_ranges_to_visible_line_ranges(&line_ranges, self.height);
        if self.buf.is_empty() {
            return TerminalData {
                scrollback: &[],
                visible: &self.buf,
            };
        }
        let start = visible_line_ranges[0].start;
        TerminalData {
            scrollback: &self.buf[0..start],
            visible: &self.buf[start..],
        }
    }
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
                italic: existing_elem.italic,
                blink:existing_elem.blink,
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
pub fn buffer_index_to_cursor_pos(buf: &[u8], index: usize) -> (usize, usize) {
    let mut y = 0;
    let mut current = 0;
    for line in buf.split(|&b| b == b'\n') {
        let line_len = line.len();
        if current + line_len >= index {
            return (index - current, y);
        }
        current += line_len + 1; // +1 for the newline character
        y += 1;
    }
    (0, y)
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

#[derive(Debug, Clone)]
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
    pub(crate) color: TerminalColor,

}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalColor {
    Default,
    Faint,          // 2
    Italic,         // 3
    Underline,      // 4
    BlinkSlow,      // 5
    BlinkRapid,     // 6
    Reverse,        // 7
    Conceal,        // 8
    Reveal,         // 28 (companion to 8)
    NotItalic,      // 23
    NotUnderline,   // 24
    NormalIntensity,// 22
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
            SelectGraphicRendition::BackgroundTrueColor(r, g, b) => Some(TerminalColor::BackgroundTrueColor(r, g, b)),
            SelectGraphicRendition::Foreground8Bit(n) => {
                Some(TerminalColor::Foreground8Bit(n))
            },
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatTag {
    pub start: usize,
    pub end: usize,
    pub blink: bool,
    pub color: TerminalColor,
    pub bold: bool,
    pub italic: bool,
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
                italic: false,
                blink: false,
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
            italic: cursor.italic,
            blink: cursor.blink_mode != BlinkMode::NoBlink,
        });

        // FIXME: Insertion sort
        // FIXME: Merge adjacent
        self.color_info.sort_by(|a, b| a.start.cmp(&b.start));
    }

    fn tags(&self) -> Vec<FormatTag> {
        self.color_info.clone()
    }
}


ioctl_write_ptr_bad!(set_window_size, nix::libc::TIOCSWINSZ, nix::pty::Winsize);


pub struct TerminalEmulator {
    output_buf: AnsiParser,
    buf:TerminalBuffer,
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
            cursor_state: CursorState {
                pos: CursorPos { x: 0, y: 0 },
                visible: false,
                bold: false,
                color: TerminalColor::Default,
                italic: false,
                blink_mode: BlinkMode::NoBlink,
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
                            let response = self
                                .buf
                                .insert_data(&self.cursor_state.pos, &data);
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
                        TerminalOutput::ClearForwards => {
                            if let Some(buf_pos) =
                                self.buf.clear_forwards(&self.cursor_state.pos)
                            {
                                self.format_tracker
                                    .push_range(&self.cursor_state, buf_pos..usize::MAX);
                            }
                        }
                        
                        TerminalOutput::CarriageReturn => {
                            self.cursor_state.pos.x = 0;
                        }
                        TerminalOutput::Newline => {
                            self.buf
                                .append_newline_at_line_end(&self.cursor_state.pos);
                            self.cursor_state.pos.y += 1;
                        }
                        TerminalOutput::Backspace => {
                            if self.cursor_state.pos.x >= 1 {
                                self.cursor_state.pos.x -= 1;
                            }
                        }

                        TerminalOutput::ClearAll => {
                            self.format_tracker
                                .push_range(&self.cursor_state, 0..usize::MAX);
                            self.buf.clear_all();
                        }
                        TerminalOutput::Sgr(sgr) => {
                            if let Some(color) = TerminalColor::from_sgr(sgr) {
                                self.cursor_state.color = color;
                            } else if sgr == SelectGraphicRendition::Reset {
                                self.cursor_state.color = TerminalColor::Default;
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
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_color_tracker_test() {
        let mut format_tracker = FormatTracker::new();
        let mut cursor_state = CursorState {
            pos: CursorPos { x: 0, y: 0 },
            blink_mode: BlinkMode::NoBlink,
            color: TerminalColor::Default,
            bold: false,
            visible: false,
            italic: false,
        };

        cursor_state.color = TerminalColor::BackgroundYellow;
        format_tracker.push_range(&cursor_state, 3..10);
        let tags = format_tracker.tags();
        assert_eq!(
            tags,
            &[
                FormatTag {
                    start: 0,
                    end: 3,
                    blink: false,
                    color: TerminalColor::Default,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 3,
                    end: 10,
                    blink: false,
                    color: TerminalColor::BackgroundYellow,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 10,
                    end: usize::MAX,
                    blink: false,
                    color: TerminalColor::Default,
                    bold: false,
                    italic: false,
                },
            ]
        );

        cursor_state.color = TerminalColor::BackgroundBlue;
        format_tracker.push_range(&cursor_state, 5..7);
        let tags = format_tracker.tags();
        assert_eq!(
            tags,
            &[
                FormatTag {
                    start: 0,
                    end: 3,
                    blink: false,
                    color: TerminalColor::Default,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 3,
                    end: 5,
                    blink: false,
                    color: TerminalColor::BackgroundYellow,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 5,
                    end: 7,
                    blink: false,
                    color: TerminalColor::BackgroundBlue,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 7,
                    end: 10,
                    blink: false,
                    color: TerminalColor::BackgroundYellow,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 10,
                    end: usize::MAX,
                    blink: false,
                    color: TerminalColor::Default,
                    bold: false,
                    italic: false,
                },
            ]
        );

        cursor_state.color = TerminalColor::BackgroundGreen;
        format_tracker.push_range(&cursor_state, 7..9);
        let tags = format_tracker.tags();
        assert_eq!(
            tags,
            &[
                FormatTag {
                    start: 0,
                    end: 3,
                    blink: false,
                    color: TerminalColor::Default,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 3,
                    end: 5,
                    blink: false,
                    color: TerminalColor::BackgroundYellow,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 5,
                    end: 7,
                    blink: false,
                    color: TerminalColor::BackgroundBlue,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 7,
                    end: 9,
                    blink: false,
                    color: TerminalColor::BackgroundGreen,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 9,
                    end: 10,
                    blink: false,
                    color: TerminalColor::BackgroundYellow,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 10,
                    end: usize::MAX,
                    blink: false,
                    color: TerminalColor::Default,
                    bold: false,
                    italic: false,
                },
            ]
        );

        cursor_state.color = TerminalColor::BackgroundRed;
        cursor_state.bold = true;
        format_tracker.push_range(&cursor_state, 6..11);
        let tags = format_tracker.tags();
        assert_eq!(
            tags,
            &[
                FormatTag {
                    start: 0,
                    end: 3,
                    blink: false,
                    color: TerminalColor::Default,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 3,
                    end: 5,
                    blink: false,
                    color: TerminalColor::BackgroundYellow,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 5,
                    end: 6,
                    blink: false,
                    color: TerminalColor::BackgroundBlue,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 6,
                    end: 11,
                    blink: false,
                    color: TerminalColor::BackgroundRed,
                    bold: true,
                    italic: false,
                },
                FormatTag {
                    start: 11,
                    end: usize::MAX,
                    blink: false,
                    color: TerminalColor::Default,
                    bold: false,
                    italic: false,
                },
            ]
        );
    }

    #[test]
    fn test_range_overlap() {
        assert!(ranges_overlap(5..10, 7..9));
        assert!(ranges_overlap(5..10, 8..12));
        assert!(ranges_overlap(5..10, 3..6));
        assert!(ranges_overlap(5..10, 2..12));
        assert!(!ranges_overlap(5..10, 10..12));
        assert!(!ranges_overlap(5..10, 0..5));
    }

    #[test]
    fn test_calc_line_ranges() {
        let line_starts = calc_line_ranges(b"asdf\n0123456789\n012345678901", 10);
        assert_eq!(line_starts, &[0..4, 5..15, 16..26, 26..28]);
    }

    #[test]
    fn test_buffer_padding() {
        let mut buf = b"asdf\n1234\nzxyw".to_vec();

        let cursor_pos = CursorPos { x: 8, y: 0 };
        let copy_idx = pad_buffer_for_write(&mut buf, 10,  &cursor_pos,10, 10);
        assert_eq!(buf, b"asdf              \n1234\nzxyw");
        assert_eq!(copy_idx, 8);
    }

    #[test]
    fn test_canvas_clear_forwards() {
        let mut buffer = TerminalBuffer::new(5, 5);
        buffer.insert_data(&CursorPos { x: 0, y: 0 }, b"012\n3456789");
        buffer.clear_forwards(&CursorPos { x: 1, y: 1 });
        assert_eq!(buffer.data().visible, b"012\n3");
    }

    #[test]
    fn test_canvas_clear() {
        let mut buffer = TerminalBuffer::new(5, 5);
        buffer.insert_data(&CursorPos { x: 0, y: 0 }, b"0123456789");
        buffer.clear_all();
        assert_eq!(buffer.data().visible, &[]);
    }

    #[test]
    fn test_terminal_buffer_overwrite_early_newline() {
        let mut buffer = TerminalBuffer::new(5, 5);
        buffer.insert_data(&CursorPos { x: 0, y: 0 }, b"012\n3456789");
        assert_eq!(buffer.data().visible, b"012\n3456789\n");

        // Cursor pos should be calculated based off wrapping at column 5, but should not result in
        // an extra newline
        buffer.insert_data(&CursorPos { x: 2, y: 1 }, b"test");
        assert_eq!(buffer.data().visible, b"012\n34test9\n");
    }

    #[test]
    fn test_terminal_buffer_overwrite_no_newline() {
        let mut buffer = TerminalBuffer::new(5, 5);
        buffer.insert_data(&CursorPos { x: 0, y: 0 }, b"0123456789");
        assert_eq!(buffer.data().visible, b"0123456789\n");

        // Cursor pos should be calculated based off wrapping at column 5, but should not result in
        // an extra newline
        buffer.insert_data(&CursorPos { x: 2, y: 1 }, b"test");
        assert_eq!(buffer.data().visible, b"0123456test\n");
    }

    #[test]
    fn test_terminal_buffer_overwrite_late_newline() {
        // This should behave exactly as test_terminal_buffer_overwrite_no_newline(), except with a
        // neline between lines 1 and 2
        let mut buffer = TerminalBuffer::new(5, 5);
        buffer.insert_data(&CursorPos { x: 0, y: 0 }, b"01234\n56789");
        assert_eq!(buffer.data().visible, b"01234\n56789\n");

        buffer.insert_data(&CursorPos { x: 2, y: 1 }, b"test");
        assert_eq!(buffer.data().visible, b"01234\n56test\n");
    }

    #[test]
    fn test_terminal_buffer_insert_unallocated_data() {
        let mut buffer = TerminalBuffer::new(10, 10);
        buffer.insert_data(&CursorPos { x: 4, y: 5 }, b"hello world");
        assert_eq!(buffer.data().visible, b"\n\n\n\n\n    hello world\n");

        buffer.insert_data(&CursorPos { x: 3, y: 2 }, b"hello world");
        assert_eq!(
            buffer.data().visible,
            b"\n\n   hello world\n\n\n    hello world\n"
        );
    }

    #[test]
    fn test_canvas_newline_append() {
        let mut canvas = TerminalBuffer::new(10, 10);
        let mut cursor_pos = CursorPos { x: 0, y: 0 };
        canvas.insert_data(&cursor_pos, b"asdf\n1234\nzxyw");

        cursor_pos.x = 2;
        cursor_pos.y = 1;
        canvas.append_newline_at_line_end(&cursor_pos);
        assert_eq!(canvas.buf, b"asdf\n1234\nzxyw\n");

        canvas.clear_forwards(&cursor_pos);
        assert_eq!(canvas.buf, b"asdf\n12");

        cursor_pos.x = 0;
        cursor_pos.y = 1;
        canvas.append_newline_at_line_end(&cursor_pos);
        assert_eq!(canvas.buf, b"asdf\n12\n");

        cursor_pos.x = 0;
        cursor_pos.y = 2;
        canvas.insert_data(&cursor_pos, b"01234567890123456");

        cursor_pos.x = 4;
        cursor_pos.y = 3;
        canvas.clear_forwards(&cursor_pos);
        assert_eq!(canvas.buf, b"asdf\n12\n01234567890123");

        cursor_pos.x = 4;
        cursor_pos.y = 2;
        canvas.append_newline_at_line_end(&cursor_pos);
        assert_eq!(canvas.buf, b"asdf\n12\n01234567890123\n");
    }

    #[test]
    fn test_canvas_scrolling() {
        let mut canvas = TerminalBuffer::new(10, 3);
        let initial_cursor_pos = CursorPos { x: 0, y: 0 };

        fn crlf(pos: &mut CursorPos) {
            pos.y += 1;
            pos.x = 0;
        }

        // Simulate real terminal usage where newlines are injected with cursor moves
        let mut response = canvas.insert_data(&initial_cursor_pos, b"asdf");
        crlf(&mut response.new_cursor_pos);
        let mut response = canvas.insert_data(&response.new_cursor_pos, b"xyzw");
        crlf(&mut response.new_cursor_pos);
        let mut response = canvas.insert_data(&response.new_cursor_pos, b"1234");
        crlf(&mut response.new_cursor_pos);
        let mut response = canvas.insert_data(&response.new_cursor_pos, b"5678");
        crlf(&mut response.new_cursor_pos);

        assert_eq!(canvas.data().scrollback, b"asdf\n");
        assert_eq!(canvas.data().visible, b"xyzw\n1234\n5678\n");
    }

    #[test]
    fn test_format_tracker_scrollback_split() {
        let tags = vec![
            FormatTag {
                start: 0,
                end: 5,
                blink: false,
                color: TerminalColor::BackgroundBlue,
                bold: true,
                italic: false,
            },
            FormatTag {
                start: 5,
                end: 7,
                blink: false,
                color: TerminalColor::BackgroundRed,
                bold: false,
                italic: false,
            },
            FormatTag {
                start: 7,
                end: 10,
                blink: false,
                color: TerminalColor::BackgroundBlue,
                bold: true,
                italic: false,
            },
            FormatTag {
                start: 10,
                end: usize::MAX,
                blink: false,
                color: TerminalColor::BackgroundRed,
                bold: true,
                italic: false,
            },
        ];

        // Case 1: no split
        let res = split_format_data_for_scrollback(tags.clone(), 0);
        assert_eq!(res.scrollback, &[]);
        assert_eq!(res.visible, &tags[..]);

        // Case 2: Split on a boundary
        let res = split_format_data_for_scrollback(tags.clone(), 10);
        assert_eq!(res.scrollback, &tags[0..3]);
        assert_eq!(
            res.visible,
            &[FormatTag {
                start: 0,
                end: usize::MAX,
                blink: false,
                color: TerminalColor::BackgroundRed,
                bold: true,
                italic: false,
            },]
        );

        // Case 3: Split a segment
        let res = split_format_data_for_scrollback(tags.clone(), 9);
        assert_eq!(
            res.scrollback,
            &[
                FormatTag {
                    start: 0,
                    end: 5,
                    blink: false,
                    color: TerminalColor::BackgroundBlue,
                    bold: true,
                    italic: false,
                },
                FormatTag {
                    start: 5,
                    end: 7,
                    blink: false,
                    color: TerminalColor::BackgroundRed,
                    bold: false,
                    italic: false,
                },
                FormatTag {
                    start: 7,
                    end: 9,
                    blink: false,
                    color: TerminalColor::BackgroundBlue,
                    bold: true,
                    italic: false,
                },
            ]
        );
        assert_eq!(
            res.visible,
            &[
                FormatTag {
                    start: 0,
                    end: 1,
                    blink: false,
                    color: TerminalColor::BackgroundBlue,
                    bold: true,
                    italic: false,
                },
                FormatTag {
                    start: 1,
                    end: usize::MAX,
                    blink: false,
                    color: TerminalColor::BackgroundRed,
                    bold: true,
                    italic: false,
                },
            ]
        );
    }
}
