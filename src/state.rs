//! Everything the app knows and does, apart from drawing it. A background thread
//! ("brain") handles hotkeys, loaded files, the tray menu and the end of each song,
//! so all of that keeps working while the window is hidden in the tray.

use crate::audio::{self, Clip, Cmd, Shared, SoundPlay};
use crate::dsp::{Params, Voice};
use crate::hotkeys::{self, Hotkey, HotkeyMods};
use crate::tray::{self, TrayCmd};
use crate::{apploop, tts, winaudio};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const ORIGINAL_DIR: &str = r"C:\Program Files (x86)\ClownfishVoiceChanger";
pub const AUDIO_EXTS: [&str; 5] = ["mp3", "wav", "ogg", "flac", "m4a"];

/// Per-sound choices on the sound board.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SoundOpts {
    /// Shown name; empty = the file's name.
    pub name: String,
    pub volume: f32,
    pub speed: f32,
    pub looped: bool,
    /// Hotkey slot 1..=10 (modifier + 1..9, 0); 0 = none.
    pub slot: u8,
    pub fav: bool,
}

impl Default for SoundOpts {
    fn default() -> Self {
        SoundOpts { name: String::new(), volume: 1.0, speed: 1.0, looped: false, slot: 0, fav: false }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayMode {
    /// Sounds play on top of each other (like the original).
    Layer,
    /// A new sound stops the others.
    OneAtATime,
    /// Pressing a playing sound stops it.
    Toggle,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Repeat {
    Off,
    All,
    One,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub params: Params,
    pub input: String,
    pub output: String,
    pub monitor: String,
    pub hear_voice: bool,
    pub hear_sounds: bool,
    pub local_volume: f32,
    pub sounds: Vec<PathBuf>,
    pub music: Vec<PathBuf>,
    pub repeat: Repeat,
    pub shuffle: bool,
    pub starred: Option<String>,
    pub mod1: u8,
    pub mod2: u8,
    pub carrier_file: Option<PathBuf>,
    pub tts_voice: String,
    pub tts_rate: i32,
    pub auto_default: bool,
    /// Windows' default microphones before we switched them (console, communications).
    pub prev_default: Option<(String, String)>,
    // sound board
    pub sound_opts: HashMap<String, SoundOpts>,
    pub play_mode: PlayMode,
    pub big_tiles: bool,
    // music player
    pub music_speed: f32,
    pub music_fade: f32,
    pub music_bass: f32,
    pub music_treble: f32,
    pub duck: bool,
    pub duck_level: f32,
    pub music_to_call: bool,
    pub music_muted: bool,
    // window
    pub hide_to_tray: bool,
    pub start_hidden: bool,
    pub tray_hint_shown: bool,
    // another app's sound into the microphone
    pub app_on: bool,
    /// Program file name of the app, e.g. "firefox.exe".
    pub app_exe: String,
    pub app_volume: f32,
    pub app_fx: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            params: Params::default(),
            input: String::new(),
            output: String::new(),
            monitor: String::new(),
            hear_voice: false,
            hear_sounds: true,
            local_volume: 0.8,
            sounds: Vec::new(),
            music: Vec::new(),
            repeat: Repeat::All,
            shuffle: false,
            starred: Some("synth:Ding".into()),
            mod1: 0x11,
            mod2: 0x10,
            carrier_file: None,
            tts_voice: String::new(),
            tts_rate: 0,
            auto_default: true,
            prev_default: None,
            sound_opts: HashMap::new(),
            play_mode: PlayMode::Layer,
            big_tiles: false,
            music_speed: 1.0,
            music_fade: 0.0,
            music_bass: 0.0,
            music_treble: 0.0,
            duck: false,
            duck_level: 0.3,
            music_to_call: true,
            music_muted: false,
            hide_to_tray: true,
            start_hidden: false,
            tray_hint_shown: false,
            app_on: false,
            app_exe: String::new(),
            app_volume: 1.0,
            app_fx: false,
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("VoiceMorph").join("settings.json")
}

pub fn load_config() -> Config {
    std::fs::read_to_string(config_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

#[derive(Clone)]
pub struct SoundItem {
    pub id: String,
    pub file_name: String,
    pub path: Option<PathBuf>,
    pub user: bool,
}

impl SoundItem {
    pub fn builtin(&self) -> bool {
        self.path.is_none()
    }
}

pub enum Job {
    Music(usize, Result<Clip, String>),
    Speech(Result<Clip, String>),
    Voices(Vec<String>),
    Carrier(Result<Clip, String>),
    Cleanup(Result<String, String>),
}

pub fn list_dir(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()).map(|e| exts.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

pub fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

pub struct Core {
    pub cfg: Config,
    pub shared: Arc<Shared>,
    pub mods: Arc<HotkeyMods>,
    pub jobs_tx: Sender<Job>,
    pub sounds: Vec<SoundItem>,
    clips: HashMap<String, Clip>,
    pub music_index: Option<usize>,
    pub music_paused: bool,
    pub music_loading: bool,
    music_history: Vec<usize>,
    music_cache: Option<(usize, Clip)>,
    pub tts_busy: bool,
    pub tts_status: String,
    pub voices: Vec<String>,
    pub toast: Option<(String, Instant)>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub mics: Vec<winaudio::Endpoint>,
    pub cable: Option<(String, winaudio::Endpoint)>,
    pub route_msg: Option<(String, bool)>,
    pub cleanup_busy: bool,
    pub vocoder_files: Vec<PathBuf>,
    /// The window should restart audio (devices changed).
    pub need_restart: bool,
    pub apps: Vec<apploop::AudioApp>,
    app_capture: Option<apploop::AppCapture>,
    /// What app sound is doing now, and whether that's good.
    pub app_status: (String, bool),
    app_retry: Instant,
    rng: u64,
}

impl Core {
    pub fn new(cfg: Config) -> Self {
        let shared = Shared::new(cfg.params.clone());
        let mods = Arc::new(HotkeyMods { primary: cfg.mod1.into(), secondary: cfg.mod2.into() });
        let (jobs_tx, _) = channel();
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
        let mut c = Core {
            cfg,
            shared,
            mods,
            jobs_tx,
            sounds: Vec::new(),
            clips: HashMap::new(),
            music_index: None,
            music_paused: false,
            music_loading: false,
            music_history: Vec::new(),
            music_cache: None,
            tts_busy: false,
            tts_status: String::new(),
            voices: Vec::new(),
            toast: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            mics: Vec::new(),
            cable: None,
            route_msg: None,
            cleanup_busy: false,
            vocoder_files: list_dir(&Path::new(ORIGINAL_DIR).join("vocoders"), &AUDIO_EXTS),
            need_restart: false,
            apps: Vec::new(),
            app_capture: None,
            app_status: (String::new(), false),
            app_retry: Instant::now(),
            rng: seed | 1,
        };
        c.apply_listen_opts();
        c.apply_music_opts();
        c.apply_app_opts();
        c.refresh_sounds();
        c
    }

    // ------------------------------------------------------------ app sound

    pub fn apply_app_opts(&self) {
        let a = &self.shared.app;
        a.on.store(self.cfg.app_on && self.app_capture.is_some(), Ordering::Relaxed);
        a.volume.set(self.cfg.app_volume);
        a.fx.store(self.cfg.app_fx, Ordering::Relaxed);
    }

    pub fn refresh_apps(&mut self) {
        self.apps = apploop::list_apps();
    }

    pub fn app_capturing(&self) -> bool {
        self.app_capture.is_some()
    }

    /// Start (or restart) recording the chosen app, or stop when it's off.
    pub fn restart_app_capture(&mut self) {
        self.app_capture = None;
        if let Ok(mut q) = self.shared.app_ring.lock() {
            q.clear();
        }
        self.app_retry = Instant::now() + Duration::from_secs(3);
        if !self.cfg.app_on || self.cfg.app_exe.is_empty() {
            self.app_status = (if self.cfg.app_exe.is_empty() { "Pick an app below.".into() } else { "Off".into() }, false);
            self.apply_app_opts();
            return;
        }
        let name = apploop::pretty_name(&self.cfg.app_exe);
        match apploop::find_pid(&self.cfg.app_exe) {
            None => self.app_status = (format!("{name} isn't running. It starts as soon as you open it."), false),
            Some(pid) => match apploop::AppCapture::start(pid, self.rate(), self.shared.app_ring.clone()) {
                Ok(cap) => {
                    self.app_capture = Some(cap);
                    self.app_status = (format!("{name}'s sound is going into your microphone."), true);
                }
                Err(e) => self.app_status = (format!("Couldn't record {name}: {e}"), false),
            },
        }
        self.apply_app_opts();
    }

    /// Called from the background thread: wait for the app to open, notice when it closes.
    fn app_tick(&mut self) -> bool {
        if !self.cfg.app_on || self.cfg.app_exe.is_empty() || Instant::now() < self.app_retry {
            return false;
        }
        self.app_retry = Instant::now() + Duration::from_secs(3);
        let alive = apploop::find_pid(&self.cfg.app_exe).is_some();
        if self.app_capture.is_some() != alive {
            self.restart_app_capture();
            return true;
        }
        false
    }

    // ------------------------------------------------------------ basics

    pub fn rate(&self) -> u32 {
        self.shared.engine_rate.load(Ordering::Relaxed)
    }

    pub fn params(&self) -> Params {
        self.shared.params.lock().map(|p| p.clone()).unwrap_or_default()
    }

    pub fn set_params(&mut self, p: Params) {
        tray::VOICE_ON.store(p.enabled, Ordering::Relaxed);
        if let Ok(mut s) = self.shared.params.lock() {
            *s = p.clone();
        }
        self.cfg.params = p;
    }

    pub fn save(&mut self) {
        if let Ok(p) = self.shared.params.lock() {
            self.cfg.params = p.clone();
        }
        let path = config_path();
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(&self.cfg) {
            let _ = std::fs::write(path, s);
        }
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now() + Duration::from_millis(2500)));
    }

    fn random(&mut self, n: usize) -> usize {
        // xorshift
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % n.max(1) as u64) as usize
    }

    pub fn apply_listen_opts(&self) {
        self.shared.hear_voice.store(self.cfg.hear_voice, Ordering::Relaxed);
        self.shared.hear_sounds.store(self.cfg.hear_sounds, Ordering::Relaxed);
        self.shared.local_volume.set(self.cfg.local_volume);
        self.mods.primary.store(self.cfg.mod1, Ordering::Relaxed);
        self.mods.secondary.store(self.cfg.mod2, Ordering::Relaxed);
    }

    pub fn apply_music_opts(&self) {
        let m = &self.shared.music;
        m.speed.set(self.cfg.music_speed);
        m.fade_ms.store((self.cfg.music_fade * 1000.0) as u32, Ordering::Relaxed);
        m.bass_db.set(self.cfg.music_bass);
        m.treble_db.set(self.cfg.music_treble);
        m.duck.store(self.cfg.duck, Ordering::Relaxed);
        m.duck_level.set(self.cfg.duck_level);
        m.to_call.store(self.cfg.music_to_call, Ordering::Relaxed);
    }

    pub fn toggle_enabled(&mut self) {
        let mut p = self.params();
        p.enabled = !p.enabled;
        let msg = if p.enabled { "Voice changing on" } else { "Voice changing off" };
        self.set_params(p);
        self.notify(msg);
    }

    pub fn cycle_voice(&mut self, dir: i32) {
        let mut p = self.params();
        let order = &Voice::ALL;
        let i = order.iter().position(|v| *v == p.voice).unwrap_or(0) as i32;
        let n = order.len() as i32;
        p.voice = order[((i + dir).rem_euclid(n)) as usize];
        let name = p.voice.name();
        self.set_params(p);
        self.notify(format!("Voice: {name}"));
    }

    pub fn stop_all(&mut self) {
        self.shared.send(Cmd::StopSounds);
        self.shared.send(Cmd::StopMusic);
        self.music_index = None;
        self.music_paused = false;
    }

    // ------------------------------------------------------------ sound board

    pub fn refresh_sounds(&mut self) {
        let mut v: Vec<SoundItem> = audio::SYNTH_SOUNDS
            .iter()
            .map(|n| SoundItem { id: format!("synth:{n}"), file_name: n.to_string(), path: None, user: false })
            .collect();
        for p in list_dir(&Path::new(ORIGINAL_DIR).join("sounds"), &AUDIO_EXTS) {
            v.push(SoundItem { id: p.to_string_lossy().to_string(), file_name: stem(&p), path: Some(p), user: false });
        }
        for p in &self.cfg.sounds {
            v.push(SoundItem { id: p.to_string_lossy().to_string(), file_name: stem(p), path: Some(p.clone()), user: true });
        }
        self.sounds = v;
    }

    pub fn add_sounds(&mut self, files: Vec<PathBuf>) -> usize {
        let mut n = 0;
        for f in files {
            if is_audio(&f) && !self.cfg.sounds.contains(&f) {
                self.cfg.sounds.push(f);
                n += 1;
            }
        }
        self.refresh_sounds();
        self.save();
        n
    }

    pub fn remove_sound(&mut self, id: &str) {
        self.cfg.sounds.retain(|p| p.to_string_lossy() != id);
        self.cfg.sound_opts.remove(id);
        self.refresh_sounds();
        self.save();
    }

    pub fn opts(&self, id: &str) -> SoundOpts {
        self.cfg.sound_opts.get(id).cloned().unwrap_or_default()
    }

    pub fn set_opts(&mut self, id: &str, o: SoundOpts) {
        if o.slot != 0 {
            // a slot belongs to one sound only
            for (k, v) in self.cfg.sound_opts.iter_mut() {
                if k != id && v.slot == o.slot {
                    v.slot = 0;
                }
            }
        }
        if o == SoundOpts::default() {
            self.cfg.sound_opts.remove(id);
        } else {
            self.cfg.sound_opts.insert(id.to_string(), o);
        }
    }

    pub fn display_name(&self, s: &SoundItem) -> String {
        let o = self.cfg.sound_opts.get(&s.id);
        match o {
            Some(o) if !o.name.trim().is_empty() => o.name.clone(),
            _ => s.file_name.clone(),
        }
    }

    pub fn is_playing(&self, id: &str) -> Option<f32> {
        self.shared.playing.lock().ok().and_then(|p| p.iter().find(|(i, _)| i == id).map(|(_, f)| *f))
    }

    pub fn play_sound_id(&mut self, id: &str) {
        if let Some(i) = self.sounds.iter().position(|s| s.id == id) {
            self.play_sound(i);
        }
    }

    pub fn play_sound(&mut self, idx: usize) {
        let Some(item) = self.sounds.get(idx).cloned() else { return };
        if self.cfg.play_mode == PlayMode::Toggle && self.is_playing(&item.id).is_some() {
            self.shared.send(Cmd::StopSound(item.id));
            return;
        }
        let rate = self.rate();
        let key = format!("{}@{}", item.id, rate);
        let clip = if let Some(c) = self.clips.get(&key) {
            Ok(c.clone())
        } else if let Some(p) = &item.path {
            audio::load_clip(p, rate)
        } else {
            Ok(audio::synth_sound(&item.file_name, rate))
        };
        match clip {
            Ok(c) => {
                self.clips.insert(key, c.clone());
                let o = self.opts(&item.id);
                self.shared.send(Cmd::Sound(SoundPlay {
                    id: item.id,
                    clip: c,
                    volume: o.volume,
                    speed: o.speed,
                    looped: o.looped,
                    exclusive: self.cfg.play_mode == PlayMode::OneAtATime,
                }));
            }
            Err(e) => {
                let name = self.display_name(&item);
                self.notify(format!("Couldn't play {name}: {e}"))
            }
        }
    }

    pub fn play_slot(&mut self, slot: u8) {
        let id = self.cfg.sound_opts.iter().find(|(_, o)| o.slot == slot).map(|(k, _)| k.clone());
        if let Some(id) = id {
            self.play_sound_id(&id);
        }
    }

    // ------------------------------------------------------------ music

    pub fn add_music(&mut self, files: Vec<PathBuf>) -> usize {
        let before = self.cfg.music.len();
        for f in files {
            if is_audio(&f) {
                self.cfg.music.push(f);
            }
        }
        self.save();
        self.cfg.music.len() - before
    }

    pub fn play_track(&mut self, i: usize) {
        let Some(p) = self.cfg.music.get(i).cloned() else { return };
        if let Some(cur) = self.music_index {
            if cur != i {
                self.music_history.push(cur);
                if self.music_history.len() > 200 {
                    self.music_history.remove(0);
                }
            }
        }
        self.music_index = Some(i);
        self.music_paused = false;
        if let Some((ci, clip)) = &self.music_cache {
            if *ci == i && clip.len() > 0 {
                self.shared.send(Cmd::Music(clip.clone()));
                return;
            }
        }
        self.music_loading = true;
        let rate = self.rate();
        let tx = self.jobs_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Job::Music(i, audio::load_clip(&p, rate)));
        });
    }

