//! The "App sound" page: pick an app (a browser, a game, a player) and its sound
//! plays into your microphone.

use crate::apploop;
use crate::state::Core;
use crate::ui::App;
use crate::widgets::*;
use crate::icons;
use egui::{pos2, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Vec2};

/// A steady colour per app, for its letter badge.
fn badge_color(name: &str) -> Color32 {
    const COLS: [Color32; 6] = [ACCENT, ACCENT2, GOOD, PINK, WARN, Color32::from_rgb(0x60, 0xa5, 0xfa)];
    let h = name.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b.to_ascii_lowercase() as u32));
    COLS[(h % COLS.len() as u32) as usize]
}

fn letter_badge(ui: &egui::Ui, r: Rect, name: &str, size: f32) {
    let col = badge_color(name);
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same((r.width() * 0.28) as u8), col.gamma_multiply(0.22));
    let letter: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    p.text(r.center(), Align2::CENTER_CENTER, letter, semi(size), col);
}

impl App {
    pub fn page_apps(&mut self, ui: &mut egui::Ui, c: &mut Core) {
        if !self.apps_loaded {
            self.apps_loaded = true;
            c.refresh_apps();
        }
        heading(ui, "App sound", "Play another app's sound into your microphone: a browser, a game or a music app. You keep hearing it from the app as usual.");
        let mut restart = false;
        let mut save = false;

        // ---- the chosen app
        let chosen = (!c.cfg.app_exe.is_empty()).then(|| apploop::pretty_name(&c.cfg.app_exe));
        egui::Frame::new().fill(CARD).corner_radius(CornerRadius::same(16)).inner_margin(egui::Margin::same(18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
                match &chosen {
                    Some(n) => letter_badge(ui, r, n, 24.0),
                    None => {
                        ui.painter().rect_filled(r, CornerRadius::same(14), FIELD);
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, icons::APPS, FontId::proportional(22.0), MUTED);
                    }
                }
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.add_space(4.0);
                    ui.label(RichText::new(chosen.clone().unwrap_or_else(|| "No app chosen".into())).font(semi(18.0)).color(TEXT));
                    let (msg, ok) = if c.cfg.app_on { c.app_status.clone() } else if chosen.is_some() { ("Off. Turn it on to share this app's sound.".into(), false) } else { ("Pick an app from the list below.".into(), false) };
                    ui.horizontal(|ui| {
                        let (d, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                        ui.painter().circle_filled(d.center(), 4.0, if ok { GOOD } else if c.cfg.app_on { WARN } else { FIELD_STROKE });
                        ui.label(RichText::new(msg).size(12.5).color(MUTED));
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.add_enabled_ui(chosen.is_some(), |ui| {
                        if big_switch(ui, c.cfg.app_on) {
                            c.cfg.app_on = !c.cfg.app_on;
                            restart = true;
                        }
                    });
                    if c.app_capturing() {
                        level_bar(ui, c.shared.app.level.get(), 120.0);
                    }
                });
            });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            if slider_row(ui, "Volume", &mut c.cfg.app_volume, 0.0, 2.0, 0.0, 1.0, pct) {
                save = true;
            }
            if toggle(ui, &mut c.cfg.app_fx, "Change it with my voice effect") {
                save = true;
            }
            ui.label(RichText::new("Off: people hear the app exactly as it sounds. On: it goes through your voice, room and vocoder like your microphone.").size(11.5).color(MUTED));
        });
        ui.add_space(16.0);

        // ---- the list
        let mut pick: Option<String> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Choose an app").font(semi(16.0)).color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("{}  Refresh", icons::REFRESH)).on_hover_text("Look for open apps again").clicked() {
                    c.refresh_apps();
                }
                search_box(ui, &mut self.app_search, "Search apps", 170.0);
            });
        });
        ui.add_space(8.0);
        let q = self.app_search.to_lowercase();
        let matches = |a: &apploop::AudioApp| q.is_empty() || a.name.to_lowercase().contains(&q) || a.exe.to_lowercase().contains(&q);
        let playing: Vec<apploop::AudioApp> = c.apps.iter().filter(|a| a.has_audio && matches(a)).cloned().collect();
        let others: Vec<apploop::AudioApp> = c.apps.iter().filter(|a| !a.has_audio && matches(a)).cloned().collect();
        let gap = 10.0;
        let tiles = |ui: &mut egui::Ui, list: &[apploop::AudioApp], pick: &mut Option<String>, id: &str| {
            let (cols, w) = grid_cells(ui.available_width(), 220.0, gap);
            egui::Grid::new(id).spacing(Vec2::splat(gap)).show(ui, |ui| {
                for (n, a) in list.iter().enumerate() {
                    let sel = a.exe.eq_ignore_ascii_case(&c.cfg.app_exe);
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 56.0), Sense::click());
                    let hovered = resp.hovered();
                    let p = ui.painter();
                    p.rect_filled(rect, CornerRadius::same(12), if sel { ACCENT.gamma_multiply(0.22) } else if hovered { CARD_HOVER } else { CARD });
                    if sel {
                        p.rect_stroke(rect, CornerRadius::same(12), Stroke::new(1.5f32, ACCENT), egui::StrokeKind::Inside);
                    }
                    let b = Rect::from_center_size(rect.left_center() + Vec2::new(30.0, 0.0), Vec2::splat(34.0));
                    letter_badge(ui, b, &a.name, 15.0);
                    let tx = b.right() + 12.0;
                    let right_room = if sel || a.has_audio { 34.0 } else { 12.0 };
                    let font = FontId::proportional(14.0);
                    let name = fit_text(ui, &a.name, &font, rect.right() - tx - right_room);
                    let p = ui.painter();
                    p.text(pos2(tx, rect.center().y - 8.0), Align2::LEFT_CENTER, name, font, TEXT);
                    let sub = fit_text(ui, &a.exe.to_lowercase(), &FontId::proportional(11.0), rect.right() - tx - right_room);
                    p.text(pos2(tx, rect.center().y + 10.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.0), MUTED);
                    let mark = rect.right_center() - Vec2::new(20.0, 0.0);
                    if sel {
                        p.text(mark, Align2::CENTER_CENTER, icons::CHECK, FontId::proportional(14.0), ACCENT);
                    } else if a.has_audio {
                        p.text(mark, Align2::CENTER_CENTER, icons::VOLUME, FontId::proportional(13.0), GOOD);
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *pick = Some(a.exe.clone());
                    }
                    if (n + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
        };
        if c.apps.is_empty() {
            ui.label(RichText::new("No apps found. Open the app you want to share, then press Refresh.").color(MUTED));
        }
        if !playing.is_empty() {
            ui.label(RichText::new(format!("{}  Apps with sound", icons::VOLUME)).size(12.5).color(MUTED));
            ui.add_space(2.0);
            tiles(ui, &playing, &mut pick, "apps_audio");
            ui.add_space(14.0);
        }
        if !others.is_empty() {
            ui.label(RichText::new(format!("{}  Other open apps", icons::APP_WINDOW)).size(12.5).color(MUTED));
            ui.add_space(2.0);
            tiles(ui, &others, &mut pick, "apps_other");
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new("Tip: for a browser, every tab is included. If you also turn on \"Hear my changed voice\" in Settings, you'll hear the app twice.")
                .size(11.5)
                .color(MUTED),
        );

        if let Some(exe) = pick {
            if !exe.eq_ignore_ascii_case(&c.cfg.app_exe) || !c.cfg.app_on {
                c.cfg.app_exe = exe;
                c.cfg.app_on = true;
                restart = true;
            }
        }
        if restart {
            c.restart_app_capture();
            save = true;
        }
        if save {
            c.apply_app_opts();
            c.save();
        }
    }
}

/// A larger on/off switch for the main thing on a page. Returns true when clicked.
pub fn big_switch(ui: &mut egui::Ui, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(58.0, 32.0), Sense::click());
    let t = ui.ctx().animate_bool(resp.id, on);
    let enabled = ui.is_enabled();
    let mut fill = mix(FIELD, ACCENT, t);
    if !enabled {
        fill = fill.gamma_multiply(0.5);
    }
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(16), fill);
    let x = egui::lerp(rect.left() + 16.0..=rect.right() - 16.0, t);
    p.circle_filled(pos2(x, rect.center().y), if resp.hovered() { 12.5 } else { 12.0 }, if enabled { Color32::WHITE } else { MUTED });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && enabled
}

/// A plain level bar (how loud the app is right now).
pub fn level_bar(ui: &mut egui::Ui, level: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(4), FIELD);
    let dbv = 20.0 * level.max(1e-5).log10();
    let frac = ((dbv + 60.0) / 60.0).clamp(0.0, 1.0);
    let mut f = rect;
    f.set_width(rect.width() * frac);
    p.rect_filled(f, CornerRadius::same(4), GOOD);
}
