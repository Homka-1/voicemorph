// Always a windowed program: no terminal opens beside it, in any build.
#![windows_subsystem = "windows"]

/// Prints a message to the terminal if there is one; never stops the app when there isn't.
macro_rules! note {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($t)*);
    }};
}

mod apploop;
mod apppage;
mod audio;
mod state;
mod dsp;
mod hotkeys;
mod icons;
mod tray;
mod tts;
mod media;
mod ui;
mod widgets;
mod winaudio;

fn main() -> eframe::Result<()> {
    // Screenshot runs are checks and may run beside the real app; anything else
    // reopens an already running copy (e.g. hidden in the tray) instead of starting twice.
    let shot = std::env::args().any(|a| a == "--screenshot");
    // Test options print messages: show them in the terminal that started us, if any.
    #[cfg(windows)]
    if std::env::args().skip(1).any(|a| a.starts_with("--")) {
        unsafe {
            windows_sys::Win32::System::Console::AttachConsole(
                windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
            );
        }
    }
    if !shot && tray::signal_existing() {
        return Ok(());
    }
    ui::run()
}
