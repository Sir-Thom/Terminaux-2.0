mod gui;
mod terminal_emulator;

use terminal_emulator::TerminalEmulator;


fn main() {
    let terminal_emulator = TerminalEmulator::new();
    gui::run(terminal_emulator);

}