    pub fn toggle_play(&mut self, selected: Option<usize>) {
        if self.music_index.is_none() {
            if self.cfg.music.is_empty() {
                self.notify("Add some songs to the playlist first");
                return;
            }
            let i = selected.unwrap_or(0);
            self.play_track(i);
        } else {
            self.music_paused = !self.music_paused;
            self.shared.send(Cmd::PauseMusic(self.music_paused));
        }
    }

    pub fn stop_music(&mut self) {
        self.shared.send(Cmd::StopMusic);
        self.music_index = None;
        self.music_paused = false;
    }

    /// `auto`: the song ended by itself.
    pub fn next_track(&mut self, auto: bool) {
        let n = self.cfg.music.len();
        if n == 0 {
            return;
        }
        if auto && self.cfg.repeat == Repeat::One {
            if let Some(i) = self.music_index {
                self.play_track(i);
                return;
            }
        }
        let next = if self.cfg.shuffle && n > 1 {
            let cur = self.music_index.unwrap_or(usize::MAX);
            let mut r = self.random(n);
            if r == cur {
                r = (r + 1 + self.random(n - 1)) % n;
            }
            r
        } else {
            match self.music_index {
                Some(i) if i + 1 < n => i + 1,
                Some(_) if auto && self.cfg.repeat == Repeat::Off => {
                    self.music_index = None;
                    return;
                }
                _ => 0,
            }
        };
        self.play_track(next);
    }

