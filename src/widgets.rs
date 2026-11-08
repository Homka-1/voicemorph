//! Colours, style and the small custom controls (switches, sliders, buttons) that
//! keep every control clearly visible on the dark cards.

use crate::dsp::Voice;
use crate::{hotkeys, icons};
use egui::{pos2, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, RichText, Sense, Stroke, Vec2};

pub const BG: Color32 = Color32::from_rgb(0x0f, 0x11, 0x17);
pub const PANEL: Color32 = Color32::from_rgb(0x15, 0x18, 0x21);
pub const CARD: Color32 = Color32::from_rgb(0x1c, 0x21, 0x2e);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x25, 0x2b, 0x3b);
/// Rails, switch tracks and boxes: lighter than any card so they never blend in.
pub const FIELD: Color32 = Color32::from_rgb(0x36, 0x3f, 0x57);
pub const FIELD_STROKE: Color32 = Color32::from_rgb(0x4a, 0x55, 0x70);
/// Hairline around cards.
pub const CARD_EDGE: Color32 = Color32::from_rgb(0x22, 0x27, 0x34);
pub const BUTTON: Color32 = Color32::from_rgb(0x2a, 0x31, 0x44);
pub const BUTTON_HOVER: Color32 = Color32::from_rgb(0x36, 0x3f, 0x57);
pub const INPUT_BG: Color32 = Color32::from_rgb(0x11, 0x14, 0x1c);
pub const ACCENT: Color32 = Color32::from_rgb(0x7c, 0x5c, 0xff);
pub const ACCENT2: Color32 = Color32::from_rgb(0x22, 0xd3, 0xee);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe8, 0xef);
pub const MUTED: Color32 = Color32::from_rgb(0x8b, 0x93, 0xa7);
pub const GOOD: Color32 = Color32::from_rgb(0x34, 0xd3, 0x99);
pub const WARN: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
pub const BAD: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);
pub const PINK: Color32 = Color32::from_rgb(0xf4, 0x72, 0xb6);

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn semi(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(icons::SEMIBOLD.into()))
}

/// Width of the label column in slider rows, so every row lines up.
pub const LABEL_W: f32 = 130.0;
/// Width kept for the value shown after a slider ("+0.0 dB", "100%").
pub const VALUE_W: f32 = 66.0;

/// How many cells of at least `min_w` fit in `avail`, and how wide each becomes so
/// the row fills the space exactly (no ragged gap on the right).
pub fn grid_cells(avail: f32, min_w: f32, gap: f32) -> (usize, f32) {
    let cols = (((avail + gap) / (min_w + gap)).floor() as usize).max(1);
    let w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor();
    (cols, w.max(40.0))
}

