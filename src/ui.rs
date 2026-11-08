//! The window: top bar, side bar, the Voices / Speak / Settings pages, closing to
//! the tray and the --screenshot option. Sound board and music pages are in media.rs.

use crate::audio::{AudioSystem, Cmd};
use crate::dsp::{self, CarrierKind, Room, Voice};
use crate::state::{self, stem, Core, AUDIO_EXTS};
use crate::widgets::*;
use crate::{hotkeys, icons, tray, winaudio};
use egui::{pos2, Align2, Color32, CornerRadius, FontId, Margin, Rect, RichText, Sense, Stroke, Vec2};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Voice,
    Sounds,
    Music,
    Apps,
    Speak,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SoundFilter {
    All,
    Favorites,
    Mine,
    Builtin,
}

pub struct App {
    pub core: Arc<Mutex<Core>>,
    pub audio: Option<AudioSystem>,
    pub audio_error: Option<String>,
    pub page: Page,
    pub music_sel: Option<usize>,
    pub music_search: String,
    pub music_adv: bool,
    pub seek_drag: Option<f32>,
    pub sound_search: String,
    pub sound_filter: SoundFilter,
    pub editing: Option<String>,
    pub tts_text: String,
    pub app_search: String,
    /// The app list was read for this visit to the App sound page.
    pub apps_loaded: bool,
    shot: Option<PathBuf>,
    frame: u32,
    shot_sent: bool,
    hid_at_start: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, page: Page) -> Self {
        icons::install_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        let mut core = Core::new(state::load_config());
        core.refresh_devices();
        if shot.is_none() {
            core.apply_route();
        }
        let core = Arc::new(Mutex::new(core));
        let tray_tx = state::spawn_brain(core.clone(), cc.egui_ctx.clone());
        if shot.is_none() {
            #[cfg(windows)]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Ok(h) = cc.window_handle() {
                    if let RawWindowHandle::Win32(w) = h.as_raw() {
                        tray::set_main_window(w.hwnd.get());
                    }
                }
            }
            tray::start(tray_tx, cc.egui_ctx.clone());
        }
        let mut app = App {
            core,
            audio: None,
            audio_error: None,
            page,
            music_sel: None,
            music_search: String::new(),
            music_adv: true,
            seek_drag: None,
            sound_search: String::new(),
            sound_filter: SoundFilter::All,
            editing: None,
            tts_text: "Hello! This is my new voice.".into(),
            app_search: String::new(),
            apps_loaded: false,
            shot,
            frame: 0,
            shot_sent: false,
            hid_at_start: false,
        };
        app.restart_audio();
        if std::env::args().any(|a| a == "--mic-page") {
            app.page = Page::Settings;
        }
        app
    }

    pub fn restart_audio(&mut self) {
        self.audio = None;
        let core = self.core.clone();
        let mut c = core.lock().unwrap();
        match AudioSystem::start(&c.cfg.input, &c.cfg.output, &c.cfg.monitor, c.shared.clone()) {
            Ok(a) => {
                self.audio = Some(a);
                self.audio_error = None;
            }
            Err(e) => self.audio_error = Some(e),
        }
        c.audio_restarted();
    }

    // ------------------------------------------------------------ frame pieces

    fn top_bar(&mut self, ui: &mut egui::Ui, c: &mut Core) {
        let mut p = c.params();
        let was_on = p.enabled;
        let mut go_music = false;
        ui.horizontal_centered(|ui| {
            ui.set_height(48.0);
            let (r, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(r, CornerRadius::same(10), ACCENT);
            painter.text(r.center(), Align2::CENTER_CENTER, icons::MIC, FontId::proportional(17.0), Color32::WHITE);
            ui.add_space(4.0);
            // title and subtitle as one painted block, so both lines sit centred in the bar
            let (tr, _) = ui.allocate_exact_size(Vec2::new(170.0, 40.0), Sense::hover());
            ui.painter().text(tr.left_center() - Vec2::new(0.0, 9.0), Align2::LEFT_CENTER, "VoiceMorph", semi(17.0), TEXT);
            ui.painter().text(tr.left_center() + Vec2::new(0.0, 11.0), Align2::LEFT_CENTER, "Real-time voice changer", FontId::proportional(12.0), MUTED);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                // power switch
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(112.0, 36.0), Sense::click());
                let on = p.enabled;
                let t = ui.ctx().animate_bool(resp.id, on);
                let fill = mix(BUTTON, ACCENT, t);
                let pp = ui.painter();
                pp.rect_filled(rect, CornerRadius::same(18), if resp.hovered() { mix(fill, Color32::WHITE, 0.08) } else { fill });
                let knob_x = egui::lerp(rect.left() + 18.0..=rect.right() - 18.0, t);
                pp.circle_filled(pos2(knob_x, rect.center().y), 13.0, Color32::WHITE);
                pp.text(pos2(knob_x, rect.center().y), Align2::CENTER_CENTER, icons::POWER, FontId::proportional(13.0), if on { ACCENT } else { MUTED });
                pp.text(
                    if on { rect.left_center() + Vec2::new(16.0, 0.0) } else { rect.right_center() - Vec2::new(16.0, 0.0) },
                    if on { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER },
                    if on { "ON" } else { "OFF" },
                    semi(14.0),
                    if on { Color32::WHITE } else { MUTED },
                );
                if resp.on_hover_text("Turn voice changing on or off (hotkey + F4)").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    p.enabled = !p.enabled;
                }
                ui.add_space(8.0);
                meter(ui, "OUT", c.shared.out_level.get(), ACCENT2);
                meter(ui, "MIC", c.shared.in_level.get(), GOOD);
                ui.add_space(8.0);
                let name = if p.voice == Voice::Chainer {
                    let names: Vec<&str> = p.chain.iter().filter(|v| **v != Voice::None).map(|v| v.name()).collect();
                    format!("Chain: {}", names.join(" › "))
                } else {
                    p.voice.name().to_string()
                };
                let vr = bar_pill(ui, p.voice.icon(), ACCENT, &name, 220.0, CARD);
                if vr.on_hover_text("Your voice now. Click to pick another.").clicked() {
                    self.page = Page::Voice;
                }
                if c.app_capturing() {
                    let n = crate::apploop::pretty_name(&c.cfg.app_exe);
                    if bar_pill(ui, icons::APPS, GOOD, &n, 160.0, CARD).on_hover_text("This app's sound goes into your microphone").clicked() {
                        self.page = Page::Apps;
                    }
                }
                if let Some(i) = c.music_index {
                    let title = c.cfg.music.get(i).map(|f| stem(f)).unwrap_or_default();
                    // the song gets all the room that's left
                    let room = ui.available_width() - 16.0;
                    if room > 90.0 {
                        let icon = if c.music_paused { icons::PAUSE } else { icons::MUSIC };
                        let r = bar_pill(ui, icon, ACCENT2, &title, room, Color32::TRANSPARENT);
                        if r.on_hover_text(title.clone()).clicked() {
                            go_music = true;
                        }
                    }
                }
            });
        });
        if go_music {
            self.page = Page::Music;
        }
        if p.enabled != was_on {
            c.set_params(p);
        }
    }

    fn side_bar(&mut self, ui: &mut egui::Ui, c: &mut Core) {
        ui.add_space(6.0);
        let items = [
            (Page::Voice, icons::FACE, "Voices"),
            (Page::Sounds, icons::SOUNDS, "Sound board"),
            (Page::Music, icons::MUSIC, "Music"),
            (Page::Apps, icons::APPS, "App sound"),
            (Page::Speak, icons::SPEAK, "Speak text"),
            (Page::Settings, icons::SETTINGS, "Settings"),
        ];
        for (page, icon, label) in items {
            let sel = self.page == page;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
            let fill = if sel {
                ACCENT.gamma_multiply(0.25)
            } else if resp.hovered() {
                CARD
            } else {
                Color32::TRANSPARENT
            };
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::same(10), fill);
            if sel {
                painter.rect_filled(Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 11.0), Vec2::new(3.0, 18.0)), CornerRadius::same(2), ACCENT);
            }
            painter.text(rect.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(15.0), if sel { TEXT } else { MUTED });
            painter.text(rect.left_center() + Vec2::new(44.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(14.0), if sel { TEXT } else { MUTED });
            // small live dots: music playing, app sound on
            let live = match page {
                Page::Music => c.music_index.is_some() && !c.music_paused,
                Page::Apps => c.app_capturing(),
                _ => false,
            };
            if live {
                painter.circle_filled(rect.right_center() - Vec2::new(14.0, 0.0), 3.5, if page == Page::Apps { GOOD } else { ACCENT2 });
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.page = page;
            }
            ui.add_space(2.0);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(4.0);
            // audio status: one short line per device, the full name on hover
            let (ok, title, lines): (bool, String, Vec<(&str, String)>) = match (&self.audio, &self.audio_error) {
                (Some(a), _) => (true, "Audio running".into(), a.lines.clone()),
                (None, Some(e)) => (false, "Audio stopped".into(), vec![("", e.clone())]),
                _ => (false, "Starting audio…".into(), Vec::new()),
            };
            egui::Frame::new().fill(CARD.gamma_multiply(0.6)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let w = ui.available_width();
                let rows = 1 + lines.len() + usize::from(!hotkeys::modifier_text(c.cfg.mod1, c.cfg.mod2).is_empty());
                let est = rows as f32 * 18.0 - 3.0;
                ui.allocate_ui_with_layout(Vec2::new(w, est), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, if ok { GOOD } else { BAD });
                    ui.label(RichText::new(title).size(12.0).color(TEXT));
                });
                let font = FontId::proportional(11.0);
                let w = ui.available_width();
                for (k, v) in lines {
                    let full = if k.is_empty() { v.clone() } else { format!("{k}: {v}") };
                    let shown = fit_text(ui, &full, &font, w);
                    let r = ui.label(RichText::new(shown).font(font.clone()).color(MUTED));
                    r.on_hover_text(full);
                }
                let m = hotkeys::modifier_text(c.cfg.mod1, c.cfg.mod2);
                if !m.is_empty() {
                    ui.label(RichText::new(format!("{}  {m} + F4 on / off", icons::KEYBOARD)).font(font.clone()).color(MUTED));
                }
                });
            });
        });
    }

    // ------------------------------------------------------------ voices

    fn page_voice(&mut self, ui: &mut egui::Ui, c: &mut Core) {
        let mut p = c.params();
        let before = format!("{p:?}");
        heading(ui, "Voices", "Pick how you sound. Click a card to switch instantly.");
        let gap = 10.0;
        let (cols, cw) = grid_cells(ui.available_width(), 112.0, gap);
        let card = Vec2::new(cw, 90.0);
        let special = |v: Voice| matches!(v, Voice::Custom | Voice::Chainer | Voice::Silence);
        let regular: Vec<Voice> = Voice::ALL.iter().copied().filter(|v| !special(*v)).collect();
        egui::Grid::new("voices").spacing(Vec2::splat(gap)).show(ui, |ui| {
            for (i, v) in regular.iter().enumerate() {
                let sel = p.voice == *v;
                let (rect, resp) = ui.allocate_exact_size(card, Sense::click());
                let painter = ui.painter();
                let fill = if sel { ACCENT.gamma_multiply(0.30) } else if resp.hovered() { CARD_HOVER } else { CARD };
                painter.rect_filled(rect, CornerRadius::same(14), fill);
                if sel {
                    painter.rect_stroke(rect, CornerRadius::same(14), Stroke::new(2.0f32, ACCENT), egui::StrokeKind::Inside);
                }
                painter.text(rect.center() - Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, v.icon(), FontId::proportional(28.0), if sel { Color32::WHITE } else { TEXT });
                painter.text(rect.center() + Vec2::new(0.0, 26.0), Align2::CENTER_CENTER, v.name(), FontId::proportional(13.5), if sel { Color32::WHITE } else { TEXT });
                if v.is_new() {
                    let b = Rect::from_min_size(rect.right_top() + Vec2::new(-38.0, 6.0), Vec2::new(32.0, 16.0));
                    painter.rect_filled(b, CornerRadius::same(8), ACCENT2.gamma_multiply(0.25));
                    painter.text(b.center(), Align2::CENTER_CENTER, "NEW", FontId::proportional(10.0), ACCENT2);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    p.voice = *v;
                }
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(14.0);
        subheading(ui, "Special voices", "These have their own controls. Press Use to switch to one.");

        special_area(ui, &mut p.voice, Voice::Custom, "Pick any pitch, from very deep to very high.", |ui, pitch: &mut f32| {
            slider_row(ui, "Pitch", pitch, -12.0, 12.0, 0.05, 4.5, |v| format!("{v:+.2} st"));
            ui.horizontal(|ui| {
                ui.add_space(LABEL_W + ui.spacing().item_spacing.x);
                for (label, v) in [("Deeper", -5.0), ("Default", 4.5), ("Higher", 9.0)] {
                    if pill(ui, label, (*pitch - v).abs() < 0.001) {
                        *pitch = v;
                    }
                }
            });
        }, &mut p.custom_pitch);

        let mut cs = (p.chain, p.custom_pitch);
        special_area(ui, &mut p.voice, Voice::Chainer, "Stack up to four voices, one after another.", |ui, (chain, cpitch): &mut ([Voice; 4], f32)| {
            ui.horizontal_wrapped(|ui| {
                for (i, slot) in chain.iter_mut().enumerate() {
                    egui::ComboBox::from_id_salt(("chain", i))
                        .selected_text(format!("{} {}", slot.icon(), if *slot == Voice::None { "Empty" } else { slot.name() }))
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for v in Voice::ALL.iter().filter(|v| **v != Voice::Chainer) {
                                let label = if *v == Voice::None { "Empty".to_string() } else { format!("{} {}", v.icon(), v.name()) };
                                ui.selectable_value(slot, *v, label);
                            }
                        });
                    if i < 3 {
                        ui.label(RichText::new(icons::CHEVRON_RIGHT).color(MUTED));
                    }
                }
                if pill(ui, "Clear", false) {
                    *chain = [Voice::None; 4];
                }
            });
            if chain.contains(&Voice::Custom) {
                slider_row(ui, "Custom pitch", cpitch, -12.0, 12.0, 0.05, 4.5, |v| format!("{v:+.2} st"));
            }
        }, &mut cs);
        p.chain = cs.0;
        p.custom_pitch = cs.1;

        special_area(ui, &mut p.voice, Voice::Silence, "Silence your microphone completely. Sounds and music still play.", |ui, _: &mut ()| {
            ui.label(RichText::new("Tip: hotkey + F8/F9 also steps through all voices.").size(12.0).color(MUTED));
        }, &mut ());
        ui.add_space(6.0);

        ui.columns(2, |cols| {
            let (l, r) = cols.split_at_mut(1);
            let l = &mut l[0];
            let r = &mut r[0];
            section(l, "Room", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for room in Room::ALL {
                        if pill(ui, &format!("{} {}", room.icon(), room.name()), p.room == room) {
                            p.room = room;
                        }
                    }
                });
                if p.room != Room::None {
                    slider_row(ui, "Amount", &mut p.room_mix, 0.0, 1.5, 0.0, 1.0, pct);
                }
            });
            section(l, "Tone", |ui| {
                slider_row(ui, "Bass", &mut p.bass_db, -30.0, 30.0, 0.5, 0.0, db);
                slider_row(ui, "Treble", &mut p.treble_db, -30.0, 30.0, 0.5, 0.0, db);
            });
            section(l, "Levels", |ui| {
                slider_row(ui, "Mic volume", &mut p.mic_gain, 0.0, 3.0, 0.0, 1.0, pct);
                let mut b = p.boost as f32;
                slider_row(ui, "Output boost", &mut b, 0.0, 100.0, 1.0, 0.0, |v| format!("×{:.2}", 1.0 + v * dsp::BOOST_PER_STEP));
                p.boost = b as u8;
            });
            section(r, "Vocoder (sing like a synth)", |ui| {
                toggle(ui, &mut p.vocoder, "Turn on vocoder");
                ui.add_enabled_ui(p.vocoder, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for k in CarrierKind::ALL {
                            if pill(ui, k.name(), p.carrier == k) {
                                p.carrier = k;
                            }
                        }
                    });
                    if p.carrier == CarrierKind::File {
                        let current = c.cfg.carrier_file.as_ref().map(|f| stem(f)).unwrap_or_else(|| "Choose a sound…".into());
                        let mut pick: Option<PathBuf> = None;
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("carrier_file").selected_text(current).width(180.0).show_ui(ui, |ui| {
                                for f in &c.vocoder_files {
                                    if ui.selectable_label(c.cfg.carrier_file.as_ref() == Some(f), stem(f)).clicked() {
                                        pick = Some(f.clone());
                                    }
                                }
                            });
                            if ui.button(format!("{}  Browse…", icons::FOLDER)).clicked() {
                                pick = rfd::FileDialog::new().add_filter("Audio", &AUDIO_EXTS).pick_file();
                            }
                        });
                        if let Some(f) = pick {
                            c.cfg.carrier_file = Some(f);
                            c.load_carrier();
                        }
                    } else if p.carrier != CarrierKind::Whisper {
                        slider_row(ui, "Note", &mut p.carrier_note, 30.0, 72.0, 1.0, 48.0, |v| note_name(v as i32));
                    }
                    slider_row(ui, "Mix", &mut p.vocoder_mix, 0.0, 1.0, 0.0, 1.0, pct);
                });
            });
            section(r, "Clean up", |ui| {
                toggle(ui, &mut p.denoise, "Remove background noise");
                ui.add_enabled_ui(p.denoise, |ui| {
                    slider_row(ui, "Strength", &mut p.denoise_strength, 0.3, 2.0, 0.0, 1.0, |v| format!("{v:.2}"));
                });
                let mut g = p.gate as f32;
                slider_row(ui, "Silence below", &mut g, 0.0, 100.0, 1.0, 0.0, |v| format!("{v:.0}%"));
                p.gate = g as u8;
                ui.label(RichText::new("Mutes the mic when you're quieter than this.").size(11.5).color(MUTED));
            });
        });
        if format!("{p:?}") != before {
            c.set_params(p);
        }
    }

    // ------------------------------------------------------------ speak

    fn page_speak(&mut self, ui: &mut egui::Ui, c: &mut Core) {
        heading(ui, "Speak text", "Type something and a computer voice says it into your microphone, with your voice effect if you like.");
        ui.add(egui::TextEdit::multiline(&mut self.tts_text).desired_rows(6).desired_width(f32::INFINITY).hint_text("Type what to say…"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Voice").color(MUTED));
            let voices = c.voices.clone();
            egui::ComboBox::from_id_salt("tts_voice")
                .selected_text(if c.cfg.tts_voice.is_empty() { "Default".to_string() } else { c.cfg.tts_voice.clone() })
                .width(240.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut c.cfg.tts_voice, String::new(), "Default");
                    for v in voices {
                        ui.selectable_value(&mut c.cfg.tts_voice, v.clone(), v);
                    }
                });
            ui.add_space(12.0);
            let mut r = c.cfg.tts_rate as f32;
            slider_row(ui, "Speed", &mut r, -10.0, 10.0, 1.0, 0.0, |v| format!("{v:+.0}"));
            c.cfg.tts_rate = r as i32;
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can = !c.tts_busy && !self.tts_text.trim().is_empty();
            let b = egui::Button::new(RichText::new(format!("{}  Speak", icons::SPEAK)).size(15.0).color(Color32::WHITE))
                .fill(ACCENT)
                .corner_radius(CornerRadius::same(10))
                .min_size(Vec2::new(120.0, 36.0));
            if ui.add_enabled(can, b).clicked() {
                c.speak(self.tts_text.clone());
            }
            if c.tts_busy {
                ui.spinner();
            }
            if ui.button(format!("{}  Stop", icons::STOP)).clicked() {
                c.shared.send(Cmd::StopSound("speech".into()));
            }
            ui.label(RichText::new(&c.tts_status).color(MUTED));
        });
        let mut p = c.params();
        ui.add_space(6.0);
        if toggle(ui, &mut p.effects_on_sounds, "Apply my voice effect to the spoken text (and sounds)") {
            c.set_params(p);
        }
    }

    // ------------------------------------------------------------ settings

    fn page_settings(&mut self, ui: &mut egui::Ui, c: &mut Core) {
        heading(ui, "Settings", "Your microphone, devices, listening, hotkeys and the window.");
        let mut restart = false;
        let mut reroute = false;

        // add-on left behind by the removed "replace my real microphone" option
        if c.old_addon_installed() {
            egui::Frame::new()
                .fill(WARN.gamma_multiply(0.12))
                .stroke(Stroke::new(1.0f32, WARN.gamma_multiply(0.5)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icons::WARNING).size(18.0).color(WARN));
                        ui.vertical(|ui| {
                            ui.label(RichText::new("The old \"replace my real microphone\" add-on is still installed").color(TEXT).strong());
                            ui.label(RichText::new("That option was removed. Take the add-on out so your microphone is fully back to normal (Windows asks for admin once).").size(12.0).color(MUTED));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if c.cleanup_busy {
                                ui.spinner();
                            } else if primary(ui, icons::DELETE, "Remove it").clicked() {
                                self.audio = None;
                                c.remove_old_addon();
                            }
                        });
                    });
                });
            ui.add_space(10.0);
        }
        if let Some((msg, ok)) = c.route_msg.clone() {
            let col = if ok { GOOD } else { BAD };
            egui::Frame::new().fill(col.gamma_multiply(0.15)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(msg).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icons::CLOSE).clicked() {
                            c.route_msg = None;
                        }
                    });
                });
            });
            ui.add_space(10.0);
        }

        // microphone (virtual cable)
        let cable = c.cable.clone();
        let to_cable = cable.as_ref().map(|x| x.0 == c.cfg.output).unwrap_or(false);
        section(ui, &format!("{}  Microphone", icons::MIC), |ui| {
            ui.label(
                RichText::new("Your changed voice goes into a free virtual cable, and VoiceMorph makes it Windows' default microphone while it runs. Your real mic comes back when you quit.")
                    .size(12.5)
                    .color(MUTED),
            );
            ui.add_space(6.0);
            match &cable {
                Some((out, _)) if !to_cable => status_line(ui, false, &format!("The changed voice isn't going to the cable yet. Press \"Look for microphones again\" to send it to \"{out}\".")),
                Some((_, mic)) if c.cfg.auto_default => status_line(ui, true, &format!("Apps using the default microphone now hear your changed voice (\"{}\").", mic.name)),
                Some((_, mic)) => status_line(ui, true, &format!("Pick \"{}\" as the microphone in your chat or game.", mic.name)),
                None => {
                    status_line(ui, false, "No virtual cable found. Install the free VB-CABLE, then press \"Look for microphones again\".");
                    if ui.button(format!("{}  Get the free virtual cable", icons::OPEN)).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab("https://vb-audio.com/Cable/"));
                    }
                }
            }
            ui.add_space(6.0);
            let real: Vec<String> = c.inputs.iter().filter(|n| !winaudio::is_virtual(n)).cloned().collect();
            egui::Grid::new("vm").num_columns(2).min_col_width(160.0).spacing(Vec2::new(16.0, 10.0)).show(ui, |ui| {
                ui.label("Your real microphone");
                reroute |= device_combo(ui, "vm_in", &mut c.cfg.input, &real, "System default");
                ui.end_row();
                ui.label("Virtual cable");
                ui.label(RichText::new(cable.as_ref().map(|x| x.0.as_str()).unwrap_or("—")).color(TEXT));
                ui.end_row();
            });
            reroute |= toggle(ui, &mut c.cfg.auto_default, "Make the virtual cable Windows' default microphone while VoiceMorph runs");
            if ui.button(format!("{}  Look for microphones again", icons::REFRESH)).clicked() {
                c.refresh_devices();
                reroute = true;
            }
        });

        section(ui, &format!("{}  Devices", icons::SOUNDS), |ui| {
            let (inputs, outputs) = (c.inputs.clone(), c.outputs.clone());
            egui::Grid::new("dev").num_columns(2).min_col_width(160.0).spacing(Vec2::new(16.0, 10.0)).show(ui, |ui| {
                ui.label("Microphone");
                restart |= device_combo(ui, "in", &mut c.cfg.input, &inputs, "System default");
                ui.end_row();
                ui.label("Send changed voice to");
                restart |= device_combo(ui, "out", &mut c.cfg.output, &outputs, "System default");
                ui.end_row();
                ui.label("Let me hear it on");
                restart |= device_combo(ui, "mon", &mut c.cfg.monitor, &outputs, "Nobody (off)");
                ui.end_row();
            });
            ui.label(RichText::new("These are set for you by the Microphone box above. Change them only for a custom setup.").size(12.0).color(MUTED));
            ui.horizontal(|ui| {
                if ui.button(format!("{}  Refresh device list", icons::REFRESH)).clicked() {
                    c.refresh_devices();
                }
                if ui.button(format!("{}  Restart audio", icons::SYNC)).clicked() {
                    restart = true;
                }
            });
        });

        section(ui, &format!("{}  Hear yourself", icons::HEADPHONE), |ui| {
            let mut ch = toggle(ui, &mut c.cfg.hear_voice, "Hear my changed voice");
            ch |= toggle(ui, &mut c.cfg.hear_sounds, "Hear sounds and music");
            ch |= slider_row(ui, "Listening volume", &mut c.cfg.local_volume, 0.0, 1.5, 0.0, 0.8, pct);
            if ch {
                c.apply_listen_opts();
                c.save();
            }
            if c.cfg.monitor.is_empty() {
                ui.label(RichText::new("Choose a device under \"Let me hear it on\" first.").size(12.0).color(WARN));
            }
        });

        section(ui, &format!("{}  Hotkeys (work in any app)", icons::KEYBOARD), |ui| {
            let mut ch = false;
            ui.horizontal(|ui| {
                ui.label("Hold");
                ch |= mod_combo(ui, "m1", &mut c.cfg.mod1);
                ui.label("+");
                ch |= mod_combo(ui, "m2", &mut c.cfg.mod2);
                ui.label("then press:");
            });
            if ch {
                c.apply_listen_opts();
                c.save();
            }
            for (_, _, label) in hotkeys::KEYS {
                ui.label(RichText::new(format!("   {label}")).color(TEXT));
            }
            ui.label(RichText::new("   1 … 9, 0 · play the sound in that slot (set it in a sound's settings)").color(TEXT));
            ui.label(RichText::new("Set both to None to turn hotkeys off.").size(12.0).color(MUTED));
        });

        section(ui, &format!("{}  Window", icons::TRAY), |ui| {
            let mut ch = toggle(ui, &mut c.cfg.hide_to_tray, "Closing the window hides VoiceMorph in the tray (it keeps running)");
            ch |= toggle(ui, &mut c.cfg.start_hidden, "Start hidden in the tray");
            if ch {
                c.save();
            }
            ui.label(
                RichText::new("Click the tray icon to bring the window back; right-click it to turn voice changing on or off, stop sounds or quit.")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.horizontal(|ui| {
                if tray::active() && ui.button(format!("{}  Hide to tray now", icons::TRAY)).clicked() {
                    tray::hide_main();
                }
                if ui.add(egui::Button::new(RichText::new(format!("{}  Quit VoiceMorph", icons::POWER)).color(BAD))).clicked() {
                    if tray::active() {
                        tray::quit();
                    } else {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
        });

        if reroute {
            c.apply_route();
            restart = true;
        }
        if restart {
            c.save();
            c.need_restart = true;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let core = self.core.clone();
        let mut c = core.lock().unwrap();
        if c.need_restart {
            c.need_restart = false;
            drop(c);
            self.restart_audio();
            c = core.lock().unwrap();
        }

        // files dropped on the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            let mut files = Vec::new();
            for p in dropped {
                if p.is_dir() {
                    files.extend(state::list_dir(&p, &AUDIO_EXTS));
                } else {
                    files.push(p);
                }
            }
            if self.page == Page::Music {
                let n = c.add_music(files);
                c.notify(format!("Added {n} song(s) to the playlist"));
            } else {
                let n = c.add_sounds(files);
                c.notify(format!("Added {n} sound(s) to the sound board"));
                self.page = Page::Sounds;
            }
        }

        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(20, 10)).stroke(Stroke::new(1.0f32, CARD_EDGE)))
            .show(ctx, |ui| self.top_bar(ui, &mut c));
        egui::SidePanel::left("side")
            .resizable(false)
            .exact_width(208.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(12, 12)).stroke(Stroke::new(1.0f32, CARD_EDGE)))
            .show(ctx, |ui| self.side_bar(ui, &mut c));
        if self.page != Page::Apps {
            self.apps_loaded = false;
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 28, right: 8, top: 20, bottom: 0 })).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // keep content clear of the scroll bar, and a little air at the bottom
                egui::Frame::new().inner_margin(Margin { left: 0, right: 20, top: 0, bottom: 24 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match self.page {
                        Page::Voice => self.page_voice(ui, &mut c),
                        Page::Sounds => self.page_sounds(ui, &mut c),
                        Page::Music => self.page_music(ui, &mut c),
                        Page::Apps => self.page_apps(ui, &mut c),
                        Page::Speak => self.page_speak(ui, &mut c),
                        Page::Settings => self.page_settings(ui, &mut c),
                    }
                });
            });
        });
        self.sound_editor(ctx, &mut c);

        if let Some((msg, until)) = c.toast.clone() {
            if Instant::now() < until {
                egui::Area::new(egui::Id::new("toast")).anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -24.0)).show(ctx, |ui| {
                    egui::Frame::new().fill(CARD_HOVER).stroke(Stroke::new(1.0f32, FIELD)).corner_radius(CornerRadius::same(18)).inner_margin(Margin::symmetric(18, 10)).show(ui, |ui| {
                        ui.label(RichText::new(msg).color(TEXT));
                    });
                });
            } else {
                c.toast = None;
            }
        }

        // closing the window hides it in the tray
        if ctx.input(|i| i.viewport().close_requested()) && self.shot.is_none() && c.cfg.hide_to_tray && tray::active() && !tray::quitting() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            tray::hide_main();
            if !c.cfg.tray_hint_shown {
                c.cfg.tray_hint_shown = true;
                c.save();
                tray::balloon("VoiceMorph is still running", "Your voice changer keeps working here. Click this icon to open it again, or right-click it to quit.");
            }
        }
        let start_hidden = c.cfg.start_hidden;
        drop(c);

        if self.shot.is_none() && start_hidden && !self.hid_at_start {
            self.frame += 1;
            if self.frame >= 2 && tray::active() {
                self.hid_at_start = true;
                tray::hide_main();
            }
        }

        // screenshot mode: draw ~120 frames, then save the window and quit
        if let Some(path) = self.shot.clone() {
            self.frame += 1;
            ctx.request_repaint();
            if self.frame >= 120 && !self.shot_sent {
                self.shot_sent = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let shots: Vec<Arc<egui::ColorImage>> = ctx.input(|i| {
                i.raw.events.iter().filter_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }).collect()
            });
            if let Some(img) = shots.first() {
                save_png(&path, img);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.frame > 600 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.audio = None;
        if let Ok(mut c) = self.core.lock() {
            c.restore_default();
            c.save();
        }
        tray::remove();
    }
}