    pub fn prev_track(&mut self) {
        let n = self.cfg.music.len();
        if n == 0 {
            return;
        }
        // more than 3 s in: start this song again
        if let Some(i) = self.music_index {
            let secs = self.shared.music_pos.load(Ordering::Relaxed) as f32 / self.rate().max(1) as f32;
            if secs > 3.0 {
                self.play_track(i);
                return;
            }
        }
        let prev = if self.cfg.shuffle {
            self.music_history.pop()
        } else {
            None
        };
        let i = prev.unwrap_or_else(|| self.music_index.map(|i| (i + n - 1) % n).unwrap_or(0));
        let cur = self.music_index;
        self.play_track(i);
        if prev.is_some() {
            // going back shouldn't add to the history
            if let Some(c) = cur {
                if self.music_history.last() == Some(&c) {
                    self.music_history.pop();
                }
            }
        }
    }

    pub fn skip_seconds(&mut self, secs: f32) {
        let len = self.shared.music_len.load(Ordering::Relaxed).max(1) as f64;
        let pos = self.shared.music_pos.load(Ordering::Relaxed) as f64;
        let to = pos + secs as f64 * self.rate() as f64;
        self.shared.send(Cmd::SeekMusic((to / len).clamp(0.0, 1.0)));
    }

    pub fn remove_track(&mut self, i: usize) {
        if i >= self.cfg.music.len() {
            return;
        }
        self.cfg.music.remove(i);
        self.music_history.clear();
        self.music_cache = None;
        match self.music_index {
            Some(c) if c == i => self.stop_music(),
            Some(c) if c > i => self.music_index = Some(c - 1),
            _ => {}
        }
        self.save();
    }

