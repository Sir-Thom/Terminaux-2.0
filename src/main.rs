mod gui;
mod terminal_emulator;

use terminal_emulator::TerminalEmulator;


fn main() {
    let mut  terminal_emulator = TerminalEmulator::new();
    println!("\x1b[38;2;255;0;0mThis text is red\x1b[0m");
    println!("\x1b[38;2;0;255;0mThis text is green\x1b[0m");
    println!("\x1b[38;2;0;0;255mThis text is blue\x1b[0m");

    // Test truecolor background
    println!("\x1b[48;2;255;0;0mThis background is red\x1b[0m");
    println!("\x1b[48;2;0;255;0mThis background is green\x1b[0m");
    println!("\x1b[48;2;0;0;255mThis background is blue\x1b[0m");

    // Test combined foreground and background
    println!("\x1b[38;2;255;255;255m\x1b[48;2;0;0;0mWhite on black\x1b[0m");
    println!("\x1b[38;2;0;0;0m\x1b[48;2;255;255;255mBlack on white\x1b[0m");
    gui::run(terminal_emulator);

}


