use std::sync::Arc;
use crate::terminal_emulator::{buffer_index_to_cursor_pos, cursor_to_buffer_position, BlinkMode, CursorPos, CursorState, FormatTag, TerminalColor, TerminalEmulator, TerminalInput};
use eframe::egui::{self, CentralPanel, Color32, Event, FontData, FontDefinitions,Modifiers, FontFamily, InputState, Key, Rect, TextFormat, TextStyle, Ui};
use eframe::egui::text::LayoutJob;
use std::borrow::Cow;
use log::info;

const REGULAR_FONT_NAME: &str = "JetBrainsMono-Regular";
const BOLD_FONT_NAME: &str = "JetBrainsMono-Bold";

const ITALIC_FONT_NAME: &str = "JetBrainsMono-Italic";

fn char_to_ctrl_code(c: u8) -> u8 {
    // https://catern.com/posts/terminal_quirks.html
    // man ascii
    c & 0b0001_1111
}

struct TerminalFonts {
    regular: FontFamily,
    bold: FontFamily,
    italic: FontFamily,
}

impl TerminalFonts {
    fn new() -> TerminalFonts {
        let bold = FontFamily::Name(BOLD_FONT_NAME.to_string().into());
        let regular = FontFamily::Name(REGULAR_FONT_NAME.to_string().into());
        let italic = FontFamily::Name(ITALIC_FONT_NAME.to_string().into());

        TerminalFonts { regular, bold, italic }
    }

    fn get_family(&self, is_bold: bool,is_italic:bool) -> FontFamily {
        if is_bold {
            self.bold.clone()
        }else if is_italic  {
            self.italic.clone()
        }else {
            self.regular.clone()
        }
    }
}
fn terminal_color_to_egui(default_color: &Color32, color: &TerminalColor) -> Color32 {
    match color {
        TerminalColor::Default => default_color.clone(),
        TerminalColor::ForegroundBlack => Color32::BLACK,
        TerminalColor::ForegroundRed => Color32::RED,
        TerminalColor::ForegroundGreen => Color32::GREEN,
        TerminalColor::ForegroundYellow => Color32::YELLOW,
        TerminalColor::ForegroundBlue => Color32::BLUE,
        TerminalColor::ForegroundMagenta => Color32::from_rgb(255, 0, 255),
        TerminalColor::ForegroundCyan => Color32::from_rgb(0, 255, 255),
        TerminalColor::ForegroundWhite => Color32::WHITE,
        TerminalColor::ForegroundBrightRed => Color32::from_rgb(255, 0, 0),
        TerminalColor::ForegroundBrightGreen => Color32::from_rgb(0, 255, 0),
        TerminalColor::ForegroundBrightYellow => Color32::from_rgb(255, 255, 0),
        TerminalColor::ForegroundBrightBlue => Color32::from_rgb(0, 0, 255),
        TerminalColor::ForegroundBrightMagenta => Color32::from_rgb(255, 0, 255),
        TerminalColor::ForegroundBrightCyan => Color32::from_rgb(0, 255, 255),
        TerminalColor::ForegroundBrightWhite => Color32::from_rgb(255, 255, 255),
        TerminalColor::ForegroundRgb(r, g, b) => Color32::from_rgb(*r, *g, *b),
        TerminalColor::Foreground8Bit(n) => {
            let (r, g, b) = index_to_rgb(*n);
            Color32::from_rgb(r, g, b)
        }
        TerminalColor::BackgroundTrueColor(r, g, b) => Color32::from_rgb(*r, *g, *b),
        TerminalColor::BackgroundBlack => Color32::BLACK,
        TerminalColor::BackgroundRed => Color32::RED,
        TerminalColor::BackgroundGreen => Color32::GREEN,
        TerminalColor::BackgroundYellow => Color32::YELLOW,
        TerminalColor::BackgroundBlue => Color32::BLUE,
        TerminalColor::BackgroundMagenta => Color32::from_rgb(255, 0, 255),
        TerminalColor::BackgroundCyan => Color32::from_rgb(0, 255, 255),
        TerminalColor::BackgroundWhite => Color32::WHITE,
        TerminalColor::BackgroundBrightRed => Color32::from_rgb(255, 0, 0),
        TerminalColor::BackgroundBrightGreen => Color32::from_rgb(0, 255, 0),
        TerminalColor::BackgroundBrightYellow => Color32::from_rgb(255, 255, 0),
        TerminalColor::BackgroundBrightBlue => Color32::from_rgb(0, 0, 255),
        TerminalColor::BackgroundBrightMagenta => Color32::from_rgb(255, 0, 255),
        TerminalColor::BackgroundBrightCyan => Color32::from_rgb(0, 255, 255),
        TerminalColor::BackgroundBrightWhite => Color32::from_rgb(255, 255, 255),
        _ =>  default_color.clone()
    }
}