/// Shorten `text` with "…" until it fits in `max_w` pixels in `font`.
pub fn fit_text(ui: &egui::Ui, text: &str, font: &FontId, max_w: f32) -> String {
    let width = |s: &str| ui.fonts(|f| f.layout_no_wrap(s.to_string(), font.clone(), TEXT).size().x);
    if width(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let s: String = chars[..mid].iter().collect::<String>() + "…";
        if width(&s) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"
}

/// A fixed-width label cell, vertically centred, so rows line up in columns.
pub fn label_cell(ui: &mut egui::Ui, text: &str, width: f32, color: Color32) {
    ui.allocate_ui_with_layout(Vec2::new(width, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        ui.label(RichText::new(text).color(color));
    });
}

/// The value after a slider, in a fixed-width cell.
pub fn value_cell(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(Vec2::new(VALUE_W, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(VALUE_W);
        ui.label(RichText::new(text).size(13.0).color(MUTED));
    });
}

/// A clickable pill for the top bar: coloured icon and text, at most `max_w` wide
/// (the text is shortened with "…" to fit, never cut off).
pub fn bar_pill(ui: &mut egui::Ui, icon: &str, icon_color: Color32, text: &str, max_w: f32, fill: Color32) -> egui::Response {
    let font = FontId::proportional(13.5);
    let pad = 12.0;
    let icon_w = 22.0;
    let text = fit_text(ui, text, &font, (max_w - pad * 2.0 - icon_w).max(20.0));
    let tw = ui.fonts(|f| f.layout_no_wrap(text.clone(), font.clone(), TEXT).size().x);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(pad * 2.0 + icon_w + tw, 32.0), Sense::click());
    let hovered = resp.hovered();
    let bg = if hovered { CARD_HOVER } else { fill };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(16), bg);
    p.text(rect.left_center() + Vec2::new(pad + 7.0, 0.0), Align2::CENTER_CENTER, icon, FontId::proportional(14.0), icon_color);
    p.text(rect.left_center() + Vec2::new(pad + icon_w, 0.0), Align2::LEFT_CENTER, text, font, if hovered { Color32::WHITE } else { TEXT });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A switch with a label. Returns true when flipped.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) -> bool {
    let flipped = ui
        .horizontal(|ui| {
            ui.set_min_height(28.0);
            let (rect, r) = ui.allocate_exact_size(Vec2::new(40.0, 22.0), Sense::click());
            let t = ui.ctx().animate_bool(r.id, *on);
            let enabled = ui.is_enabled();
            let mut fill = mix(FIELD, ACCENT, t);
            if !enabled {
                fill = fill.gamma_multiply(0.5);
            }
            let painter = ui.painter();
            painter.rect(rect, CornerRadius::same(11), fill, Stroke::new(1.0f32, mix(FIELD_STROKE, ACCENT, t)), egui::StrokeKind::Inside);
            let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
            painter.circle_filled(pos2(x, rect.center().y), if r.hovered() { 8.5 } else { 8.0 }, if enabled { Color32::WHITE } else { MUTED });
            let l = ui.add(egui::Label::new(RichText::new(label).color(if enabled { TEXT } else { MUTED })).sense(Sense::click()));
            (r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() || l.clicked()) && enabled
        })
        .inner;
    if flipped {
        *on = !*on;
    }
    flipped
}

/// A slider drawn to stand out on any card: light rail, accent fill, white knob.
/// Double-click resets it to `default`.
pub fn slider(ui: &mut egui::Ui, v: &mut f32, lo: f32, hi: f32, step: f32, default: f32, width: f32) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::click_and_drag());
    let pad = 9.0;
    let enabled = ui.is_enabled();
    if enabled {
        if resp.double_clicked() {
            *v = default;
            resp.mark_changed();
        } else if let Some(p) = resp.interact_pointer_pos() {
            if resp.is_pointer_button_down_on() || resp.clicked() {
                let t = ((p.x - rect.left() - pad) / (rect.width() - 2.0 * pad)).clamp(0.0, 1.0);
                let mut nv = lo + t * (hi - lo);
                if step > 0.0 {
                    nv = (nv / step).round() * step;
                }
                nv = nv.clamp(lo, hi);
                if nv != *v {
                    *v = nv;
                    resp.mark_changed();
                }
            }
        }
    }
    let t = ((*v - lo) / (hi - lo)).clamp(0.0, 1.0);
    let rail = Rect::from_min_max(pos2(rect.left() + pad, rect.center().y - 3.0), pos2(rect.right() - pad, rect.center().y + 3.0));
    let painter = ui.painter();
    painter.rect_filled(rail, CornerRadius::same(3), FIELD);
    let x = rail.left() + rail.width() * t;
    let zero = if lo < 0.0 && hi > 0.0 { rail.left() + rail.width() * (-lo / (hi - lo)) } else { rail.left() };
    let (a, b) = if x >= zero { (zero, x) } else { (x, zero) };
    let accent = if enabled { ACCENT } else { MUTED };
    painter.rect_filled(Rect::from_min_max(pos2(a, rail.top()), pos2(b, rail.bottom())), CornerRadius::same(3), accent);
    let big = resp.hovered() || resp.dragged();
    painter.circle(pos2(x, rail.center().y), if big { 8.5 } else { 7.5 }, if enabled { Color32::WHITE } else { FIELD_STROKE }, Stroke::new(2.0f32, accent));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Label, slider and its value on one line. Returns true when changed.
