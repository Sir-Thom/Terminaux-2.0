use std::ops::Range;
use crate::terminal_emulator::CursorPos;

pub struct TerminalBufferSetWinSizeResponse {
    pub changed: bool,
    pub insertion_range: Range<usize>,
    pub new_cursor_pos: CursorPos,
}
struct PadBufferForWriteResponse {
    /// Where to copy data into
    write_idx: usize,
    /// Indexes where we added data
    inserted_padding: Range<usize>,
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

    let mut current_start = 0;

    for (i, c) in buf.iter().enumerate() {
        if *c == b'\n' {
            ret.push(current_start..i);
            current_start = i + 1;
            continue;
        }

        let bytes_since_start = i - current_start;
        assert!(bytes_since_start <= width);
        if bytes_since_start == width {
            ret.push(current_start..i);
            current_start = i;
            continue;
        }
    }

    if buf.len() > current_start {
        ret.push(current_start..buf.len());
    }
    ret
}

#[derive(Debug, Eq, PartialEq)]
struct InvalidBufPos {
    buf_pos: usize,
    buf_len: usize,
}
fn buf_to_cursor_pos(
    buf: &[u8],
    width: usize,
    height: usize,
    buf_pos: usize,
) -> Result<CursorPos, InvalidBufPos> {
    let new_line_ranges = calc_line_ranges(buf, width);
    let new_visible_line_ranges = line_ranges_to_visible_line_ranges(&new_line_ranges, height);
    let (new_cursor_y, new_cursor_line) = new_visible_line_ranges
        .iter()
        .enumerate()
        .find(|(_i, r)| r.end >= buf_pos)
        .ok_or(InvalidBufPos {
            buf_pos,
            buf_len: buf.len(),
        })?;

    if buf_pos < new_cursor_line.start {
        info!("Old cursor position no longer on screen");
        return Ok(CursorPos { x: 0, y: 0 });
    };

    let new_cursor_x = buf_pos - new_cursor_line.start;
    Ok(CursorPos {
        x: new_cursor_x,
        y: new_cursor_y,
    })
}
fn line_ranges_to_visible_line_ranges(
    line_ranges: &[Range<usize>],
    height: usize,
) -> &[Range<usize>] {
    if line_ranges.is_empty() {
        return line_ranges;
    }
    let num_lines = line_ranges.len();
    let first_visible_line = num_lines.saturating_sub(height);
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
) -> PadBufferForWriteResponse {
    let mut visible_line_ranges = {
        // Calculate in block scope to avoid accidental usage of scrollback line ranges later
        let line_ranges = calc_line_ranges(buf, width);
        line_ranges_to_visible_line_ranges(&line_ranges, height).to_vec()
    };

    let mut padding_start_pos = None;
    let mut num_inserted_characters = 0;

    let vertical_padding_needed = if cursor_pos.y + 1 > visible_line_ranges.len() {
        cursor_pos.y + 1 - visible_line_ranges.len()
    } else {
        0
    };

    if vertical_padding_needed != 0 {
        padding_start_pos = Some(buf.len());
        num_inserted_characters += vertical_padding_needed;
    }

    for _ in 0..vertical_padding_needed {
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

    // If we did not set the padding start position, it means that we are padding not at the end of
    // the buffer, but at the end of a line
    if padding_start_pos.is_none() {
        padding_start_pos = Some(actual_end);
    }

    let number_of_spaces = if desired_end > actual_end {
        desired_end - actual_end
    } else {
        0
    };

    num_inserted_characters += number_of_spaces;

    for i in 0..number_of_spaces {
        buf.insert(actual_end + i, b' ');
    }
    let start_buf_pos =
        padding_start_pos.expect("start buf pos should be guaranteed initialized by this point");

    PadBufferForWriteResponse {
        write_idx: desired_start,
        inserted_padding: start_buf_pos..start_buf_pos + num_inserted_characters,
    }
}

fn cursor_to_buf_pos_from_visible_line_ranges(
    cursor_pos: &CursorPos,
    visible_line_ranges: &[Range<usize>],
) -> Option<(usize, Range<usize>)> {

    visible_line_ranges.get(cursor_pos.y).and_then(|range| {
        let candidate_pos = range.start + cursor_pos.x;
        if candidate_pos > range.end {
            None
        } else {
            Some((candidate_pos, range.clone()))
        }
    })
}
fn cursor_to_buf_pos(
    buf: &[u8],
    cursor_pos: &CursorPos,
    width: usize,
    height: usize,
) -> Option<(usize, Range<usize>)> {
    let line_ranges = calc_line_ranges(buf, width);
    let visible_line_ranges = line_ranges_to_visible_line_ranges(&line_ranges, height);

    cursor_to_buf_pos_from_visible_line_ranges(cursor_pos, visible_line_ranges)
}



pub(crate) struct TerminalBufferInsertResponse {
    /// Range of written data after insertion of padding
    pub written_range: Range<usize>,
    /// Range of written data that is new. Note this will shift all data after it
    /// Includes padding that was previously not there, e.g. newlines needed to get to the
    /// requested row for writing
    pub insertion_range: Range<usize>,
    pub(crate) new_cursor_pos: CursorPos,
}

pub(crate) struct TerminalBuffer {
    pub(crate) buf: Vec<u8>,
    pub(crate) width: usize,   // Make sure this is pub(crate)
    pub(crate) height: usize,  // Make sure this is pub(crate)
}


impl TerminalBuffer {
        pub fn new(width: usize, height: usize) -> TerminalBuffer {
            TerminalBuffer {
                buf: vec![],
                width,
                height,
            }

        }
    pub(crate) fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub(crate) fn delete_forwards(
        &mut self,
        cursor_pos: &CursorPos,
        num_chars: usize,
    ) -> Option<Range<usize>> {
        let Some((buf_pos, line_range)) =
            cursor_to_buf_pos(&self.buf, cursor_pos, self.width, self.height)
        else {
            return None;
        };

        let mut delete_range = buf_pos..buf_pos + num_chars;

        if delete_range.end > line_range.end && self.buf.get(line_range.end) != Some(&b'\n') {
            self.buf.insert(line_range.end, b'\n');
        }

        delete_range.end = line_range.end.min(delete_range.end);

        self.buf.drain(delete_range.clone());
        Some(delete_range)
    }
    pub fn set_win_size(
        &mut self,
        width: usize,
        height: usize,
        cursor_pos: &CursorPos,
    ) -> TerminalBufferSetWinSizeResponse {
        let changed = self.width != width || self.height != height;
        if !changed {
            return TerminalBufferSetWinSizeResponse {
                changed: false,
                insertion_range: 0..0,
                new_cursor_pos: cursor_pos.clone(),
            };
        }

        // Ensure that the cursor position has a valid buffer position. That way when we resize we
        // can just look up where the cursor is supposed to be and map it back to it's new cursor
        // position
        let pad_response =
            pad_buffer_for_write(&mut self.buf, self.width, cursor_pos,self.height, 0);
        let buf_pos = pad_response.write_idx;
        let inserted_padding = pad_response.inserted_padding;
        let new_cursor_pos = buf_to_cursor_pos(&self.buf, width, height, buf_pos)
            .expect("buf pos should exist in buffer");
        self.width = width;
        self.height = height;

        TerminalBufferSetWinSizeResponse {
            changed,
            insertion_range: inserted_padding,
            new_cursor_pos,
        }
    }

    pub(crate) fn insert_data(&mut self, cursor_pos: &CursorPos, data: &[u8]) -> TerminalBufferInsertResponse {
        let PadBufferForWriteResponse {
            write_idx,
            inserted_padding,
        } = pad_buffer_for_write(
            &mut self.buf,
            self.width,
            cursor_pos,
            self.height,

            data.len(),
        );
        let write_range = write_idx..write_idx + data.len();
        self.buf[write_range.clone()].copy_from_slice(data);
        let new_cursor_pos = buf_to_cursor_pos(&self.buf, self.width, self.height, write_range.end).expect("buf pos should exist in buffer");;
        TerminalBufferInsertResponse {
            written_range: write_range,
            insertion_range: inserted_padding,
            new_cursor_pos,
        }
    }
    pub fn clear_line_forwards(&mut self, cursor_pos: &CursorPos) -> Option<Range<usize>> {
        // Can return early if none, we didn't delete anything if there is nothing to delete
        let (buf_pos, line_range) =
            cursor_to_buf_pos(&self.buf, cursor_pos, self.width, self.height)?;

        let del_range = buf_pos..line_range.end;
        self.buf.drain(del_range.clone());
        Some(del_range)
    }
    /// Inserts data, but will not wrap. If line end is hit, data stops
    pub fn insert_spaces(
        &mut self,
        cursor_pos: &CursorPos,
        mut num_spaces: usize,
    ) -> TerminalBufferInsertResponse {
        num_spaces = self.width.min(num_spaces);

        let buf_pos = cursor_to_buf_pos(&self.buf, cursor_pos, self.width, self.height);
        match buf_pos {
            Some((buf_pos, line_range)) => {
                // Insert spaces until either we hit num_spaces, or the line width is too long
                let line_len = line_range.end - line_range.start;
                let num_inserted = (num_spaces).min(self.width - line_len);

                // Overwrite existing with spaces until we hit num_spaces or we hit the line end
                let num_overwritten = (num_spaces - num_inserted).min(line_range.end - buf_pos);

                // NOTE: We do the overwrite first so we don't have to worry about adjusting
                // indices for the newly inserted data
                self.buf[buf_pos..buf_pos + num_overwritten].fill(b' ');
                self.buf
                    .splice(buf_pos..buf_pos, std::iter::repeat(b' ').take(num_inserted));

                let used_spaces = num_inserted + num_overwritten;
                TerminalBufferInsertResponse {
                    written_range: buf_pos..buf_pos + used_spaces,
                    insertion_range: buf_pos..buf_pos + num_inserted,
                    new_cursor_pos: cursor_pos.clone(),
                }
            }
            None => {
                let PadBufferForWriteResponse {
                    write_idx,
                    inserted_padding,
                } = pad_buffer_for_write(
                    &mut self.buf,
                    self.width,
                    cursor_pos,
                    self.height,
                    num_spaces,
                );
                TerminalBufferInsertResponse {
                    written_range: write_idx..write_idx + num_spaces,
                    insertion_range: inserted_padding,
                    new_cursor_pos: cursor_pos.clone(),
                }
            }
        }
    }

    pub fn clear_forwards(&mut self, cursor_pos: &CursorPos) -> Option<usize> {
        let line_ranges = calc_line_ranges(&self.buf, self.width);
        let visible_line_ranges = line_ranges_to_visible_line_ranges(&line_ranges, self.height);

        let Some((buf_pos, _)) =
            cursor_to_buf_pos_from_visible_line_ranges(cursor_pos, visible_line_ranges)
        else {
            return None;
        };

        let previous_last_char = self.buf[buf_pos];
        self.buf.truncate(buf_pos);

        // If we truncate at the start of a line, and the previous line did not end with a newline,
        // the first inserted newline will not have an effect on the number of visible lines. This
        // is because we are allowed to have a trailing newline that is longer than the terminal
        // width. To keep the cursor pos the same as it was before, if the truncate position is the
        // start of a line, and the previous character is _not_ a newline, insert an extra newline
        // to compensate
        //
        // If we truncated a newline it's the same situation
        if cursor_pos.x == 0 && buf_pos > 0 && self.buf[buf_pos - 1] != b'\n'
            || previous_last_char == b'\n'
        {
            self.buf.push(b'\n');
        }

        for line in visible_line_ranges {
            if line.end > buf_pos {
                self.buf.push(b'\n');
            }
        }

        let new_cursor_pos =
            buf_to_cursor_pos(&self.buf, self.width, self.height, buf_pos).map(|mut pos| {
                // NOTE: buf to cursor pos may put the cursor one past the end of the line. In this
                // case it's ok because there are two valid cursor positions and we only care about one
                // of them
                if pos.x == self.width {
                    pos.x = 0;
                    pos.y += 1;
                }
                pos
            });

        assert_eq!(new_cursor_pos, Ok(cursor_pos.clone()));
        Some(buf_pos)
    }

    pub(crate) fn clear_all(&mut self) {
        self.buf.clear();
    }

    pub(crate) fn data(&self) -> crate::terminal_emulator::TerminalData<&[u8]> {
        let line_ranges = calc_line_ranges(&self.buf, self.width);
        let visible_line_ranges = line_ranges_to_visible_line_ranges(&line_ranges, self.height);
        if self.buf.is_empty() {
            return crate::terminal_emulator::TerminalData {
                scrollback: &[],
                visible: &self.buf,
            };
        }
        let start = visible_line_ranges[0].start;
        crate::terminal_emulator::TerminalData {
            scrollback: &self.buf[0..start],
            visible: &self.buf[start..],
        }
    }

}
#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_calc_line_ranges() {
        let line_starts = calc_line_ranges(b"asdf\n0123456789\n012345678901", 10);
        assert_eq!(line_starts, &[0..4, 5..15, 16..26, 26..28]);
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
}