    pub fn move_track(&mut self, i: usize, to: usize) {
        let n = self.cfg.music.len();
        if i >= n || to >= n || i == to {
            return;
        }
        let item = self.cfg.music.remove(i);
        self.cfg.music.insert(to, item);
        self.music_history.clear();
        self.music_cache = None;
        if let Some(c) = self.music_index {
            self.music_index = Some(if c == i {
                to
            } else if i < c && to >= c {
                c - 1
            } else if i > c && to <= c {
                c + 1
            } else {
                c
            });
        }
        self.save();
    }

    pub fn sort_music(&mut self) {
        let cur = self.music_index.and_then(|i| self.cfg.music.get(i).cloned());
        self.cfg.music.sort_by_key(|p| stem(p).to_lowercase());
        self.music_index = cur.and_then(|c| self.cfg.music.iter().position(|p| *p == c));
        self.music_history.clear();
        self.music_cache = None;
        self.save();
    }

    pub fn clear_music(&mut self) {
        self.stop_music();
        self.cfg.music.clear();
        self.music_history.clear();
        self.music_cache = None;
        self.save();
    }

    // ------------------------------------------------------------ microphone route

    pub fn refresh_devices(&mut self) {
        self.inputs = audio::input_names();
        self.outputs = audio::output_names();
        self.mics = winaudio::capture_endpoints();
        self.cable = winaudio::find_cable(&self.outputs, &self.mics);
    }

