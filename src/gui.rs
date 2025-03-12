use std::sync::Arc;
use crate::terminal_emulator::{cursor_to_buffer_position, CursorState, TerminalColor, TerminalEmulator};
use eframe::egui::{
    self, CentralPanel, Color32, Event, FontData, FontDefinitions, FontFamily, InputState, Key,
    Rect, TextStyle, Ui,
};

const REGULAR_FONT_NAME: &str = "JetBrainsMono-Regular";
const BOLD_FONT_NAME: &str = "JetBrainsMono-Bold";

fn write_input_to_terminal(input: &InputState, terminal_emulator: &mut TerminalEmulator) {
    for event in &input.events {
        let text = match event {
            Event::Text(text) => text,
            Event::Key {
                key: Key::Enter,
                pressed: true,
                ..
            } => "\n",
            _ => "",
        };

        terminal_emulator.write(text.as_bytes());
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
    character_pos: &CursorState,
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
    cursor_pos: &CursorState,
    terminal_buf: &[u8],
    ui: &mut Ui,
) {
    let painter = ui.painter();

    let bottom = label_rect.bottom();
    let left = label_rect.left();
    let cursor_offset = character_to_cursor_offset(cursor_pos, character_size, terminal_buf);
    //make it blink by adding a delay
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(left + cursor_offset.0 - 1.0, bottom + cursor_offset.1),
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

    ctx.set_fonts(fonts);
}

struct TermieGui {
    terminal_emulator: TerminalEmulator,
    character_size: Option<(f32, f32)>,
}

impl TermieGui {
    fn new(cc: &eframe::CreationContext<'_>, terminal_emulator: TerminalEmulator) -> Self {
        cc.egui_ctx.style_mut(|style| {
            style.override_text_style = Some(TextStyle::Monospace);
        });

        cc.egui_ctx.set_pixels_per_point(1.0);
        setup_fonts(&cc.egui_ctx);

        TermieGui {
            terminal_emulator,
            character_size: None,
        }
    }
}

impl eframe::App for TermieGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.character_size.is_none() {
            self.character_size = Some(get_char_size(ctx));
        }

        self.terminal_emulator.read();

        CentralPanel::default().show(ctx, |ui| {
            ui.input(|input_state| {
                write_input_to_terminal(input_state, &mut self.terminal_emulator);
            });

            let response = unsafe {
                let style = &ctx.style().text_styles[&TextStyle::Monospace];
                let mut job = egui::text::LayoutJob::simple(
                    std::str::from_utf8_unchecked(self.terminal_emulator.data()).to_string(),
                    style.clone(),
                    ctx.style().visuals.text_color(),
                    ui.available_width(),
                );

                let mut textformat = job.sections[0].format.clone();
                job.sections.clear();
                let default_color = textformat.color;
                let bold_font_family = FontFamily::Name(BOLD_FONT_NAME.to_string().into());
                let regular_font_family = FontFamily::Name(REGULAR_FONT_NAME.to_string().into());

                for tag in self.terminal_emulator.format_data() {
                    let mut range = tag.start..tag.end;
                    let color = tag.color;

                    if range.end == usize::MAX {
                        range.end = self.terminal_emulator.data().len()
                    }

                    if tag.bold {
                        textformat.font_id.family = bold_font_family.clone();
                    } else {
                        textformat.font_id.family = regular_font_family.clone();
                    }

                    textformat.color = match color {
                        TerminalColor::Default => default_color,
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
                        TerminalColor::ForegroundRgb(r, g, b) => Color32::from_rgb(r, g, b),
                        _ => default_color,
                    };
                    textformat.background = match color {
                        TerminalColor::BackgroundRgb(r, g, b) => Color32::from_rgb(r, g, b),
                        TerminalColor::BackgroundBlack => Color32::BLACK,
                        TerminalColor::BackgroundRed => Color32::RED,
                        TerminalColor::BackgroundGreen => Color32::GREEN,
                        TerminalColor::BackgroundYellow => Color32::YELLOW,
                        TerminalColor::BackgroundBlue => Color32::BLUE,
                        TerminalColor::BackgroundMagenta => Color32::from_rgb(255, 0, 0),
                        TerminalColor::BackgroundCyan => Color32::from_rgb(0, 255, 0),
                        TerminalColor::BackgroundWhite => Color32::WHITE,
                        TerminalColor::BackgroundBrightRed => Color32::from_rgb(255, 0, 0),
                        TerminalColor::BackgroundBrightGreen => Color32::from_rgb(0, 255, 0),
                        TerminalColor::BackgroundBrightYellow => Color32::from_rgb(255, 255, 0),
                        TerminalColor::BackgroundBrightBlue => Color32::from_rgb(0, 0, 255),
                        TerminalColor::BackgroundBrightMagenta => Color32::from_rgb(255, 0, 255),
                        TerminalColor::BackgroundBrightCyan => Color32::from_rgb(0, 255, 255),
                        TerminalColor::BackgroundBrightWhite => Color32::from_rgb(255, 255, 255),
                        _ => {Color32::BLACK}
                    };

                    job.sections.push(egui::text::LayoutSection {
                        leading_space: 0.0f32,
                        byte_range: range,
                        format: textformat.clone(),
                    });
                }

                ui.label(job)
            };
            // Render background colors
            for tag in self.terminal_emulator.format_data() {
                if let TerminalColor::BackgroundRgb(r, g, b) = tag.color {
                    let start_pos = cursor_to_buffer_position(
                        &CursorState {
                            x: tag.start,
                            y: 0, // Adjust y if needed
                            bold: false,
                            color: TerminalColor::Default,
                        },
                        self.terminal_emulator.data(),
                    );
                    let end_pos = cursor_to_buffer_position(
                        &CursorState {
                            x: tag.end,
                            y: 0, // Adjust y if needed
                            bold: false,
                            color: TerminalColor::Default,
                        },
                        self.terminal_emulator.data(),
                    );

                    let char_size = self.character_size.as_ref().unwrap();
                    let x_start = start_pos as f32 * char_size.0;
                    let x_end = end_pos as f32 * char_size.0;

                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            egui::pos2(x_start, response.rect.min.y),
                            egui::pos2(x_end, response.rect.max.y),
                        ),
                        0.0,
                        Color32::from_rgb(r, g, b),

                    );
                }
            }

            paint_cursor(
                response.rect,
                self.character_size.as_ref().unwrap(),
                &self.terminal_emulator.cursor_pos(),
                self.terminal_emulator.data(),
                ui,
            );
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