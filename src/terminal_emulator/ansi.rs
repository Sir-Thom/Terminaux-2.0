
use super::Mode;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectGraphicRendition {
    // NOTE: Non-exhaustive list
    Reset,
    Bold,
    BlinkSlow,
    Faint,          // 2
    Italic,         // 3
    Underline,      // 4
    BlinkRapid,     // 6
    Reverse,        // 7
    Conceal,        // 8
    Reveal,         // 28 (companion to 8)
    NotItalic,      // 23
    NotUnderline,   // 24
    NormalIntensity,// 22
    ForegroundDefault,
    ForegroundBlack,
    ForegroundRed,
    ForegroundGreen,
    ForegroundYellow,
    ForegroundBlue,
    ForegroundMagenta,
    ForegroundCyan,
    ForegroundWhite,
    BackgroundDefault,
    BackgroundBlack,
    BackgroundRed,
    BackgroundGreen,
    BackgroundYellow,
    BackgroundBlue,
    BackgroundMagenta,
    BackgroundCyan,
    BackgroundWhite,
    ForegroundBrightBlack,
    ForegroundBrightRed,
    ForegroundBrightGreen,
    ForegroundBrightYellow,
    ForegroundBrightBlue,
    ForegroundBrightMagenta,
    ForegroundBrightCyan,
    ForegroundBrightWhite,
    BackgroundBrightBlack,
    BackgroundBrightRed,
    BackgroundBrightGreen,
    BackgroundBrightYellow,
    BackgroundBrightBlue,
    BackgroundBrightMagenta,
    BackgroundBrightCyan,
    BackgroundBrightWhite,
    Foreground8Bit(u8),       // \x1b[38;5;<n>m
    Background8Bit(u8),       // \x1b[48;5;<n>m
    ForegroundTrueColor(u8, u8, u8), // \x1b[38;2;<r>;<g>;<b>m
    BackgroundTrueColor(u8, u8, u8), // \x1b[48;2;<r>;<g>;<b>m
    Unknown(usize),
}

impl SelectGraphicRendition {
    fn from_usize(val: usize, params: &[Option<usize>]) -> SelectGraphicRendition {
        match val {
            0 => SelectGraphicRendition::Reset,
            1 => SelectGraphicRendition::Bold,
            2 => SelectGraphicRendition::Faint,
            3 => SelectGraphicRendition::Italic,
            4 => SelectGraphicRendition::Underline,
            5 => SelectGraphicRendition::BlinkSlow,
            6 => SelectGraphicRendition::BlinkRapid,
            7 => SelectGraphicRendition::Reverse,
            8 => SelectGraphicRendition::Conceal,
            22 => SelectGraphicRendition::NormalIntensity,
            23 => SelectGraphicRendition::NotItalic,
            24 => SelectGraphicRendition::NotUnderline,
            28 => SelectGraphicRendition::Reveal,
            30 => SelectGraphicRendition::ForegroundBlack,
            31 => SelectGraphicRendition::ForegroundRed,
            32 => SelectGraphicRendition::ForegroundGreen,
            33 => SelectGraphicRendition::ForegroundYellow,
            34 => SelectGraphicRendition::ForegroundBlue,
            35 => SelectGraphicRendition::ForegroundMagenta,
            36 => SelectGraphicRendition::ForegroundCyan,
            37 => SelectGraphicRendition::ForegroundWhite,
            38 => {
                if params.len() >= 2 {
                    match params[0] {
                        Some(5) => {
                            // 8-bit color: \x1b[38;5;<n>m
                            if let Some(n) = params.get(1).copied().flatten() {
                                return SelectGraphicRendition::Foreground8Bit(n as u8);
                            }
                        }
                        Some(2) => {
                            // True color: \x1b[38;2;<r>;<g>;<b>m
                            if params.len() >= 4 {
                                let r = params[1].unwrap_or(0) as u8;
                                let g = params[2].unwrap_or(0) as u8;
                                let b = params[3].unwrap_or(0) as u8;
                                return SelectGraphicRendition::ForegroundTrueColor(r, g, b);
                            }
                        }
                        _ => {}
                    }
                }
                // Fallback if parameters are invalid
                SelectGraphicRendition::Unknown(val)
            }
            40 => SelectGraphicRendition::BackgroundBlack,
            41 => SelectGraphicRendition::BackgroundRed,
            42 => SelectGraphicRendition::BackgroundGreen,
            43 => SelectGraphicRendition::BackgroundYellow,
            44 => SelectGraphicRendition::BackgroundBlue,
            45 => SelectGraphicRendition::BackgroundMagenta,
            46 => SelectGraphicRendition::BackgroundCyan,
            47 => SelectGraphicRendition::BackgroundWhite,
            48 => {
                // Similar logic for background colors
                if params.len() >= 2 {
                    match params[0] {
                        Some(5) => {
                            if let Some(n) = params.get(1).copied().flatten() {
                                return SelectGraphicRendition::Background8Bit(n as u8);
                            }
                        }
                        Some(2) => {
                            if params.len() >= 4 {
                                let r = params[1].unwrap_or(0) as u8;
                                let g = params[2].unwrap_or(0) as u8;
                                let b = params[3].unwrap_or(0) as u8;
                                return SelectGraphicRendition::BackgroundTrueColor(r, g, b);
                            }
                        }
                        _ => {}
                    }
                }
                SelectGraphicRendition::Unknown(val)
            }
            90 => SelectGraphicRendition::ForegroundBrightBlack,
            91 => SelectGraphicRendition::ForegroundBrightRed,
            92 => SelectGraphicRendition::ForegroundBrightGreen,
            93 => SelectGraphicRendition::ForegroundBrightYellow,
            94 => SelectGraphicRendition::ForegroundBrightBlue,
            95 => SelectGraphicRendition::ForegroundBrightMagenta,
            96 => SelectGraphicRendition::ForegroundBrightCyan,
            97 => SelectGraphicRendition::ForegroundBrightWhite,
            100 => SelectGraphicRendition::BackgroundBrightBlack,
            101 => SelectGraphicRendition::BackgroundBrightRed,
            102 => SelectGraphicRendition::BackgroundBrightGreen,
            103 => SelectGraphicRendition::BackgroundBrightYellow,
            104 => SelectGraphicRendition::BackgroundBrightBlue,
            105 => SelectGraphicRendition::BackgroundBrightMagenta,
            106 => SelectGraphicRendition::BackgroundBrightCyan,
            107 => SelectGraphicRendition::BackgroundBrightWhite,


            _ => SelectGraphicRendition::Unknown(val),
        }
    }
}


