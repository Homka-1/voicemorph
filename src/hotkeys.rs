//! System-wide hotkeys: hold the chosen modifier key(s) and press F4/F8-F12.
//! Same keys as the original (help text at 0x4c6f20), polled like the original does.
//! New: modifier + 1..9, 0 plays the sound placed in that slot.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hotkey {
    Toggle,
    PrevVoice,
    NextVoice,
    StopSound,
    MusicNext,
    PlayAssigned,
    /// Sound slot 1..=10 (the 0 key is slot 10).
    Slot(u8),
}

pub const MODIFIERS: [(&str, u8); 4] = [("None", 0), ("Ctrl", 0x11), ("Shift", 0x10), ("Alt", 0x12)];

pub const KEYS: [(u8, Hotkey, &str); 6] = [
    (0x73, Hotkey::Toggle, "F4 · turn voice changing on/off"),
    (0x77, Hotkey::PrevVoice, "F8 · previous voice"),
    (0x78, Hotkey::NextVoice, "F9 · next voice"),
    (0x79, Hotkey::StopSound, "F10 · stop sounds and music"),
    (0x7A, Hotkey::MusicNext, "F11 · play music / next track"),
    (0x7B, Hotkey::PlayAssigned, "F12 · play the starred sound"),
];

/// Virtual-key code of a slot's number key.
pub fn slot_vk(slot: u8) -> u8 {
    if slot == 10 {
        0x30
    } else {
        0x30 + slot
    }
}

pub fn slot_key_name(slot: u8) -> String {
    if slot == 10 {
        "0".into()
    } else {
        slot.to_string()
    }
}

pub fn modifier_text(m1: u8, m2: u8) -> String {
    let name = |v: u8| MODIFIERS.iter().find(|m| m.1 == v).map(|m| m.0).unwrap_or("None");
    match (m1, m2) {
        (0, 0) => String::new(),
        (a, 0) => name(a).to_string(),
        (0, b) => name(b).to_string(),
        (a, b) => format!("{}+{}", name(a), name(b)),
    }
}

pub struct HotkeyMods {
    pub primary: AtomicU8,
    pub secondary: AtomicU8,
}

#[cfg(windows)]
fn down(vk: u8) -> bool {
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk as i32) < 0 }
}

#[cfg(not(windows))]
fn down(_vk: u8) -> bool {
    false
}

pub fn spawn(mods: Arc<HotkeyMods>, tx: Sender<Hotkey>) {
    std::thread::spawn(move || {
        let all: Vec<(u8, Hotkey)> = KEYS
            .iter()
            .map(|(vk, hk, _)| (*vk, *hk))
            .chain((1..=10u8).map(|s| (slot_vk(s), Hotkey::Slot(s))))
            .collect();
        let mut was = vec![false; all.len()];
        loop {
            std::thread::sleep(std::time::Duration::from_millis(15));
            let p = mods.primary.load(Ordering::Relaxed);
            let s = mods.secondary.load(Ordering::Relaxed);
            if p == 0 && s == 0 {
                was.iter_mut().for_each(|w| *w = false);
                continue;
            }
            let held = (p == 0 || down(p)) && (s == 0 || down(s));
            for (i, (vk, hk)) in all.iter().enumerate() {
                let now = held && down(*vk);
                if now && !was[i] && tx.send(*hk).is_err() {
                    return;
                }
                was[i] = now;
            }
        }
    });
}