struct TerminalOutputRenderResponse {
scrollback_area: Rect,
canvas_area: Rect,
}


fn render_terminal_output(
    ui: &mut egui::Ui,
    terminal_emulator: &TerminalEmulator,
) -> TerminalOutputRenderResponse {
    let terminal_data = terminal_emulator.data();
    let mut scrollback_data = terminal_data.scrollback;
    let mut canvas_data = terminal_data.visible;
    let mut format_data = terminal_emulator.format_data();

    // Arguably incorrect. Scrollback does end with a newline, and that newline causes a blank
    // space between widgets. Should we strip it here, or in the terminal emulator output?
    if scrollback_data.ends_with(b"\n") {
        scrollback_data = &scrollback_data[0..scrollback_data.len() - 1];
        if let Some(last_tag) = format_data.scrollback.last_mut() {
            last_tag.end = last_tag.end.min(scrollback_data.len());
        }
    }

    if canvas_data.ends_with(b"\n") {
        canvas_data = &canvas_data[0..canvas_data.len() - 1];
    }

    let response = egui::ScrollArea::new([false, true])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let scrollback_area =
                add_terminal_data_to_ui(ui, scrollback_data, &format_data.scrollback).rect;
            let canvas_area = add_terminal_data_to_ui(ui, canvas_data, &format_data.visible).rect;
            TerminalOutputRenderResponse {
                scrollback_area,
                canvas_area,
            }
        });

    response.inner
}

struct DebugRenderer {
    enable: bool,
}

impl DebugRenderer {
    fn new() -> DebugRenderer {
        DebugRenderer { enable: false }
    }

    fn render(&self, ui: &mut Ui, rect: Rect, color: Color32) {
        if !self.enable {
            return;
        }

        let color = color.gamma_multiply(0.25);
        ui.painter().rect_filled(rect, 0.0, color);
    }
}

fn create_terminal_output_layout_job(
    style: &egui::Style,
    width: f32,
    data: &[u8],
) -> (LayoutJob, TextFormat) {
    let text_style = &style.text_styles[&TextStyle::Monospace];
    let mut job = egui::text::LayoutJob::simple(
        std::str::from_utf8(data).unwrap().to_string(),
        text_style.clone(),
        style.visuals.text_color(),
        width,
    );

    job.wrap.break_anywhere = true;
    let textformat = job.sections[0].format.clone();
    job.sections.clear();
    (job, textformat)
}
fn write_input_to_terminal(input: &InputState, terminal_emulator: &mut TerminalEmulator) {
    for event in &input.raw.events {
        match event {
            Event::Text(text) => {
                for c in text.as_bytes() {
                    terminal_emulator.write(TerminalInput::Ascii(*c));
                }
            }
            Event::Key {
                key: Key::Enter,
                pressed: true,
                ..
            } => {
            terminal_emulator.write(TerminalInput::Enter);
        }
            // https://github.com/emilk/egui/issues/3653
            Event::Copy => {
                terminal_emulator.write(TerminalInput::Ctrl(b'c'));
            }
            Event::Key {
                key,
                pressed: true,
                modifiers: Modifiers { ctrl: true, .. },
                ..
            } => {
                if *key >= Key::A && *key <= Key::Z {
                    let name = key.name();
                    assert!(name.len() == 1);
                    let name_c = name.as_bytes()[0];
                    terminal_emulator.write(TerminalInput::Ctrl(name_c));
                } else if *key == Key::OpenBracket {
                    terminal_emulator.write(TerminalInput::Ctrl(b'['));
                } else if *key == Key::CloseBracket {
                    let ctrl_code = char_to_ctrl_code(b']');
                    terminal_emulator.write(TerminalInput::Ctrl(b']'));
                } else if *key == Key::Backslash {
                    terminal_emulator.write(TerminalInput::Ctrl(b'\\'));
                } else {
                    warn!("Unexpected ctrl key: {}", key.name());
                }
            }
            Event::Key {
                key: Key::Backspace,
                pressed: true,
                ..
            } => {
                terminal_emulator.write(TerminalInput::Backspace);
            }
            Event::Key {
                key: Key::ArrowUp,
                pressed: true,
                ..
            } => {
                terminal_emulator.write(TerminalInput::ArrowUp);
            }
            Event::Key {
                key: Key::ArrowDown,
                pressed: true,
                ..
            } => {
                terminal_emulator.write(TerminalInput::ArrowDown);
            }
            Event::Key {
                key: Key::ArrowLeft,
                pressed: true,
                ..
            } => {
                terminal_emulator.write(TerminalInput::ArrowLeft);
            }
            Event::Key {
                key: Key::ArrowRight,
                pressed: true,
                ..
            } => {
                terminal_emulator.write(TerminalInput::ArrowRight);
            }
            Event::Key {
                key: Key::Home,
                pressed: true,
                ..
            } => {
                terminal_emulator.write(TerminalInput::Home);
            }
            Event::Key {
                key: Key::End,
                pressed: true,
                ..
            } => {
                terminal_emulator.write(TerminalInput::End);
            }
            _ => (),
        };

    }
}
fn index_to_rgb(index: u8) -> (u8, u8, u8) {
let index = index as u32;
if index < 16 {
// Basic 16 colors
match index {
0 => (0, 0, 0),       // Black
1 => (128, 0, 0),     // Red
2 => (0, 128, 0),     // Green
3 => (128, 128, 0),   // Yellow
4 => (0, 0, 128),     // Blue
5 => (128, 0, 128),   // Magenta
6 => (0, 128, 128),   // Cyan
7 => (192, 192, 192), // White
8 => (128, 128, 128), // Bright black
9 => (255, 0, 0),     // Bright red
10 => (0, 255, 0),    // Bright green
11 => (255, 255, 0), // Bright yellow
12 => (0, 0, 255),   // Bright blue
13 => (255, 0, 255), // Bright magenta
14 => (0, 255, 255), // Bright cyan
15 => (255, 255, 255), // Bright white
_ => (0, 0, 0),
}
} else if index >= 16 && index <= 231 {
// 6x6x6 color cube
let index = index - 16;
let r = index / 36;
let g = (index % 36) / 6;
let b = index % 6;
(
(r * 51) as u8,
(g * 51) as u8,
(b * 51) as u8,
)
} else {
// Grayscale
let gray = 8 + (index - 232) as u8 * 10;
(gray, gray, gray)
}
}
fn get_char_size(ctx: &egui::Context) -> (f32, f32) {
    let font_id = ctx.style().text_styles[&egui::TextStyle::Monospace].clone();
    ctx.fonts(move |fonts| {
        // NOTE: Glyph width seems to be a little too wide
        let width = fonts
            .layout(
                "@".to_string(),
                font_id.clone(),
                Color32::WHITE,
                f32::INFINITY,
            )
            .mesh_bounds
            .width();

        let height = fonts.row_height(&font_id);

        (width, height)
    })
}