#[allow(clippy::too_many_arguments)]
pub fn slider_row(ui: &mut egui::Ui, label: &str, v: &mut f32, lo: f32, hi: f32, step: f32, default: f32, fmt: impl Fn(f32) -> String) -> bool {
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        label_cell(ui, label, LABEL_W, TEXT);
        let w = (ui.available_width() - VALUE_W - ui.spacing().item_spacing.x).clamp(120.0, 300.0);
        let r = slider(ui, v, lo, hi, step, default, w);
        value_cell(ui, &fmt(*v));
        r.changed()
    })
    .inner
}

pub fn primary(ui: &mut egui::Ui, icon: &str, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(format!("{icon}  {text}")).color(Color32::WHITE))
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(10))
            .min_size(Vec2::new(0.0, 32.0)),
    )
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str, on: bool) -> egui::Response {
    let b = egui::Button::new(RichText::new(icon).size(16.0).color(if on { ACCENT2 } else { TEXT }))
        .fill(if on { ACCENT2.gamma_multiply(0.15) } else { BUTTON })
        .corner_radius(CornerRadius::same(20))
        .min_size(Vec2::splat(40.0));
    ui.add(b).on_hover_text(tip)
}

pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, opts: &[(T, &str)]) -> bool {
    let mut changed = false;
    egui::Frame::new().fill(BUTTON).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(3)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (v, label) in opts {
                let sel = *value == *v;
                let b = egui::Button::new(RichText::new(*label).size(13.0).color(if sel { Color32::WHITE } else { MUTED }))
                    .fill(if sel { ACCENT } else { Color32::TRANSPARENT })
                    .corner_radius(CornerRadius::same(8));
                if ui.add(b).clicked() && !sel {
                    *value = *v;
                    changed = true;
                }
            }
        });
    });
    changed
}

pub fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str, width: f32) {
    egui::Frame::new().fill(INPUT_BG).stroke(Stroke::new(1.0f32, FIELD)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::SEARCH).color(MUTED));
            ui.add(egui::TextEdit::singleline(text).hint_text(hint).frame(false).desired_width(width));
            if !text.is_empty() && ui.add(egui::Button::new(RichText::new(icons::CLOSE).size(11.0)).frame(false)).clicked() {
                text.clear();
            }
        });
    });
}

pub fn status_line(ui: &mut egui::Ui, ok: bool, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 5.0, if ok { GOOD } else { WARN });
        ui.label(RichText::new(text).color(TEXT));
    });
}

pub fn device_combo(ui: &mut egui::Ui, id: &str, value: &mut String, list: &[String], default: &str) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(if value.is_empty() { default.to_string() } else { value.clone() })
        .width(320.0)
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(value, String::new(), default).changed();
            for n in list {
                changed |= ui.selectable_value(value, n.clone(), n).changed();
            }
        });
    changed
}

pub fn mod_combo(ui: &mut egui::Ui, id: &str, value: &mut u8) -> bool {
    let cur = hotkeys::MODIFIERS.iter().find(|m| m.1 == *value).map(|m| m.0).unwrap_or("None");
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(cur).width(80.0).show_ui(ui, |ui| {
        for (n, vk) in hotkeys::MODIFIERS {
            changed |= ui.selectable_value(value, vk, n).changed();
        }
    });
    changed
}

pub fn note_name(n: i32) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[n.rem_euclid(12) as usize], n / 12 - 1)
}

pub fn pct(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

pub fn db(v: f32) -> String {
    if v.abs() < 0.05 {
        "0 dB".into()
    } else {
        format!("{v:+.1} dB")
    }
}

pub fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

pub fn mmss(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn heading(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(title).font(semi(24.0)).color(TEXT));
    ui.add_space(2.0);
    ui.label(RichText::new(sub).size(13.0).color(MUTED));
    ui.add_space(16.0);
}

/// A small title above a group of cards.
pub fn subheading(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).font(semi(16.0)).color(TEXT));
    if !sub.is_empty() {
        ui.label(RichText::new(sub).size(12.5).color(MUTED));
    }
    ui.add_space(8.0);
}

pub fn card_frame(fill: Color32) -> egui::Frame {
    egui::Frame::new().fill(fill).stroke(Stroke::new(1.0f32, CARD_EDGE)).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(16))
}