#[derive(Debug, Eq, PartialEq)]
pub enum TerminalOutput {
    SetCursorPos { x: Option<usize>, y: Option<usize> },
    ClearForwards,
    SetCursorVisibility(bool),
    CarriageReturn,
    Backspace,
    Newline,
    ClearAll,
    Sgr(SelectGraphicRendition),
    Data(Vec<u8>),
    Invalid,
    SetMode(Mode),
    ResetMode(Mode),
    Delete(usize),
    ClearLineForwards,
    // ich (8.3.64 of ecma-48)
    InsertSpaces(usize),
    //SetCursorVisibility(bool),
    EnterAltScreen,
    ExitAltScreen,
    CursorUp(usize),
    CursorDown(usize),
    CursorForward(usize),
    CursorBackward(usize),
}

fn mode_from_params(params: &[u8]) -> Mode {
    match params {
        b"?1" => Mode::Decckm,
        _ => Mode::Unknown(params.to_vec()),
    }
}

enum CsiParserState {
    Params,
    Intermediates,
    Finished(u8),
    Invalid,
    InvalidFinished,
}


fn is_csi_terminator(b: u8) -> bool {
    (0x40..=0x7d).contains(&b)
}

fn is_csi_param(b: u8) -> bool {
    (0x30..=0x3f).contains(&b)
}

fn is_csi_intermediate(b: u8) -> bool {
    (0x20..=0x2f).contains(&b)
}

fn extract_param(idx: usize, params: &[Option<usize>]) -> Option<usize> {
    params.get(idx).copied().flatten()
}

fn split_params_into_semicolon_delimited_usize(params: &[u8]) -> Result<Vec<Option<usize>>, ()> {
    let params = params
        .split(|b| *b == b';')
        .map(parse_param_as_usize)
        .collect::<Result<Vec<Option<usize>>, ()>>();

    params
}

fn parse_param_as_usize(param_bytes: &[u8]) -> Result<Option<usize>, ()> {
    let param_str = std::str::from_utf8(param_bytes).expect("valid utf8");
    if param_str.is_empty() {
        return Ok(None);
    }
    let param = param_str.parse().map_err(|_| ())?; // Valid use of `?`
    Ok(Some(param))
}