fn save_png(path: &std::path::Path, img: &egui::ColorImage) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let [w, h] = img.size;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for c in &img.pixels {
        bytes.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
    }
    if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
        if let Err(e) = buf.save(path) {
            note!("could not save screenshot: {e}");
        }
    }
}

pub fn run() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = winaudio::admin_main(&args) {
        std::process::exit(code);
    }
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    // self-test: record an app for 2 s and report (e.g. --app-test firefox.exe)
    if let Some(exe) = arg("--app-test") {
        let code = match crate::apploop::find_pid(&exe) {
            None => {
                note!("{exe} isn't running");
                2
            }
            Some(pid) => {
                let ring = Arc::new(Mutex::new(std::collections::VecDeque::new()));
                match crate::apploop::AppCapture::start(pid, 48000, ring.clone()) {
                    Ok(cap) => {
                        let mut got = 0usize;
                        let mut peak = 0f32;
                        for _ in 0..20 {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            let mut q = ring.lock().unwrap();
                            got += q.len();
                            peak = q.iter().fold(peak, |m, v| m.max(v.abs()));
                            q.clear();
                        }
                        drop(cap);
                        note!("recording {exe} (pid {pid}) works: {got} samples in 2 s, peak {peak:.3}");
                        0
                    }
                    Err(e) => {
                        note!("recording {exe} failed: {e}");
                        1
                    }
                }
            }
        };
        std::process::exit(code);
    }
    let shot = arg("--screenshot").map(PathBuf::from);
    let page = match arg("--page").as_deref() {
        Some("sounds") => Page::Sounds,
        Some("music") => Page::Music,
        Some("apps") => Page::Apps,
        Some("speak") => Page::Speak,
        Some("settings") => Page::Settings,
        _ => Page::Voice,
    };
    let size = 64;
    let icon = egui::IconData { rgba: tray::icon_rgba(size), width: size, height: size };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("VoiceMorph – voice changer")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([900.0, 580.0])
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native("VoiceMorph", options, Box::new(move |cc| Ok(Box::new(App::new(cc, shot, page)))))
}
