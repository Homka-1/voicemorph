//! Sound board and music player pages, and the per-sound settings window.

use crate::audio::Cmd;
use crate::state::{self, stem, Core, PlayMode, Repeat, SoundOpts, AUDIO_EXTS};
use crate::ui::{App, SoundFilter};
use crate::widgets::*;
use crate::{hotkeys, icons};
use egui::{pos2, Align2, Color32, CornerRadius, FontId, Margin, Rect, RichText, Sense, Stroke, Vec2};
use std::sync::atomic::Ordering;

impl App {
    // ------------------------------------------------------------ sound board

    pub fn page_sounds(&mut self, ui: &mut egui::Ui, c: &mut Core) {
        let mut p = c.params();
        let (vol0, eos0) = (p.sound_volume, p.effects_on_sounds);
        heading(
            ui,
            "Sound board",
            &format!("Play sounds into your microphone. Right-click a sound, or press its {} button, for its own volume, speed, loop and hotkey.", icons::SETTINGS),
        );

        // ---- toolbar
        let mode0 = c.cfg.play_mode;
        let big0 = c.cfg.big_tiles;
        card_frame(PANEL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if primary(ui, icons::ADD, "Add sounds").clicked() {
                    if let Some(files) = rfd::FileDialog::new().add_filter("Audio", &AUDIO_EXTS).pick_files() {
                        let n = c.add_sounds(files);
                        c.notify(format!("Added {n} sound(s)"));
                    }
                }
                if ui.button(format!("{}  Add folder", icons::FOLDER)).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        let n = c.add_sounds(state::list_dir(&dir, &AUDIO_EXTS));
                        c.notify(format!("Added {n} sound(s)"));
                    }
                }
                if ui.button(format!("{}  Stop all", icons::STOP)).clicked() {
                    c.shared.send(Cmd::StopSounds);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    search_box(ui, &mut self.sound_search, "Search sounds", 200.0);
                });
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("When I press a sound").color(MUTED));
                segmented(
                    ui,
                    &mut c.cfg.play_mode,
                    &[(PlayMode::Layer, "Play on top"), (PlayMode::OneAtATime, "Stop the others"), (PlayMode::Toggle, "Press again to stop")],
                );
                ui.add_space(16.0);
                ui.label(RichText::new(icons::VOLUME).color(MUTED));
                slider(ui, &mut p.sound_volume, 0.0, 1.5, 0.0, 0.8, 160.0);
                value_cell(ui, &pct(p.sound_volume));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                toggle(ui, &mut p.effects_on_sounds, "Change sounds with my voice effect");
                ui.add_space(20.0);
                toggle(ui, &mut c.cfg.big_tiles, "Big buttons");
            });
        });
        if p.sound_volume != vol0 || p.effects_on_sounds != eos0 {
            c.set_params(p);
        }
        if c.cfg.play_mode != mode0 || c.cfg.big_tiles != big0 {
            c.save();
        }
        ui.add_space(14.0);

        // ---- filters + now playing
        let playing: Vec<(String, f32)> = c.shared.playing.lock().map(|v| v.clone()).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            let favs = c.sounds.iter().filter(|s| c.opts(&s.id).fav).count();
            let mine = c.sounds.iter().filter(|s| s.user).count();
            let built = c.sounds.iter().filter(|s| s.builtin()).count();
            for (f, label) in [
                (SoundFilter::All, format!("All  {}", c.sounds.len())),
                (SoundFilter::Favorites, format!("{}  Favorites  {favs}", icons::HEART_FILL)),
                (SoundFilter::Mine, format!("Added by me  {mine}")),
                (SoundFilter::Builtin, format!("Built in  {built}")),
            ] {
                if pill(ui, &label, self.sound_filter == f) {
                    self.sound_filter = f;
                }
            }
            if !playing.is_empty() {
                ui.add_space(12.0);
                ui.label(RichText::new("Playing:").color(MUTED));
                for (id, _) in playing.iter().take(6) {
                    let name = c.sounds.iter().find(|s| &s.id == id).map(|s| c.display_name(s)).unwrap_or_else(|| "Speech".into());
                    let b = egui::Button::new(RichText::new(format!("{}  {}", icons::CLOSE, short(&name, 16))).size(12.5).color(Color32::WHITE))
                        .fill(ACCENT.gamma_multiply(0.6))
                        .corner_radius(CornerRadius::same(14));
                    if ui.add(b).on_hover_text("Stop this sound").clicked() {
                        c.shared.send(Cmd::StopSound(id.clone()));
                    }
                }
            }
        });
        ui.add_space(10.0);

        // ---- tiles
        let q = self.sound_search.to_lowercase();
        let shown: Vec<usize> = (0..c.sounds.len())
            .filter(|&i| {
                let s = &c.sounds[i];
                let o = c.opts(&s.id);
                let pass = match self.sound_filter {
                    SoundFilter::All => true,
                    SoundFilter::Favorites => o.fav,
                    SoundFilter::Mine => s.user,
                    SoundFilter::Builtin => s.builtin(),
                };
                pass && (q.is_empty() || c.display_name(s).to_lowercase().contains(&q))
            })
            .collect();
        if shown.is_empty() {
            ui.add_space(20.0);
            let msg = if c.sounds.is_empty() { "No sounds yet. Add some, or drop sound files on this window." } else { "No sounds match." };
            ui.label(RichText::new(msg).color(MUTED));
        }
        let big = c.cfg.big_tiles;
        let gap = 10.0;
        let (cols, tw) = grid_cells(ui.available_width(), if big { 214.0 } else { 180.0 }, gap);
        let tile = Vec2::new(tw, if big { 92.0 } else { 70.0 });
        let modtext = hotkeys::modifier_text(c.cfg.mod1, c.cfg.mod2);
        let mut play = None;
        let mut edit = None;
        let mut fav = None;
        egui::Grid::new("sounds").spacing(Vec2::splat(gap)).show(ui, |ui| {
            for (n, &i) in shown.iter().enumerate() {
                let s = c.sounds[i].clone();
                let o = c.opts(&s.id);
                let prog = playing.iter().find(|(id, _)| *id == s.id).map(|(_, f)| *f);
                let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
                let hovered = resp.hovered();
                let starred = c.cfg.starred.as_deref() == Some(s.id.as_str());
                let painter = ui.painter().clone();
                painter.rect_filled(rect, CornerRadius::same(12), if hovered { CARD_HOVER } else { CARD });
                if prog.is_some() {
                    painter.rect_stroke(rect, CornerRadius::same(12), Stroke::new(1.5f32, ACCENT), egui::StrokeKind::Inside);
                }
                let disc_r = if big { 20.0 } else { 16.0 };
                let disc = rect.left_center() + Vec2::new(12.0 + disc_r, 0.0);
                let disc_fill = if prog.is_some() || hovered { ACCENT } else { mix(CARD, ACCENT, 0.45) };
                painter.circle_filled(disc, disc_r, disc_fill);
                let glyph = if prog.is_some() && c.cfg.play_mode == PlayMode::Toggle { icons::STOP } else { icons::PLAY };
                painter.text(disc, Align2::CENTER_CENTER, glyph, FontId::proportional(disc_r * 0.8), Color32::WHITE);
                let tx = disc.x + disc_r + 10.0;
                let name = c.display_name(&s);
                let name = fit_text(ui, &name, &FontId::proportional(14.0), rect.right() - tx - 44.0);
                painter.text(pos2(tx, rect.center().y - 9.0), Align2::LEFT_CENTER, name, FontId::proportional(14.0), TEXT);
                let (sub, sub_col) = if o.slot != 0 && !modtext.is_empty() {
                    (format!("{}  {}+{}", icons::KEYBOARD, modtext, hotkeys::slot_key_name(o.slot)), ACCENT2)
                } else if s.builtin() {
                    ("built in".to_string(), MUTED)
                } else if s.user {
                    ("added by you".to_string(), MUTED)
                } else {
                    ("installed".to_string(), MUTED)
                };
                painter.text(pos2(tx, rect.center().y + 11.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.0), sub_col);
                // favourite heart (clickable), F12 star, loop mark
                let top = rect.top() + 15.0;
                let mut ix = rect.right() - 16.0;
                let heart_rect = Rect::from_center_size(pos2(ix, top), Vec2::splat(22.0));
                let heart = ui.interact(heart_rect, ui.id().with(("fav", &s.id)), Sense::click());
                if o.fav || hovered || heart.hovered() {
                    painter.text(heart_rect.center(), Align2::CENTER_CENTER, if o.fav { icons::HEART_FILL } else { icons::HEART }, FontId::proportional(13.0), if o.fav { PINK } else { MUTED });
                }
                if heart.on_hover_text("Favorite").clicked() {
                    fav = Some(s.id.clone());
                }
                ix -= 20.0;
                if starred {
                    painter.text(pos2(ix, top), Align2::CENTER_CENTER, icons::STAR_FILL, FontId::proportional(12.0), WARN);
                    ix -= 18.0;
                }
                if o.looped {
                    painter.text(pos2(ix, top), Align2::CENTER_CENTER, icons::REPEAT_ALL, FontId::proportional(12.0), ACCENT2);
                }
                // settings button
                let gear_rect = Rect::from_center_size(rect.right_bottom() + Vec2::new(-16.0, -15.0), Vec2::splat(22.0));
                let gear = ui.interact(gear_rect, ui.id().with(("gear", &s.id)), Sense::click());
                if hovered || gear.hovered() {
                    painter.text(gear_rect.center(), Align2::CENTER_CENTER, icons::SETTINGS, FontId::proportional(13.0), if gear.hovered() { TEXT } else { MUTED });
                }
                if gear.on_hover_text("Sound settings").clicked() {
                    edit = Some(s.id.clone());
                }
                // progress
                if let Some(f) = prog {
                    let bar = Rect::from_min_max(pos2(rect.left() + 10.0, rect.bottom() - 6.0), pos2(rect.right() - 10.0, rect.bottom() - 3.0));
                    painter.rect_filled(bar, CornerRadius::same(2), FIELD);
                    let mut fill = bar;
                    fill.set_width(bar.width() * f.clamp(0.0, 1.0));
                    painter.rect_filled(fill, CornerRadius::same(2), ACCENT2);
                }
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.secondary_clicked() {
                    edit = Some(s.id.clone());
                } else if resp.clicked() && edit.is_none() && fav.is_none() {
                    play = Some(i);
                }
                if (n + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        if let Some(id) = fav {
            let mut o = c.opts(&id);
            o.fav = !o.fav;
            c.set_opts(&id, o);
            c.save();
        } else if let Some(id) = edit {
            self.editing = Some(id);
        } else if let Some(i) = play {
            c.play_sound(i);
        }
    }

    pub fn sound_editor(&mut self, ctx: &egui::Context, c: &mut Core) {
        let Some(id) = self.editing.clone() else { return };
        let Some(item) = c.sounds.iter().find(|s| s.id == id).cloned() else {
            self.editing = None;
            return;
        };
        let mut o = c.opts(&id);
        let before = o.clone();
        let mut open = true;
        let mut close = false;
        let mut removed = false;
        let m = hotkeys::modifier_text(c.cfg.mod1, c.cfg.mod2);
        egui::Window::new(RichText::new(format!("{}  Sound settings", icons::SETTINGS)).font(semi(16.0)))
            .id(egui::Id::new("sound_editor"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .frame(egui::Frame::window(&ctx.style()).fill(PANEL).corner_radius(CornerRadius::same(16)).inner_margin(Margin::same(18)))
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(Vec2::new(130.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_width(130.0);
                        ui.label(RichText::new("Name").color(TEXT));
                    });
                    ui.add(egui::TextEdit::singleline(&mut o.name).hint_text(item.file_name.clone()).desired_width(260.0));
                });
                ui.add_space(4.0);
                slider_row(ui, "Volume", &mut o.volume, 0.0, 2.0, 0.0, 1.0, pct);
                slider_row(ui, "Speed", &mut o.speed, 0.5, 2.0, 0.05, 1.0, |v| format!("{v:.2}×"));
                ui.horizontal(|ui| {
                    ui.add_space(LABEL_W + ui.spacing().item_spacing.x);
                    for s in [0.75f32, 1.0, 1.25, 1.5] {
                        if pill(ui, &format!("{s}×"), (o.speed - s).abs() < 0.001) {
                            o.speed = s;
                        }
                    }
                });
                ui.label(RichText::new("Faster also sounds higher, slower sounds deeper.").size(11.5).color(MUTED));
                ui.add_space(4.0);
                toggle(ui, &mut o.looped, "Loop until I stop it");
                toggle(ui, &mut o.fav, "Favorite");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(Vec2::new(130.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_width(130.0);
                        ui.label(RichText::new(format!("{}  Hotkey", icons::KEYBOARD)).color(TEXT));
                    });
                    let label = |s: u8| {
                        if s == 0 {
                            "None".to_string()
                        } else if m.is_empty() {
                            format!("Slot {}", hotkeys::slot_key_name(s))
                        } else {
                            format!("{m} + {}", hotkeys::slot_key_name(s))
                        }
                    };
                    egui::ComboBox::from_id_salt("slot").selected_text(label(o.slot)).width(170.0).show_ui(ui, |ui| {
                        for s in 0..=10u8 {
                            let taken = s != 0 && c.cfg.sound_opts.iter().any(|(k, v)| *k != id && v.slot == s);
                            let text = if taken { format!("{}  (moves here)", label(s)) } else { label(s) };
                            ui.selectable_value(&mut o.slot, s, text);
                        }
                    });
                });
                if m.is_empty() {
                    ui.label(RichText::new("Hotkeys are off. Pick a modifier key in Settings to use them.").size(11.5).color(WARN));
                }
                let starred = c.cfg.starred.as_deref() == Some(id.as_str());
                ui.horizontal(|ui| {
                    if starred {
                        ui.label(RichText::new(format!("{}  This is the hotkey + F12 sound", icons::STAR_FILL)).color(WARN));
                    } else if ui.button(format!("{}  Use for hotkey + F12", icons::STAR)).clicked() {
                        c.cfg.starred = Some(id.clone());
                        c.save();
                    }
                });
                ui.add_space(10.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(format!("{}  Preview", icons::PLAY)).clicked() {
                        c.set_opts(&id, o.clone());
                        c.play_sound_id(&id);
                    }
                    if ui.button(format!("{}  Reset", icons::REFRESH)).clicked() {
                        o = SoundOpts::default();
                    }
                    if item.user && ui.add(egui::Button::new(RichText::new(format!("{}  Remove", icons::DELETE)).color(BAD))).clicked() {
                        removed = true;
                        close = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, icons::CHECK, "Done").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if removed {
            c.remove_sound(&id);
        } else if o != before {
            c.set_opts(&id, o);
            c.save();
        }
        if !open || close {
            self.editing = None;
        }
    }

    // ------------------------------------------------------------ music

    pub fn page_music(&mut self, ui: &mut egui::Ui, c: &mut Core) {
        let mut p = c.params();
        let p0 = (p.music_volume, p.effects_on_music);
        heading(ui, "Music", "Share music with the people you talk to.");
        let playing_name = c.music_index.and_then(|i| c.cfg.music.get(i)).map(|f| stem(f));
        let rate = c.rate().max(1) as f32;
        let pos = c.shared.music_pos.load(Ordering::Relaxed) as f32;
        let len = c.shared.music_len.load(Ordering::Relaxed).max(1) as f32;

        // ---- now playing
        egui::Frame::new().fill(CARD).corner_radius(CornerRadius::same(16)).inner_margin(Margin::same(18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
                let painter = ui.painter();
                painter.rect_filled(r, CornerRadius::same(14), mix(ACCENT, BG, 0.35));
                painter.text(r.center(), Align2::CENTER_CENTER, icons::MUSIC, FontId::proportional(24.0), Color32::WHITE);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.add_space(6.0);
                    let title = playing_name.clone().unwrap_or_else(|| "Nothing playing".into());
                    let room = ui.available_width() - if c.shared.music_ducking.load(Ordering::Relaxed) { 200.0 } else { 0.0 };
                    let shown = fit_text(ui, &title, &semi(18.0), room.max(80.0));
                    ui.label(RichText::new(shown).font(semi(18.0)).color(TEXT)).on_hover_text(title);
                    let mut info = Vec::new();
                    if let Some(i) = c.music_index {
                        info.push(format!("Song {} of {}", i + 1, c.cfg.music.len()));
                    } else {
                        let n = c.cfg.music.len();
                        info.push(format!("{n} song{} in the playlist", if n == 1 { "" } else { "s" }));
                    }
                    if c.music_paused {
                        info.push("Paused".into());
                    }
                    if c.music_loading {
                        info.push("Loading…".into());
                    }
                    if (c.cfg.music_speed - 1.0).abs() > 0.001 {
                        info.push(format!("{:.2}× speed", c.cfg.music_speed));
                    }
                    if !c.cfg.music_to_call {
                        info.push("Only you hear it".into());
                    }
                    ui.label(RichText::new(info.join("  ·  ")).size(12.5).color(MUTED));
                });
                if c.shared.music_ducking.load(Ordering::Relaxed) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::Frame::new().fill(ACCENT2.gamma_multiply(0.18)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
                            ui.label(RichText::new(format!("{}  Lowered while you talk", icons::MIC)).size(12.0).color(ACCENT2));
                        });
                    });
                }
            });
            ui.add_space(10.0);

            // seek bar: click or drag to jump
            let active = playing_name.is_some();
            let frac = if active { (pos / len).clamp(0.0, 1.0) } else { 0.0 };
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), if active { Sense::click_and_drag() } else { Sense::hover() });
            let rail = Rect::from_min_max(pos2(rect.left(), rect.center().y - 3.0), pos2(rect.right(), rect.center().y + 3.0));
            let shown = match resp.interact_pointer_pos().filter(|_| resp.is_pointer_button_down_on()) {
                Some(pp) => {
                    let f = ((pp.x - rail.left()) / rail.width()).clamp(0.0, 1.0);
                    self.seek_drag = Some(f);
                    f
                }
                None => {
                    if let Some(f) = self.seek_drag.take() {
                        c.shared.send(Cmd::SeekMusic(f as f64));
                        f
                    } else {
                        frac
                    }
                }
            };
            let painter = ui.painter();
            painter.rect_filled(rail, CornerRadius::same(3), FIELD);
            let mut done = rail;
            done.set_width(rail.width() * shown);
            painter.rect_filled(done, CornerRadius::same(3), ACCENT);
            if active {
                let x = rail.left() + rail.width() * shown;
                let grow = resp.hovered() || self.seek_drag.is_some();
                painter.circle(pos2(x, rail.center().y), if grow { 8.0 } else { 6.0 }, Color32::WHITE, Stroke::new(2.0f32, ACCENT));
                if let Some(hp) = resp.hover_pos() {
                    let f = ((hp.x - rail.left()) / rail.width()).clamp(0.0, 1.0);
                    resp.clone().on_hover_text_at_pointer(mmss(f * len / rate));
                }
            }
            ui.horizontal(|ui| {
                let secs = if active { shown * len / rate } else { 0.0 };
                ui.label(RichText::new(mmss(secs)).size(12.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = if active { format!("-{}", mmss((len / rate - secs) / c.cfg.music_speed.max(0.01))) } else { "0:00".into() };
                    ui.label(RichText::new(text).size(12.0).color(MUTED));
                });
            });
            ui.add_space(4.0);

            // transport
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if icon_button(ui, icons::SHUFFLE, if c.cfg.shuffle { "Shuffle is on" } else { "Shuffle is off" }, c.cfg.shuffle).clicked() {
                    c.cfg.shuffle = !c.cfg.shuffle;
                    c.save();
                }
                if icon_button(ui, icons::PREV, "Previous song (or start of this one)", false).clicked() {
                    c.prev_track();
                }
                if icon_button(ui, icons::BACK10, "Back 10 seconds", false).clicked() {
                    c.skip_seconds(-10.0);
                }
                let is_playing = c.music_index.is_some() && !c.music_paused;
                let b = egui::Button::new(RichText::new(if is_playing { icons::PAUSE } else { icons::PLAY }).size(20.0).color(Color32::WHITE))
                    .fill(ACCENT)
                    .corner_radius(CornerRadius::same(24))
                    .min_size(Vec2::splat(48.0));
                if ui.add(b).on_hover_text(if is_playing { "Pause" } else { "Play" }).clicked() {
                    c.toggle_play(self.music_sel);
                }
                if icon_button(ui, icons::FWD30, "Forward 30 seconds", false).clicked() {
                    c.skip_seconds(30.0);
                }
                if icon_button(ui, icons::NEXT, "Next song", false).clicked() {
                    c.next_track(false);
                }
                let (ricon, rtip) = match c.cfg.repeat {
                    Repeat::Off => (icons::REPEAT_ALL, "Repeat is off"),
                    Repeat::All => (icons::REPEAT_ALL, "Repeat the playlist"),
                    Repeat::One => (icons::REPEAT_ONE, "Repeat this song"),
                };
                if icon_button(ui, ricon, rtip, c.cfg.repeat != Repeat::Off).clicked() {
                    c.cfg.repeat = match c.cfg.repeat {
                        Repeat::Off => Repeat::All,
                        Repeat::All => Repeat::One,
                        Repeat::One => Repeat::Off,
                    };
                    c.save();
                }
                if icon_button(ui, icons::STOP, "Stop", false).clicked() {
                    c.stop_music();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.allocate_ui_with_layout(Vec2::new(42.0, 28.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.set_min_width(42.0);
                        ui.label(RichText::new(pct(p.music_volume)).size(13.0).color(MUTED));
                    });
                    slider(ui, &mut p.music_volume, 0.0, 1.5, 0.0, 0.5, 160.0);
                    let muted = p.music_volume <= 0.0001;
                    if icon_button(ui, if muted { icons::MUTE } else { icons::VOLUME }, if muted { "Unmute" } else { "Mute" }, false).clicked() {
                        p.music_volume = if muted { 0.5 } else { 0.0 };
                    }
                });
            });
        });
        ui.add_space(12.0);

        // ---- advanced options
        card_frame(PANEL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let chevron = if self.music_adv { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT };
            let head = ui.add(egui::Label::new(RichText::new(format!("{chevron}   {}  Advanced music options", icons::EQUALIZER)).font(semi(15.0)).color(TEXT)).sense(Sense::click()));
            if head.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.music_adv = !self.music_adv;
            }
            if !self.music_adv {
                return;
            }
            ui.add_space(8.0);
            let mut ch = false;
            ui.columns(2, |cols| {
                let (l, r) = cols.split_at_mut(1);
                let l = &mut l[0];
                let r = &mut r[0];
                l.label(RichText::new(format!("{}  Playback", icons::SPEED)).color(MUTED));
                ch |= slider_row(l, "Speed", &mut c.cfg.music_speed, 0.5, 2.0, 0.05, 1.0, |v| format!("{v:.2}×"));
                l.horizontal(|ui| {
                    ui.add_space(LABEL_W + ui.spacing().item_spacing.x);
                    for s in [0.75f32, 1.0, 1.25, 1.5] {
                        if pill(ui, &format!("{s}×"), (c.cfg.music_speed - s).abs() < 0.001) {
                            c.cfg.music_speed = s;
                            ch = true;
                        }
                    }
                });
                ch |= slider_row(l, "Cross-fade", &mut c.cfg.music_fade, 0.0, 8.0, 0.5, 0.0, |v| if v <= 0.0 { "off".into() } else { format!("{v:.1} s") });
                l.label(RichText::new("Songs fade into each other, and Stop fades out.").size(11.5).color(MUTED));
                r.label(RichText::new(format!("{}  Music sound", icons::EQUALIZER)).color(MUTED));
                ch |= slider_row(r, "Bass", &mut c.cfg.music_bass, -12.0, 12.0, 0.5, 0.0, db);
                ch |= slider_row(r, "Treble", &mut c.cfg.music_treble, -12.0, 12.0, 0.5, 0.0, db);
                r.label(RichText::new("Double-click any slider to reset it.").size(11.5).color(MUTED));
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(RichText::new(format!("{}  Who hears it", icons::PEOPLE)).color(MUTED));
            ui.add_space(2.0);
            ui.columns(2, |cols| {
                ch |= toggle(&mut cols[0], &mut c.cfg.music_to_call, "People I talk to hear the music");
                if toggle(&mut cols[0], &mut c.cfg.hear_sounds, "I hear it too") {
                    c.apply_listen_opts();
                    ch = true;
                }
                cols[1].add_enabled_ui(c.cfg.music_to_call, |ui| {
                    toggle(ui, &mut p.effects_on_music, "Change the music with my voice effect");
                });
                ch |= toggle(&mut cols[1], &mut c.cfg.duck, "Lower the music while I talk");
            });
            if c.cfg.hear_sounds && c.cfg.monitor.is_empty() {
                ui.label(RichText::new(format!("{}  To hear it yourself, pick a device in Settings › Devices › \"Let me hear it on\".", icons::INFO)).size(11.5).color(WARN));
            }
            if c.cfg.duck {
                ui.add_space(2.0);
                ch |= slider_row(ui, "Level while I talk", &mut c.cfg.duck_level, 0.0, 1.0, 0.05, 0.3, pct);
            }
            if ch {
                c.apply_music_opts();
                c.save();
            }
        });
        if (p.music_volume, p.effects_on_music) != p0 {
            c.set_params(p);
        }
        ui.add_space(12.0);

        // ---- playlist
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("Playlist  ·  {} song{}", c.cfg.music.len(), if c.cfg.music.len() == 1 { "" } else { "s" })).font(semi(16.0)).color(TEXT));
            ui.add_space(10.0);
            search_box(ui, &mut self.music_search, "Search songs", 180.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary(ui, icons::ADD, "Add songs").clicked() {
                    if let Some(files) = rfd::FileDialog::new().add_filter("Audio", &AUDIO_EXTS).pick_files() {
                        let n = c.add_music(files);
                        c.notify(format!("Added {n} song(s)"));
                    }
                }
                if ui.button(format!("{}  Add folder", icons::FOLDER)).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        let n = c.add_music(state::list_dir(&dir, &AUDIO_EXTS));
                        c.notify(format!("Added {n} song(s)"));
                    }
                }
                if ui.button(format!("{}  A–Z", icons::SORT)).on_hover_text("Sort by name").clicked() {
                    c.sort_music();
                    self.music_sel = None;
                }
                if !c.cfg.music.is_empty() && ui.button(format!("{}  Clear", icons::CLEAR)).on_hover_text("Remove every song from the playlist").clicked() {
                    c.clear_music();
                    self.music_sel = None;
                }
            });
        });
        ui.add_space(6.0);
        if c.cfg.music.is_empty() {
            ui.label(RichText::new("Your playlist is empty. Add songs, or drop music files on this window.").color(MUTED));
        }
        let q = self.music_search.to_lowercase();
        let mut action: Option<(&str, usize)> = None;
        let n = c.cfg.music.len();
        for i in 0..n {
            let name = stem(&c.cfg.music[i]);
            if !q.is_empty() && !name.to_lowercase().contains(&q) {
                continue;
            }
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
            let now = c.music_index == Some(i);
            let hovered = resp.hovered();
            let fill = if self.music_sel == Some(i) {
                ACCENT.gamma_multiply(0.22)
            } else if now {
                ACCENT.gamma_multiply(0.12)
            } else if hovered {
                CARD
            } else if i % 2 == 0 {
                PANEL
            } else {
                Color32::TRANSPARENT
            };
            let painter = ui.painter().clone();
            painter.rect_filled(rect, CornerRadius::same(8), fill);
            if now {
                let icon = if c.music_paused { icons::PAUSE } else { icons::VOLUME };
                painter.text(rect.left_center() + Vec2::new(22.0, 0.0), Align2::CENTER_CENTER, icon, FontId::proportional(14.0), ACCENT2);
            } else {
                painter.text(rect.left_center() + Vec2::new(22.0, 0.0), Align2::CENTER_CENTER, format!("{}", i + 1), FontId::proportional(12.0), MUTED);
            }
            let name = fit_text(ui, &name, &FontId::proportional(14.0), rect.width() - 46.0 - 150.0);
            painter.text(rect.left_center() + Vec2::new(46.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(14.0), if now { Color32::WHITE } else { TEXT });
            if hovered || self.music_sel == Some(i) {
                let buttons = [(icons::DELETE, "remove", "Remove"), (icons::DOWN, "down", "Move down"), (icons::UP, "up", "Move up"), (icons::PLAY, "play", "Play")];
                let mut x = rect.right() - 20.0;
                for (icon, act, tip) in buttons {
                    let r = Rect::from_center_size(pos2(x, rect.center().y), Vec2::splat(28.0));
                    let b = ui.interact(r, ui.id().with(("pl", act, i)), Sense::click());
                    if b.hovered() {
                        painter.rect_filled(r, CornerRadius::same(8), BUTTON_HOVER);
                    }
                    let col = if act == "remove" && b.hovered() { BAD } else if b.hovered() { TEXT } else { MUTED };
                    painter.text(r.center(), Align2::CENTER_CENTER, icon, FontId::proportional(13.0), col);
                    if b.on_hover_text(tip).clicked() {
                        action = Some((act, i));
                    }
                    x -= 32.0;
                }
            }
            if action.is_none() {
                if resp.double_clicked() {
                    action = Some(("play", i));
                } else if resp.clicked() {
                    self.music_sel = Some(i);
                }
            }
        }
        match action {
            Some(("play", i)) => {
                self.music_sel = Some(i);
                c.play_track(i);
            }
            Some(("up", i)) if i > 0 => {
                c.move_track(i, i - 1);
                self.music_sel = Some(i - 1);
            }
            Some(("down", i)) if i + 1 < n => {
                c.move_track(i, i + 1);
                self.music_sel = Some(i + 1);
            }
            Some(("remove", i)) => {
                c.remove_track(i);
                self.music_sel = None;
            }
            _ => {}
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Double-click a song to play it. Hotkey + F11 plays / skips, + F10 stops.").size(11.5).color(MUTED));
    }
}