struct CsiParser {
    state: CsiParserState,
    params: Vec<u8>,
    intermediates: Vec<u8>,
}

impl CsiParser {
    fn new() -> CsiParser {
        CsiParser {
            state: CsiParserState::Params,
            params: Vec::new(),
            intermediates: Vec::new(),
        }
    }


    fn push(&mut self, b: u8) {
        if let CsiParserState::Finished(_) | CsiParserState::InvalidFinished = &self.state {
            panic!("CsiParser should not be pushed to once finished");
        }

        match &mut self.state {
            CsiParserState::Params => {
                if is_csi_param(b) {
                    self.params.push(b);
                } else if is_csi_intermediate(b) {
                    self.intermediates.push(b);
                    self.state = CsiParserState::Intermediates;
                } else if is_csi_terminator(b) {
                    self.state = CsiParserState::Finished(b);

                } else {
                    self.state = CsiParserState::Invalid
                }
            }
            CsiParserState::Intermediates => {
                if is_csi_param(b) {
                    self.state = CsiParserState::Invalid;
                } else if is_csi_intermediate(b) {
                    self.intermediates.push(b);
                } else if is_csi_terminator(b) {
                    self.state = CsiParserState::Finished(b);
                } else {
                    self.state = CsiParserState::Invalid
                }
            }

            CsiParserState::Invalid => {
                if is_csi_terminator(b) {
                    self.state = CsiParserState::InvalidFinished;
                }

            }
            CsiParserState::Finished(_) | CsiParserState::InvalidFinished => {
                unreachable!();
            }
        }
    }
}

enum AnsiParserInner {
    Empty,
    Escape,
    Csi(CsiParser),
}

pub struct AnsiParser {
    inner: AnsiParserInner,
}
fn push_data_if_non_empty(data: &mut Vec<u8>, output: &mut Vec<TerminalOutput>) {
    if !data.is_empty() {
        output.push(TerminalOutput::Data(std::mem::take(data)));
    }
}
impl AnsiParser {
    pub fn new() -> AnsiParser {
        AnsiParser {
            inner: AnsiParserInner::Empty,
        }
    }