    pub fn old_addon_installed(&self) -> bool {
        self.mics.iter().any(|m| m.hooked)
    }

    /// Send the changed voice into the virtual cable and (optionally) make the
    /// cable Windows' default microphone.
    pub fn apply_route(&mut self) {
        let Some((out, cable_mic)) = self.cable.clone() else {
            self.restore_default();
            return;
        };
        if self.cfg.input.is_empty() || winaudio::is_virtual(&self.cfg.input) {
            // pin the real microphone first, so switching the default can't loop the cable into itself
            let def = winaudio::default_capture_id(0);
            let real = self
                .mics
                .iter()
                .find(|m| Some(&m.id) == def.as_ref() && !winaudio::is_virtual(&m.name))
                .or_else(|| self.mics.iter().find(|m| !winaudio::is_virtual(&m.name)));
            if let Some(m) = real {
                self.cfg.input = m.name.clone();
            }
        }
        self.cfg.output = out;
        if self.cfg.auto_default {
            if self.cfg.prev_default.is_none() {
                let c = winaudio::default_capture_id(0).unwrap_or_default();
                let m = winaudio::default_capture_id(2).unwrap_or_default();
                if c != cable_mic.id {
                    self.cfg.prev_default = Some((c, m));
                }
            }
            if let Err(e) = winaudio::set_default_capture(&cable_mic.id, &[0, 1, 2]) {
                self.route_msg = Some((format!("Couldn't switch the default microphone: {e}"), false));
            }
            self.save();
        } else {
            self.restore_default();
        }
    }