fn character_to_cursor_offset(
    character_pos: &CursorPos,
    character_size: &(f32, f32),
    content: &[u8],
) -> (f32, f32) {
    let content_by_lines: Vec<&[u8]> = content.split(|b| *b == b'\n').collect();
    let num_lines = content_by_lines.len();
    let x_offset = character_pos.x as f32 * character_size.0;
    let y_offset = (character_pos.y as i64 - num_lines as i64) as f32 * character_size.1;
    (x_offset, y_offset)


}

fn paint_cursor(
    label_rect: Rect,
    character_size: &(f32, f32),
    cursor_pos: &CursorPos,
   // terminal_buf: &[u8],
    ui: &mut Ui,
) {
    let painter = ui.painter();

  //  let bottom = label_rect.bottom();
    let top = label_rect.top();
    let left = label_rect.left();
   // let cursor_offset = character_to_cursor_offset(cursor_pos, character_size, terminal_buf);
   // let cursor_x = cursor_offset.0 - left;
    //let cursor_y = bottom + cursor_offset.1;
    let y_offset = cursor_pos.y as f32 * character_size.1;
    let x_offset = cursor_pos.x as f32 * character_size.0 - left;

    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(left + x_offset, top + y_offset),
            egui::vec2(character_size.0, character_size.1),

        ),
        0.0,
        Color32::GRAY,
    );




}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        REGULAR_FONT_NAME.to_owned(),
        Arc::from(FontData::from_static(include_bytes!("../res/JetBrainsMono-Regular.ttf"))),
    );

    fonts.font_data.insert(
        BOLD_FONT_NAME.to_owned(),
        Arc::from(FontData::from_static(include_bytes!("../res/JetBrainsMono-Bold.ttf"))),
    );
    fonts.font_data.insert(
        ITALIC_FONT_NAME.to_owned(),
        Arc::from(FontData::from_static(include_bytes!("../res/JetBrainsMono-Italic.ttf"))),
    );

    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .insert(0, REGULAR_FONT_NAME.to_owned());

    fonts.families.insert(
        FontFamily::Name(REGULAR_FONT_NAME.to_string().into()),
        vec![REGULAR_FONT_NAME.to_string()],
    );
    fonts.families.insert(
        FontFamily::Name(BOLD_FONT_NAME.to_string().into()),
        vec![BOLD_FONT_NAME.to_string()],
    );
    fonts.families.insert(
        FontFamily::Name(ITALIC_FONT_NAME.to_string().into()),
        vec![ITALIC_FONT_NAME.to_string()],
    );

    ctx.set_fonts(fonts);
}
fn add_terminal_data_to_ui(ui: &mut Ui, data: &[u8], format_data: &[FormatTag]) -> egui::Response {
    let (mut job, mut textformat) =
        create_terminal_output_layout_job(ui.style(), ui.available_width(), data);

    let default_color = textformat.color;
    let terminal_fonts = TerminalFonts::new();

    for tag in format_data {
        let mut range = tag.start..tag.end;
        let color = tag.color;

        if range.end == usize::MAX {
            range.end =  data.len();
        }

        textformat.font_id.family = terminal_fonts.get_family(tag.bold,tag.italic);
        textformat.color = terminal_color_to_egui(&default_color, &color);

        job.sections.push(egui::text::LayoutSection {
            leading_space: 0.0f32,
            byte_range: range,
            format: textformat.clone(),
        });
    }

    ui.label(job)
}