    pub fn push(&mut self, incoming: &[u8]) -> Vec<TerminalOutput> {
        let mut output = Vec::new();
        let mut data_output = Vec::new();
        for b in incoming {
            match &mut self.inner {
                AnsiParserInner::Empty => {
                    if *b == b'\x1b' {
                        self.inner = AnsiParserInner::Escape;
                        continue;
                    }
                    if *b == b'\r' {
                        push_data_if_non_empty(&mut data_output, &mut output);
                        output.push(TerminalOutput::CarriageReturn);
                        continue;
                    }

                    if *b == b'\n' {
                        push_data_if_non_empty(&mut data_output, &mut output);
                        output.push(TerminalOutput::Newline);
                        continue;
                    }
                    // print the contents of the buffer
                   // println!("Data: {:?}", data_output);
                    // Explicitly check for Backspace (0x08) and DEL (0x7f)
                    if *b == 0x08 || *b == 0x7f {
                        push_data_if_non_empty(&mut data_output, &mut output);
                        output.push(TerminalOutput::Backspace);
                        continue;
                    }

                    data_output.push(*b);
                }
                AnsiParserInner::Escape => {
                    if !data_output.is_empty() {
                        output.push(TerminalOutput::Data(std::mem::take(&mut data_output)));
                    }

                    match b {
                        b'[' => {
                            self.inner = AnsiParserInner::Csi(CsiParser::new());
                        }
                        _ => {
                            let b_utf8 = std::char::from_u32(*b as u32);
                            warn!("Unhandled escape sequence {b_utf8:?} {b:x}");
                            self.inner = AnsiParserInner::Empty;
                        }
                    }
                }
                AnsiParserInner::Csi(parser) => {
                    parser.push(*b);
                    match parser.state {
                        CsiParserState::Finished(b'H') => {
                            let params =
                                split_params_into_semicolon_delimited_usize(&parser.params);

                            let Ok(params) = params else {
                                warn!("Invalid cursor set position sequence");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };

                            output.push(TerminalOutput::SetCursorPos {
                                x: Some(extract_param(0, &params).unwrap_or(1)),
                                y: Some(extract_param(1, &params).unwrap_or(1)),
                            });
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'K') => {
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid erase in line command");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };

                            // ECMA-48 8.3.39
                            match param.unwrap_or(0) {
                                0 => output.push(TerminalOutput::ClearLineForwards),
                                v => {
                                    warn!("Unsupported erase in line command ({v})");
                                    output.push(TerminalOutput::Invalid);
                                }
                            }

                            self.inner = AnsiParserInner::Empty;
                        }

                        CsiParserState::Finished(b'G') => {
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid cursor set position sequence");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };

                            let x_pos = param.unwrap_or(1);

                            output.push(TerminalOutput::SetCursorPos {
                                x: Some(x_pos),
                                y: None,
                            });
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'J') => {
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid clear command");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };

                            let ret = match param.unwrap_or(0) {
                                0 => TerminalOutput::ClearForwards,
                             //   1 => TerminalOutput::ClearBackwards,
                                2 | 3 => TerminalOutput::ClearAll,
                                _ => TerminalOutput::Invalid,
                            };
                            output.push(ret);
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'h') => {
                            if parser.params == b"?1049" {
                                output.push(TerminalOutput::EnterAltScreen);
                            }else {
                            output.push(TerminalOutput::SetMode(mode_from_params(&parser.params)));}
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'l') => {
                            if parser.params == b"?1049" {
                                output.push(TerminalOutput::ExitAltScreen);
                            }else {
                            output
                                .push(TerminalOutput::ResetMode(mode_from_params(&parser.params)));}
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'P') => {
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                println!("Invalid del command");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };

                            output.push(TerminalOutput::Delete(param.unwrap_or(1)));

                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'm') => {
                            let params = match split_params_into_semicolon_delimited_usize(&parser.params) {
                                Ok(p) => p,
                                Err(_) => {
                                    output.push(TerminalOutput::Invalid);
                                    self.inner = AnsiParserInner::Empty;
                                    continue;
                                }
                            };

                            let mut i = 0;
                            while i < params.len() {
                                let code = params[i].unwrap_or(0);
                                let sgr = match code {
                                    39 => SelectGraphicRendition::ForegroundDefault,
                                    49 => SelectGraphicRendition::BackgroundDefault,
                                    38 | 48 => {
                                        // Handle multi-parameter codes (foreground/background)
                                        if i + 1 >= params.len() {
                                            SelectGraphicRendition::Unknown(code)
                                        } else {
                                            let subcode = params[i + 1].unwrap_or(0);
                                            match (code, subcode) {
                                                (38, 5) => {
                                                    // 8-bit foreground
                                                    if i + 2 < params.len() {
                                                        let n = params[i + 2].unwrap_or(0) as u8;
                                                        i += 2;
                                                        SelectGraphicRendition::Foreground8Bit(n)
                                                    } else {
                                                        SelectGraphicRendition::Unknown(code)
                                                    }
                                                }
                                                (38, 2) => {
                                                    // True color foreground
                                                    if i + 4 < params.len() {
                                                        let r = params[i + 2].unwrap_or(0) as u8;
                                                        let g = params[i + 3].unwrap_or(0) as u8;
                                                        let b = params[i + 4].unwrap_or(0) as u8;
                                                        i += 4;
                                                        SelectGraphicRendition::ForegroundTrueColor(r, g, b)
                                                    } else {
                                                        SelectGraphicRendition::Unknown(code)
                                                    }
                                                }
                                                (48, 5) => {
                                                    // 8-bit background
                                                    if i + 2 < params.len() {
                                                        let n = params[i + 2].unwrap_or(0) as u8;
                                                        i += 2;
                                                        SelectGraphicRendition::Background8Bit(n)
                                                    } else {
                                                        SelectGraphicRendition::Unknown(code)
                                                    }
                                                }
                                                (48, 2) => {
                                                    // True color background
                                                    if i + 4 < params.len() {
                                                        let r = params[i + 2].unwrap_or(0) as u8;
                                                        let g = params[i + 3].unwrap_or(0) as u8;
                                                        let b = params[i + 4].unwrap_or(0) as u8;
                                                        i += 4;
                                                        SelectGraphicRendition::BackgroundTrueColor(r, g, b)
                                                    } else {
                                                        SelectGraphicRendition::Unknown(code)
                                                    }
                                                }
                                                _ => SelectGraphicRendition::Unknown(code),
                                            }
                                        }
                                    }
                                    _ => {
                                        // Handle single-parameter codes (e.g., 1 = Bold, 31 = Red)
                                        SelectGraphicRendition::from_usize(code, &params)
                                    }
                                };
                                output.push(TerminalOutput::Sgr(sgr));
                                i += 1;
                            }
                            self.inner = AnsiParserInner::Empty;
                        }

                        CsiParserState::Finished(b'A') => {
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid cursor up sequence");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };
                            let lines = param.unwrap_or(1);
                            output.push(TerminalOutput::CursorUp(lines));
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'C') => {
                            // Cursor Forward
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid cursor forward sequence");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };
                            let columns = param.unwrap_or(1);
                            output.push(TerminalOutput::CursorForward(columns));
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'h') => {
                            // Handle Set Mode
                            // Implement set mode logic here
                            if parser.params == b"?25" {
                                output.push(TerminalOutput::SetCursorVisibility(true));
                            }

                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'K') => {
                            // Handle Erase in Line
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid erase in line sequence");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };

                            let ret = match param.unwrap_or(0) {
                                0 => TerminalOutput::ClearForwards,
                                1 => TerminalOutput::Backspace,
                                2 => TerminalOutput::ClearAll,
                                _ => TerminalOutput::Invalid,
                            };
                            output.push(ret);
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'B') => {
                            // Cursor Down
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid cursor down sequence");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };
                            let lines = param.unwrap_or(1);
                            output.push(TerminalOutput::CursorDown(lines));
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'D') => {
                            // Cursor Backward
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid cursor backward sequence");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };
                            let columns = param.unwrap_or(1);
                            output.push(TerminalOutput::CursorBackward(columns));
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(b'l') => {
                            if parser.params == b"?25" {
                                output.push(TerminalOutput::SetCursorVisibility(false));
                            }
                            self.inner = AnsiParserInner::Empty;

                            // Other CSI l handling...
                        }
                        CsiParserState::Finished(b'@') => {
                            let Ok(param) = parse_param_as_usize(&parser.params) else {
                                warn!("Invalid ich command");
                                output.push(TerminalOutput::Invalid);
                                self.inner = AnsiParserInner::Empty;
                                continue;
                            };

                            // ecma-48 8.3.64
                            output.push(TerminalOutput::InsertSpaces(param.unwrap_or(1)));
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Finished(esc) => {
                            warn!(
        "Unhandled csi code: {:?} {esc:x} {}/{}",
                                std::char::from_u32(esc as u32),
                                esc >> 4,
                                esc & 0xf,
                            );
                            output.push(TerminalOutput::Invalid);
                            self.inner = AnsiParserInner::Empty;
                        }
                        CsiParserState::Invalid => {
                            warn!("Invalid CSI sequence");
                            output.push(TerminalOutput::Invalid);
                            self.inner = AnsiParserInner::Empty;
                        }
                        _ => {}
                    }
                }
            }
        }

        if !data_output.is_empty() {
            output.push(TerminalOutput::Data(data_output));
        }

        output
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_set_cursor_position() {
        let mut output_buffer = AnsiParser::new();
        let parsed = output_buffer.push(b"\x1b[32;15H");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0],
            TerminalOutput::SetCursorPos {
                x: Some(32),
                y: Some(15)
            }
        ));

