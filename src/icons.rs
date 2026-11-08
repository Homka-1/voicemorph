//! Fonts and icons. Windows' own UI font (Segoe UI) for text, and its icon font
//! (Segoe Fluent Icons on Windows 11, Segoe MDL2 Assets on Windows 10; same code
//! points) for every icon, so no icon falls back to an empty box. Symbol and emoji
//! fonts are added after them for anything else.

use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

pub const PLAY: &str = "\u{E768}";
pub const PAUSE: &str = "\u{E769}";
pub const STOP: &str = "\u{E71A}";
pub const PREV: &str = "\u{E892}";
pub const NEXT: &str = "\u{E893}";
pub const BACK10: &str = "\u{ED3C}";
pub const FWD30: &str = "\u{ED3D}";
pub const VOLUME: &str = "\u{E767}";
pub const MUTE: &str = "\u{E74F}";
pub const SHUFFLE: &str = "\u{E8B1}";
pub const REPEAT_ALL: &str = "\u{E8EE}";
pub const REPEAT_ONE: &str = "\u{E8ED}";
pub const ADD: &str = "\u{E710}";
pub const DELETE: &str = "\u{E74D}";
pub const CLOSE: &str = "\u{E711}";
pub const SETTINGS: &str = "\u{E713}";
pub const MIC: &str = "\u{E720}";
pub const MUSIC: &str = "\u{EC4F}";
pub const FACE: &str = "\u{E76E}";
pub const SOUNDS: &str = "\u{E7F5}";
pub const SPEAK: &str = "\u{E8BD}";
pub const STAR: &str = "\u{E734}";
pub const STAR_FILL: &str = "\u{E735}";
pub const HEART: &str = "\u{EB51}";
pub const HEART_FILL: &str = "\u{EB52}";
pub const UP: &str = "\u{E74A}";
pub const DOWN: &str = "\u{E74B}";
pub const REFRESH: &str = "\u{E72C}";
pub const SEARCH: &str = "\u{E721}";
pub const FOLDER: &str = "\u{E8B7}";
pub const POWER: &str = "\u{E7E8}";
pub const HEADPHONE: &str = "\u{E7F6}";
pub const KEYBOARD: &str = "\u{E765}";
pub const INFO: &str = "\u{E946}";
pub const WARNING: &str = "\u{E7BA}";
pub const SORT: &str = "\u{E8CB}";
pub const OPEN: &str = "\u{E8A7}";
pub const CHECK: &str = "\u{E73E}";
pub const CLEAR: &str = "\u{E894}";
pub const SPEED: &str = "\u{EC4A}";
pub const EQUALIZER: &str = "\u{E9E9}";
pub const PEOPLE: &str = "\u{E716}";
pub const SYNC: &str = "\u{E895}";
pub const TRAY: &str = "\u{E8A0}";
pub const CHEVRON_DOWN: &str = "\u{E70D}";
pub const CHEVRON_RIGHT: &str = "\u{E76C}";
pub const APPS: &str = "\u{E71D}";
pub const APP_WINDOW: &str = "\u{E737}";

pub const SEMIBOLD: &str = "semibold";

fn read(name: &str) -> Option<Vec<u8>> {
    let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
    std::fs::read(dir.join("Fonts").join(name)).ok()
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |key: &str, file: &str| -> bool {
        match read(file) {
            Some(b) => {
                fonts.font_data.insert(key.to_owned(), Arc::new(FontData::from_owned(b)));
                true
            }
            None => false,
        }
    };
    let has_text = add("segoe", "segoeui.ttf");
    let has_semi = add("segoe-semi", "seguisb.ttf");
    let has_fluent = add("fluent", "SegoeIcons.ttf");
    let has_mdl2 = add("mdl2", "segmdl2.ttf");
    let has_sym = add("symbols", "seguisym.ttf");
    let has_emoji = add("emoji", "seguiemj.ttf");

    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut icons_and_fallbacks = Vec::new();
    if has_fluent {
        icons_and_fallbacks.push("fluent".to_owned());
    }
    if has_mdl2 {
        icons_and_fallbacks.push("mdl2".to_owned());
    }
    let mut tail = defaults.clone();
    if has_sym {
        tail.push("symbols".to_owned());
    }
    if has_emoji {
        tail.push("emoji".to_owned());
    }

    let mut prop = Vec::new();
    if has_text {
        prop.push("segoe".to_owned());
    }
    prop.extend(icons_and_fallbacks.iter().cloned());
    prop.extend(tail.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, prop.clone());

    let mut semi = Vec::new();
    if has_semi {
        semi.push("segoe-semi".to_owned());
    }
    semi.extend(prop.iter().cloned());
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), semi);

    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.extend(icons_and_fallbacks);
        if has_sym {
            mono.push("symbols".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}