pub fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    card_frame(PANEL).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).font(semi(15.0)).color(TEXT));
        ui.add_space(8.0);
        body(ui);
    });
    ui.add_space(12.0);
}

/// A boxed area for one special voice: its name, a Use button and its own controls.
pub fn special_area<T>(ui: &mut egui::Ui, current: &mut Voice, v: Voice, blurb: &str, body: impl FnOnce(&mut egui::Ui, &mut T), state: &mut T) {
    let active = *current == v;
    let frame = egui::Frame::new()
        .fill(if active { ACCENT.gamma_multiply(0.18) } else { PANEL })
        .stroke(Stroke::new(if active { 1.5f32 } else { 1.0 }, if active { ACCENT } else { CARD_EDGE }))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(16));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(10), if active { ACCENT } else { CARD });
            ui.painter().text(r.center(), Align2::CENTER_CENTER, v.icon(), FontId::proportional(18.0), Color32::WHITE);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(v.name()).font(semi(15.0)).color(TEXT));
                ui.label(RichText::new(blurb).size(12.5).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if active {
                    if ui.button("Turn off").clicked() {
                        *current = Voice::None;
                    }
                    ui.label(RichText::new(format!("{}  In use", icons::CHECK)).color(GOOD));
                } else if ui
                    .add(egui::Button::new(RichText::new("Use").color(Color32::WHITE)).fill(ACCENT).corner_radius(CornerRadius::same(10)).min_size(Vec2::new(72.0, 30.0)))
                    .clicked()
                {
                    *current = v;
                }
            });
        });
        ui.add_space(8.0);
        body(ui, state);
    });
    ui.add_space(12.0);
}

pub fn pill(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let b = egui::Button::new(RichText::new(label).color(if selected { Color32::WHITE } else { TEXT }))
        .fill(if selected { ACCENT } else { BUTTON })
        .corner_radius(CornerRadius::same(16))
        .min_size(Vec2::new(0.0, 30.0));
    ui.add(b).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

pub fn meter(ui: &mut egui::Ui, label: &str, level: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(96.0, 30.0), Sense::hover());
    let painter = ui.painter();
    painter.text(rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(11.0), MUTED);
    let bar = Rect::from_min_max(rect.left_center() + Vec2::new(32.0, -4.0), rect.right_center() + Vec2::new(0.0, 4.0));
    painter.rect_filled(bar, CornerRadius::same(4), FIELD);
    let dbv = 20.0 * level.max(1e-5).log10();
    let frac = ((dbv + 60.0) / 60.0).clamp(0.0, 1.0);
    let mut fill = bar;
    fill.set_width(bar.width() * frac);
    painter.rect_filled(fill, CornerRadius::same(4), color);
}

pub fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0f32, FIELD);
    v.extreme_bg_color = INPUT_BG;
    v.faint_bg_color = CARD;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0f32, Color32::WHITE);
    v.hyperlink_color = ACCENT2;
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0f32, TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, CARD_HOVER);
    // boxes and rails (checkbox, radio, built-in sliders): light enough to see on cards
    v.widgets.inactive.bg_fill = FIELD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0f32, FIELD_STROKE);
    v.widgets.inactive.weak_bg_fill = BUTTON;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0f32, TEXT);
    v.widgets.hovered.bg_fill = BUTTON_HOVER;
    v.widgets.hovered.weak_bg_fill = BUTTON_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0f32, ACCENT.gamma_multiply(0.7));
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.open.weak_bg_fill = BUTTON_HOVER;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(8);
    }
    v.slider_trailing_fill = true;
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        // an app, not a web page: text isn't selectable and shows the normal arrow
        s.interaction.selectable_labels = false;
        s.interaction.multi_widget_text_select = false;
        s.spacing.scroll = egui::style::ScrollStyle::thin();
        s.spacing.scroll.bar_inner_margin = 2.0;
        s.spacing.combo_height = 320.0;
        s.spacing.interact_size.y = 28.0;
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.slider_width = 220.0;
        s.spacing.icon_width = 18.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
    });
}