    /// Give Windows back the microphone it had before we switched it.
    pub fn restore_default(&mut self) {
        if let Some((c, m)) = self.cfg.prev_default.take() {
            if !c.is_empty() {
                let _ = winaudio::set_default_capture(&c, &[0, 1]);
            }
            if !m.is_empty() {
                let _ = winaudio::set_default_capture(&m, &[2]);
            }
            self.save();
        }
    }

    /// Take out the microphone add-on an older version installed (needs admin).
    pub fn remove_old_addon(&mut self) {
        self.cleanup_busy = true;
        let tx = self.jobs_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Job::Cleanup(winaudio::run_elevated("--apo-uninstall")));
        });
    }

    pub fn load_carrier(&mut self) {
        let Some(p) = self.cfg.carrier_file.clone() else {
            if let Ok(mut c) = self.shared.carrier.lock() {
                *c = None;
            }
            return;
        };
        let rate = self.rate();
        let tx = self.jobs_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Job::Carrier(audio::load_clip(&p, rate)));
        });
    }

    pub fn speak(&mut self, text: String) {
        self.tts_busy = true;
        self.tts_status = "Preparing speech…".into();
        let voice = self.cfg.tts_voice.clone();
        let rate = self.cfg.tts_rate;
        let sr = self.rate();
        let tx = self.jobs_tx.clone();
        std::thread::spawn(move || {
            let r = tts::speak_to_file(&text, &voice, rate).and_then(|p| {
                let c = audio::load_clip(&p, sr);
                let _ = std::fs::remove_file(&p);
                c
            });
            let _ = tx.send(Job::Speech(r));
        });
    }

    /// Audio restarted at a (maybe) new rate: cached clips must be made again.
    pub fn audio_restarted(&mut self) {
        self.music_cache = None;
        self.load_carrier();
        if self.cfg.app_on {
            self.restart_app_capture();
        }
    }

    // ------------------------------------------------------------ background events

    fn on_hotkey(&mut self, hk: Hotkey) {
        match hk {
            Hotkey::Toggle => self.toggle_enabled(),
            Hotkey::PrevVoice => self.cycle_voice(-1),
            Hotkey::NextVoice => self.cycle_voice(1),
            Hotkey::StopSound => self.stop_all(),
            Hotkey::MusicNext => {
                if self.music_index.is_none() {
                    self.toggle_play(None);
                } else {
                    self.next_track(false);
                }
            }
            Hotkey::PlayAssigned => {
                if let Some(id) = self.cfg.starred.clone() {
                    self.play_sound_id(&id);
                }
            }
            Hotkey::Slot(s) => self.play_slot(s),
        }
    }

    fn on_job(&mut self, job: Job) {
        match job {
            Job::Music(i, Ok(c)) => {
                if self.music_index == Some(i) {
                    self.shared.send(Cmd::Music(c.clone()));
                    self.music_cache = Some((i, c));
                }
                self.music_loading = false;
            }
            Job::Music(_, Err(e)) => {
                self.music_loading = false;
                self.music_index = None;
                self.notify(format!("Couldn't open that song: {e}"));
            }
            Job::Speech(r) => {
                self.tts_busy = false;
                match r {
                    Ok(c) => {
                        self.shared.send(Cmd::Sound(SoundPlay { id: "speech".into(), clip: c, volume: 1.0, speed: 1.0, looped: false, exclusive: false }));
                        self.tts_status = "Speaking into your microphone…".into();
                    }
                    Err(e) => self.tts_status = e,
                }
            }
            Job::Voices(v) => self.voices = v,
            Job::Carrier(r) => match r {
                Ok(c) => {
                    if let Ok(mut s) = self.shared.carrier.lock() {
                        *s = Some(c);
                    }
                }
                Err(e) => self.notify(format!("Couldn't open sound file: {e}")),
            },
            Job::Cleanup(r) => {
                self.cleanup_busy = false;
                self.refresh_devices();
                self.route_msg = Some(match r {
                    Ok(m) => (m, true),
                    Err(e) => (e, false),
                });
                self.apply_route();
                self.need_restart = true;
            }
        }
    }
}