        let parsed = output_buffer.push(b"\x1b[;32H");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0],
            TerminalOutput::SetCursorPos {
                x: Some(1),
                y: Some(32)
            }
        ));

        let parsed = output_buffer.push(b"\x1b[32H");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0],
            TerminalOutput::SetCursorPos {
                x: Some(32),
                y: Some(1)
            }
        ));

        let parsed = output_buffer.push(b"\x1b[32;H");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0],
            TerminalOutput::SetCursorPos {
                x: Some(32),
                y: Some(1)
            }
        ));

        let parsed = output_buffer.push(b"\x1b[H");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0],
            TerminalOutput::SetCursorPos {
                x: Some(1),
                y: Some(1)
            }
        ));

        let parsed = output_buffer.push(b"\x1b[;H");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0],
            TerminalOutput::SetCursorPos {
                x: Some(1),
                y: Some(1)
            }
        ));
    }

    #[test]
    fn test_clear() {
        let mut output_buffer = AnsiParser::new();
        let parsed = output_buffer.push(b"\x1b[J");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(parsed[0], TerminalOutput::ClearForwards,));

        let mut output_buffer = AnsiParser::new();
        let parsed = output_buffer.push(b"\x1b[0J");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(parsed[0], TerminalOutput::ClearForwards,));



        let mut output_buffer = AnsiParser::new();
        let parsed = output_buffer.push(b"\x1b[2J");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(parsed[0], TerminalOutput::ClearAll,));
    }

    #[test]
    fn test_invalid_clear() {
        let mut output_buffer = AnsiParser::new();
        let parsed = output_buffer.push(b"\x1b[8J");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(parsed[0], TerminalOutput::Invalid,));
    }

    #[test]
    fn test_invalid_csi() {
        let mut output_buffer = AnsiParser::new();
        let parsed = output_buffer.push(b"\x1b[-23;H");
        assert!(matches!(parsed[0], TerminalOutput::Invalid));

        let mut output_buffer = AnsiParser::new();
        let parsed = output_buffer.push(b"\x1b[asdf");
        assert!(matches!(parsed[0], TerminalOutput::Invalid));
    }

    #[test]
    fn test_parsing_unknown_csi() {
        let mut parser = CsiParser::new();
        for b in b"0123456789:;<=>?!\"#$%&'()*+,-./}" {
            parser.push(*b);
        }

        assert_eq!(parser.params, b"0123456789:;<=>?");
        assert_eq!(parser.intermediates, b"!\"#$%&'()*+,-./");
        assert!(matches!(parser.state, CsiParserState::Finished(b'}')));
    }

    #[test]
    fn test_parsing_invalid_csi() {
        let mut parser = CsiParser::new();
        for b in b"0$0" {
            parser.push(*b);
        }

        assert!(matches!(parser.state, CsiParserState::Invalid));
        parser.push(b'm');
        assert!(matches!(parser.state, CsiParserState::InvalidFinished));
    }

    #[test]
    fn test_empty_sgr() {
        let mut output_buffer = AnsiParser::new();
        let parsed = output_buffer.push(b"\x1b[m");
        assert!(matches!(
            parsed[0],
            TerminalOutput::Sgr(SelectGraphicRendition::Reset)
        ));
    }

    #[test]
    fn test_color_parsing() {
        let mut output_buffer = AnsiParser::new();

        struct ColorCode(u8);

        impl std::fmt::Display for ColorCode {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_fmt(format_args!("\x1b[{}m", self.0))
            }
        }

        let mut test_input = String::new();
        for i in 30..=37 {
            test_input.push_str(&ColorCode(i).to_string());
            test_input.push('a');
        }

        for i in 90..=97 {
            test_input.push_str(&ColorCode(i).to_string());
            test_input.push('a');
        }

        for i in 40..=47 {
            test_input.push_str(&ColorCode(i).to_string());
            test_input.push('a');
        }

        for i in 100..=107 {
            test_input.push_str(&ColorCode(i).to_string());
            test_input.push('a');
        }


        let output = output_buffer.push(test_input.as_bytes());
        assert_eq!(
            output,
            &[
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBlack),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundRed),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundGreen),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundYellow),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBlue),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundMagenta),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundCyan),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundWhite),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBrightBlack),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBrightRed),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBrightGreen),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBrightYellow),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBrightBlue),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBrightMagenta),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBrightCyan),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::ForegroundBrightWhite),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBlack),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundRed),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundGreen),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundYellow),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBlue),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundMagenta),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundCyan),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundWhite),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBrightBlack),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBrightRed),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBrightGreen),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBrightYellow),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBrightBlue),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBrightMagenta),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBrightCyan),
                TerminalOutput::Data(b"a".into()),
                TerminalOutput::Sgr(SelectGraphicRendition::BackgroundBrightWhite),
                TerminalOutput::Data(b"a".into()),

            ]
        );
    }
    #[test]
    fn test_true_color_parsing() {
        let mut output_buffer = AnsiParser::new();

        // Test foreground true color
        let parsed = output_buffer.push(b"\x1b[38;2;255;128;0m");
        assert!(matches!(
        parsed[0],
        TerminalOutput::Sgr(SelectGraphicRendition::ForegroundTrueColor(255, 128, 0))
    ));

        // Test background true color
        let parsed = output_buffer.push(b"\x1b[48;2;0;255;128m");
        assert!(matches!(
        parsed[0],
        TerminalOutput::Sgr(SelectGraphicRendition::BackgroundTrueColor(0, 255, 128))
    ));
    }
}