struct TermieGui {
    terminal_emulator: TerminalEmulator,
    character_size: Option<(f32, f32)>,
    last_blink_time: Option<f64>,
    blink_on: bool,
    blink_state: bool,
    last_blink_toggle: Option<f64>,
    debug_renderer: DebugRenderer,
}

impl TermieGui {
    fn update_blink_state(&mut self, ctx: &egui::Context) {
        let current_time = ctx.input(|i| i.time);
        let blink_interval = match self.terminal_emulator.cursor_state.blink_mode {
            BlinkMode::NoBlink => return,
            BlinkMode::SlowBlink => 0.5,  // 1 Hz
            BlinkMode::RapidBlink => 0.25, // 2 Hz
        };

        if let Some(last_toggle) = self.last_blink_toggle {
            if current_time - last_toggle >= blink_interval {
                self.blink_state = !self.blink_state;
                self.last_blink_toggle = Some(current_time);
                ctx.request_repaint();
            }
        } else {
            self.last_blink_toggle = Some(current_time);
            self.blink_state = true;
            ctx.request_repaint();
        }
    }

    fn new(cc: &eframe::CreationContext<'_>, terminal_emulator: TerminalEmulator) -> Self {
        cc.egui_ctx.style_mut(|style| {
            style.override_text_style = Some(TextStyle::Monospace);
        });

        cc.egui_ctx.set_pixels_per_point(1.0);
        setup_fonts(&cc.egui_ctx);

        TermieGui {
            terminal_emulator,
            character_size: None,
            last_blink_time: None,
            blink_on: true,
            blink_state: false,
            last_blink_toggle: None,
            debug_renderer: DebugRenderer::new(),

        }
    }
}

impl eframe::App for TermieGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.character_size.is_none() {
            self.character_size = Some(get_char_size(ctx));
        }



        self.terminal_emulator.read();

        let panel_response = CentralPanel::default().show(ctx, |ui| {
            let frame_response = egui::Frame::none().show(ui, |ui| {
                ui.set_width(
                    (crate::terminal_emulator::TERMINAL_WIDTH as f32 + 0.5)
                        * self.character_size.as_ref().unwrap().0,
                );
                ui.set_height(
                    (crate::terminal_emulator::TERMINAL_HEIGHT as f32 + 0.5)
                        * self.character_size.as_ref().unwrap().1,
                );

                ui.input(|input_state| {
                    write_input_to_terminal(input_state, &mut self.terminal_emulator);
                });

                let output_response = render_terminal_output(ui, &self.terminal_emulator);
                self.debug_renderer
                    .render(ui, output_response.canvas_area, Color32::BLUE);
                self.debug_renderer.render(ui, output_response.scrollback_area, Color32::YELLOW);


                paint_cursor(
                    output_response.canvas_area,
                    self.character_size.as_ref().unwrap(),
                    &self.terminal_emulator.cursor_pos(),
                  //  self.terminal_emulator.data(),
                    ui,
                );
            });
            self.debug_renderer
                .render(ui, frame_response.response.rect, Color32::RED);
        });

        panel_response.response.context_menu(|ui| {
            ui.checkbox(&mut self.debug_renderer.enable, "Debug render");
        });
    }
}


pub fn run(terminal_emulator: TerminalEmulator) {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Terminaux",
        native_options,
        Box::new(move |cc| Ok(Box::new(TermieGui::new(cc, terminal_emulator)))),
    )
        .unwrap();
}