/// Start the hotkey poller and the background thread. Returns the sender the tray uses.
pub fn spawn_brain(core: Arc<Mutex<Core>>, ctx: egui::Context) -> Sender<TrayCmd> {
    let (jobs_tx, jobs_rx): (Sender<Job>, Receiver<Job>) = channel();
    let (hk_tx, hk_rx) = channel();
    let (tray_tx, tray_rx) = channel();
    {
        let mut c = core.lock().unwrap();
        c.jobs_tx = jobs_tx.clone();
        hotkeys::spawn(c.mods.clone(), hk_tx);
        tray::VOICE_ON.store(c.params().enabled, Ordering::Relaxed);
    }
    std::thread::spawn(move || {
        let _ = jobs_tx.send(Job::Voices(tts::list_voices()));
    });
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(15));
        let Ok(mut c) = core.lock() else { return };
        let mut changed = false;
        while let Ok(hk) = hk_rx.try_recv() {
            c.on_hotkey(hk);
            changed = true;
        }
        while let Ok(job) = jobs_rx.try_recv() {
            c.on_job(job);
            changed = true;
        }
        while let Ok(t) = tray_rx.try_recv() {
            match t {
                TrayCmd::Toggle => c.toggle_enabled(),
                TrayCmd::StopAll => c.stop_all(),
            }
            changed = true;
        }
        changed |= c.app_tick();
        if c.shared.music_finished.swap(false, Ordering::Relaxed) && c.music_index.is_some() {
            c.next_track(true);
            changed = true;
        }
        drop(c);
        if changed {
            ctx.request_repaint();
        }
    });
    tray_tx
}
