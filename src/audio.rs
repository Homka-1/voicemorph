//! Microphone in, changed voice out (to a virtual cable or a speaker), plus a
//! second "hear yourself" output. The sound board and music player are mixed in here.

use crate::dsp::{Engine, Params, Tone};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[allow(unused_imports)]
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub type Clip = Arc<Vec<f32>>;

/// One press of a sound button.
pub struct SoundPlay {
    pub id: String,
    pub clip: Clip,
    pub volume: f32,
    pub speed: f32,
    pub looped: bool,
    /// Stop every other sound first ("one at a time").
    pub exclusive: bool,
}

pub enum Cmd {
    Sound(SoundPlay),
    StopSound(String),
    Music(Clip),
    StopSounds,
    StopMusic,
    PauseMusic(bool),
    /// Jump to this point of the song (0..1).
    SeekMusic(f64),
}

#[derive(Default)]
pub struct AtomicF32(AtomicU32);
impl AtomicF32 {
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// Music options the audio thread reads every block.
pub struct MusicOpts {
    pub speed: AtomicF32,
    pub fade_ms: AtomicU32,
    pub bass_db: AtomicF32,
    pub treble_db: AtomicF32,
    pub duck: AtomicBool,
    /// Music level while you talk (0..1).
    pub duck_level: AtomicF32,
    /// Others hear the music (off: only you, on your listening device).
    pub to_call: AtomicBool,
}

/// "App sound into my microphone" options the audio thread reads every block.
pub struct AppOpts {
    pub on: AtomicBool,
    pub volume: AtomicF32,
    /// Send it through the voice effect.
    pub fx: AtomicBool,
    pub level: AtomicF32,
}

pub struct Shared {
    /// Sound recorded from the chosen app (mono, at the engine rate).
    pub app_ring: Arc<Mutex<VecDeque<f32>>>,
    pub app: AppOpts,
    pub params: Mutex<Params>,
    pub cmds: Mutex<Vec<Cmd>>,
    pub carrier: Mutex<Option<Clip>>,
    pub in_level: AtomicF32,
    pub out_level: AtomicF32,
    pub music_pos: AtomicU64,
    pub music_len: AtomicU64,
    pub music_finished: AtomicBool,
    pub music_ducking: AtomicBool,
    /// Sounds playing now: id and how far along (0..1).
    pub playing: Mutex<Vec<(String, f32)>>,
    pub hear_voice: AtomicBool,
    pub hear_sounds: AtomicBool,
    pub local_volume: AtomicF32,
    pub engine_rate: AtomicU32,
    pub music: MusicOpts,
}

impl Shared {
    pub fn new(p: Params) -> Arc<Self> {
        let s = Shared {
            app_ring: Arc::new(Mutex::new(VecDeque::new())),
            app: AppOpts { on: AtomicBool::new(false), volume: AtomicF32::default(), fx: AtomicBool::new(false), level: AtomicF32::default() },
            params: Mutex::new(p),
            cmds: Mutex::new(Vec::new()),
            carrier: Mutex::new(None),
            in_level: AtomicF32::default(),
            out_level: AtomicF32::default(),
            music_pos: AtomicU64::new(0),
            music_len: AtomicU64::new(0),
            music_finished: AtomicBool::new(false),
            music_ducking: AtomicBool::new(false),
            playing: Mutex::new(Vec::new()),
            hear_voice: AtomicBool::new(false),
            hear_sounds: AtomicBool::new(true),
            local_volume: AtomicF32::default(),
            engine_rate: AtomicU32::new(48000),
            music: MusicOpts {
                speed: AtomicF32::default(),
                fade_ms: AtomicU32::new(0),
                bass_db: AtomicF32::default(),
                treble_db: AtomicF32::default(),
                duck: AtomicBool::new(false),
                duck_level: AtomicF32::default(),
                to_call: AtomicBool::new(true),
            },
        };
        s.local_volume.set(0.8);
        s.music.speed.set(1.0);
        s.music.duck_level.set(0.3);
        s.app.volume.set(1.0);
        Arc::new(s)
    }
    pub fn send(&self, c: Cmd) {
        if let Ok(mut q) = self.cmds.lock() {
            q.push(c);
        }
    }
}

pub fn input_names() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices().map(|it| it.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
}

pub fn output_names() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices().map(|it| it.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
}

fn find_input(name: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if name.is_empty() {
        return host.default_input_device();
    }
    host.input_devices().ok()?.find(|d| d.name().map(|n| n == name).unwrap_or(false)).or_else(|| host.default_input_device())
}

fn find_output(name: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if name.is_empty() {
        return host.default_output_device();
    }
    host.output_devices().ok()?.find(|d| d.name().map(|n| n == name).unwrap_or(false)).or_else(|| host.default_output_device())
}

/// Linear resampler pulling from a queue.
struct Resampler {
    ratio: f64,
    pos: f64,
    a: f32,
    b: f32,
}

impl Resampler {
    fn new(ratio: f64) -> Self {
        Resampler { ratio, pos: 0.0, a: 0.0, b: 0.0 }
    }
    fn next(&mut self, q: &mut VecDeque<f32>, starved: &mut bool) -> f32 {
        while self.pos >= 1.0 {
            self.pos -= 1.0;
            self.a = self.b;
            match q.pop_front() {
                Some(v) => self.b = v,
                None => {
                    *starved = true;
                    self.b *= 0.98;
                }
            }
        }
        let y = self.a + (self.b - self.a) * self.pos as f32;
        self.pos += self.ratio;
        y
    }
}

/// Read a clip at a fractional position (linear between samples).
#[inline]
fn sample_at(clip: &[f32], pos: f64) -> f32 {
    let i = pos as usize;
    let f = (pos - i as f64) as f32;
    let a = clip.get(i).copied().unwrap_or(0.0);
    let b = clip.get(i + 1).copied().unwrap_or(a);
    a + (b - a) * f
}

struct SoundVoice {
    id: String,
    clip: Clip,
    pos: f64,
    vol: f32,
    speed: f64,
    looped: bool,
    /// fades to zero, then is dropped
    stopping: bool,
    gain: f32,
}

struct Track {
    clip: Clip,
    pos: f64,
    gain: f32,
    target: f32,
}

/// Microphone level above which "lower music while I talk" kicks in (≈ −30 dB, estimated).
const DUCK_THRESHOLD: f32 = 0.03;
/// How long music stays lowered after you stop talking (estimated).
const DUCK_HOLD_MS: f32 = 400.0;
/// How fast the music glides down/up when ducking (estimated).
const DUCK_GLIDE_MS: f32 = 80.0;
/// App sound waits for this much in the queue before it starts (smooths out timing).
const APP_PRIME_MS: f32 = 20.0;
/// Short ramp so stopping a sound never clicks.
const CLICK_RAMP_MS: f32 = 8.0;

/// Sound board and music player, mixed per audio block.
struct Mixer {
    sounds: Vec<SoundVoice>,
    music: Option<Track>,
    /// the previous song while it fades out
    outgoing: Option<Track>,
    paused: bool,
    near_end_sent: bool,
    duck_gain: f32,
    talk_hold: u32,
    tone: Tone,
    mbuf: Vec<f32>,
    app_primed: bool,
    sr: f32,
}

impl Mixer {
    fn new(sr: f32) -> Self {
        Mixer {
            sounds: Vec::new(),
            music: None,
            outgoing: None,
            paused: false,
            near_end_sent: false,
            duck_gain: 1.0,
            talk_hold: 0,
            tone: Tone::new(sr),
            mbuf: Vec::new(),
            app_primed: false,
            sr,
        }
    }

    fn command(&mut self, c: Cmd, sh: &Shared) {
        let fade = sh.music.fade_ms.load(Ordering::Relaxed);
        match c {
            Cmd::Sound(p) => {
                if p.exclusive {
                    self.sounds.iter_mut().for_each(|v| v.stopping = true);
                }
                self.sounds.push(SoundVoice {
                    id: p.id,
                    clip: p.clip,
                    pos: 0.0,
                    vol: p.volume,
                    speed: p.speed.clamp(0.25, 4.0) as f64,
                    looped: p.looped,
                    stopping: false,
                    gain: 1.0,
                });
            }
            Cmd::StopSound(id) => self.sounds.iter_mut().filter(|v| v.id == id).for_each(|v| v.stopping = true),
            Cmd::StopSounds => self.sounds.iter_mut().for_each(|v| v.stopping = true),
            Cmd::Music(clip) => {
                sh.music_len.store(clip.len() as u64, Ordering::Relaxed);
                sh.music_finished.store(false, Ordering::Relaxed);
                self.near_end_sent = false;
                if let Some(mut o) = self.music.take() {
                    if fade > 0 && !self.paused {
                        o.target = 0.0;
                        self.outgoing = Some(o);
                    }
                }
                self.music = Some(Track { clip, pos: 0.0, gain: if fade > 0 { 0.0 } else { 1.0 }, target: 1.0 });
                self.paused = false;
            }
            Cmd::StopMusic => {
                if let Some(mut o) = self.music.take() {
                    if !self.paused {
                        o.target = 0.0;
                        self.outgoing = Some(o);
                    }
                }
                self.paused = false;
                sh.music_pos.store(0, Ordering::Relaxed);
            }
            Cmd::PauseMusic(p) => self.paused = p,
            Cmd::SeekMusic(f) => {
                if let Some(m) = &mut self.music {
                    m.pos = (f.clamp(0.0, 1.0) * m.clip.len() as f64).min(m.clip.len().saturating_sub(1) as f64);
                    self.near_end_sent = false;
                }
            }
        }
    }

    /// `mic` is the raw microphone block (for ducking). Fills `pre`/`post` (to the call,
    /// before/after the voice effect), `local` (sounds and music you hear) and
    /// `local_pre` (the part of `local` that also went into `pre`).
    #[allow(clippy::too_many_arguments)]
    fn fill(&mut self, mic: &[f32], pre: &mut [f32], post: &mut [f32], local: &mut [f32], local_pre: &mut [f32], p: &Params, sh: &Shared) {
        let n = pre.len();
        for b in [&mut *pre, &mut *post, &mut *local, &mut *local_pre] {
            b.iter_mut().for_each(|v| *v = 0.0);
        }
        let ramp = 1.0 / (self.sr * CLICK_RAMP_MS / 1000.0);

        // ---- sounds
        {
            let fx = p.effects_on_sounds;
            let dst: &mut [f32] = if fx { &mut *pre } else { &mut *post };
            for v in self.sounds.iter_mut() {
                let len = v.clip.len() as f64;
                for ((o, l), lp) in dst.iter_mut().zip(local.iter_mut()).zip(local_pre.iter_mut()) {
                    if v.pos >= len {
                        if v.looped && !v.stopping && len > 1.0 {
                            v.pos -= len;
                        } else {
                            break;
                        }
                    }
                    if v.stopping {
                        v.gain -= ramp;
                        if v.gain <= 0.0 {
                            v.gain = 0.0;
                            break;
                        }
                    }
                    let s = sample_at(&v.clip, v.pos) * v.vol * p.sound_volume * v.gain;
                    *o += s;
                    *l += s;
                    if fx {
                        *lp += s;
                    }
                    v.pos += v.speed;
                }
            }
            self.sounds.retain(|v| v.gain > 0.0 && ((v.looped && !v.stopping) || v.pos < v.clip.len() as f64));
            if let Ok(mut pl) = sh.playing.try_lock() {
                pl.clear();
                pl.extend(
                    self.sounds
                        .iter()
                        .filter(|v| !v.stopping)
                        .map(|v| (v.id.clone(), (v.pos / v.clip.len().max(1) as f64).min(1.0) as f32)),
                );
            }
        }

        // ---- ducking: follow the microphone
        let duck_on = sh.music.duck.load(Ordering::Relaxed);
        let peak = mic.iter().fold(0.0f32, |m, v| m.max(v.abs())) * p.mic_gain;
        let block_ms = (n as f32 / self.sr * 1000.0).max(0.1);
        if peak > DUCK_THRESHOLD {
            self.talk_hold = (DUCK_HOLD_MS / block_ms) as u32 + 1;
        } else {
            self.talk_hold = self.talk_hold.saturating_sub(1);
        }
        let talking = self.talk_hold > 0;
        let duck_target = if duck_on && talking { sh.music.duck_level.get().clamp(0.0, 1.0) } else { 1.0 };
        sh.music_ducking.store(duck_on && talking && self.music.is_some() && !self.paused, Ordering::Relaxed);

        // ---- music
        let fade = sh.music.fade_ms.load(Ordering::Relaxed) as f32;
        let fstep = if fade > 0.0 { 1.0 / (self.sr * fade / 1000.0) } else { 1.0 };
        let speed = sh.music.speed.get().clamp(0.25, 3.0) as f64;
        self.mbuf.resize(n, 0.0);
        self.mbuf.iter_mut().for_each(|v| *v = 0.0);
        let render = |t: &mut Track, out: &mut [f32]| -> bool {
            let len = t.clip.len() as f64;
            for o in out.iter_mut() {
                if t.pos >= len {
                    return true;
                }
                if t.gain < t.target {
                    t.gain = (t.gain + fstep).min(t.target);
                } else if t.gain > t.target {
                    t.gain = (t.gain - fstep.max(ramp)).max(t.target);
                }
                *o += sample_at(&t.clip, t.pos) * t.gain;
                t.pos += speed;
            }
            false
        };
        if let Some(o) = &mut self.outgoing {
            if render(o, &mut self.mbuf) || o.gain <= 0.0 {
                self.outgoing = None;
            }
        }
        if let Some(t) = &mut self.music {
            if !self.paused {
                let ended = render(t, &mut self.mbuf);
                // tell the player early, so the next song can cross-fade in
                let left = (t.clip.len() as f64 - t.pos) / speed;
                if !self.near_end_sent && fade > 0.0 && left < (self.sr * fade / 1000.0) as f64 {
                    self.near_end_sent = true;
                    sh.music_finished.store(true, Ordering::Relaxed);
                }
                if ended {
                    if !self.near_end_sent {
                        sh.music_finished.store(true, Ordering::Relaxed);
                    }
                    self.music = None;
                }
            }
        }
        if let Some(t) = &self.music {
            sh.music_pos.store(t.pos as u64, Ordering::Relaxed);
        }
        self.tone.process(&mut self.mbuf, sh.music.bass_db.get(), sh.music.treble_db.get());
        let to_call = sh.music.to_call.load(Ordering::Relaxed);
        let mfx = p.effects_on_music && to_call;
        let dst: &mut [f32] = if mfx { &mut *pre } else { &mut *post };
        let dstep = 1.0 / (self.sr * DUCK_GLIDE_MS / 1000.0);
        for i in 0..n {
            if self.duck_gain < duck_target {
                self.duck_gain = (self.duck_gain + dstep).min(duck_target);
            } else if self.duck_gain > duck_target {
                self.duck_gain = (self.duck_gain - dstep).max(duck_target);
            }
            let m = self.mbuf[i] * p.music_volume;
            if to_call {
                dst[i] += m * self.duck_gain;
            }
            local[i] += m;
            if mfx {
                local_pre[i] += m;
            }
        }

        // ---- another app's sound (you already hear it from the app itself)
        let mut peak = 0.0f32;
        if sh.app.on.load(Ordering::Relaxed) {
            let vol = sh.app.volume.get();
            let dst: &mut [f32] = if sh.app.fx.load(Ordering::Relaxed) { pre } else { post };
            if let Ok(mut q) = sh.app_ring.try_lock() {
                if !self.app_primed && q.len() as f32 >= self.sr * APP_PRIME_MS / 1000.0 {
                    self.app_primed = true;
                }
                if self.app_primed {
                    for o in dst.iter_mut() {
                        match q.pop_front() {
                            Some(v) => {
                                let s = v * vol;
                                peak = peak.max(s.abs());
                                *o += s;
                            }
                            None => {
                                self.app_primed = false;
                                break;
                            }
                        }
                    }
                }
            }
        } else if let Ok(mut q) = sh.app_ring.try_lock() {
            q.clear();
            self.app_primed = false;
        }
        let old = sh.app.level.get();
        sh.app.level.set(peak.max(old * 0.85));
    }
}

pub struct AudioSystem {
    _streams: Vec<cpal::Stream>,
    /// Short status lines for the side bar: (what, device).
    pub lines: Vec<(&'static str, String)>,
}

fn build_input<T>(
    dev: &cpal::Device,
    cfg: &cpal::StreamConfig,
    ring: Arc<Mutex<VecDeque<f32>>>,
    sh: Arc<Shared>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let ch = cfg.channels as usize;
    let cap = cfg.sample_rate.0 as usize / 2;
    dev.build_input_stream(
        cfg,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut peak = 0.0f32;
            if let Ok(mut q) = ring.lock() {
                for f in data.chunks(ch) {
                    let mut s = 0.0;
                    for v in f {
                        s += (*v).to_sample::<f32>();
                    }
                    let m = s / ch as f32;
                    peak = peak.max(m.abs());
                    q.push_back(m);
                }
                while q.len() > cap {
                    q.pop_front();
                }
            }
            let old = sh.in_level.get();
            sh.in_level.set(peak.max(old * 0.85));
        },
        |e| note!("input error: {e}"),
        None,
    )
}

struct OutState {
    engine: Engine,
    mixer: Mixer,
    mic_rs: Resampler,
    mic: Arc<Mutex<VecDeque<f32>>>,
    mon: Option<Arc<Mutex<VecDeque<f32>>>>,
    /// Second effect chain, only for the sounds/music you hear when your own
    /// changed voice isn't played back to you.
    local_fx: Engine,
    in_rate: f64,
    mono: Vec<f32>,
    raw: Vec<f32>,
    pre: Vec<f32>,
    post: Vec<f32>,
    local: Vec<f32>,
    local_pre: Vec<f32>,
    params: Params,
}

impl OutState {
    fn render(&mut self, frames: usize, sh: &Shared) {
        if let Ok(p) = sh.params.try_lock() {
            self.params = p.clone();
        }
        if let Ok(mut q) = sh.cmds.try_lock() {
            for c in q.drain(..) {
                self.mixer.command(c, sh);
            }
        }
        for b in [&mut self.mono, &mut self.raw, &mut self.pre, &mut self.post, &mut self.local, &mut self.local_pre] {
            b.resize(frames, 0.0);
        }
        {
            let mut starved = false;
            if let Ok(mut q) = self.mic.lock() {
                // keep latency low: never sit on more than ~60 ms of microphone
                let max = (self.in_rate * 0.06) as usize + frames;
                if q.len() > max {
                    let drop = q.len() - (self.in_rate * 0.02) as usize;
                    q.drain(0..drop);
                }
                for v in self.mono.iter_mut() {
                    *v = self.mic_rs.next(&mut q, &mut starved);
                }
            } else {
                self.mono.iter_mut().for_each(|v| *v = 0.0);
            }
        }
        self.raw.copy_from_slice(&self.mono);
        self.mixer.fill(&self.raw, &mut self.pre, &mut self.post, &mut self.local, &mut self.local_pre, &self.params, sh);
        let carrier = sh.carrier.try_lock().ok().and_then(|c| c.clone());
        let carrier = carrier.as_ref().map(|c| c.as_slice());
        self.engine.process(&mut self.mono, &self.pre, &self.post, &self.params, carrier);
        let pk = self.mono.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let old = sh.out_level.get();
        sh.out_level.set(pk.max(old * 0.85));
        if let Some(mon) = &self.mon {
            let hv = sh.hear_voice.load(Ordering::Relaxed);
            let hs = sh.hear_sounds.load(Ordering::Relaxed);
            let lv = sh.local_volume.get();
            // Sounds/music that go through the effect: you hear them changed. With your
            // own voice played back they're already in the call mix; without it they
            // run through a second effect chain of their own.
            let changed_local = hs && !hv && self.local_pre.iter().any(|v| *v != 0.0);
            if changed_local {
                // swap the plain copy in `local` for the changed one
                for (l, v) in self.local.iter_mut().zip(&self.local_pre) {
                    *l -= *v;
                }
                self.local_fx.process_effects_only(&mut self.local_pre, &self.params, carrier);
                for (l, v) in self.local.iter_mut().zip(&self.local_pre) {
                    *l += *v;
                }
            }
            if let Ok(mut q) = mon.lock() {
                for i in 0..frames {
                    let mut m = 0.0;
                    if hv {
                        m += self.mono[i] - self.post[i];
                    }
                    if hs {
                        // `local` holds the plain copy of everything you hear
                        m += self.local[i];
                        if hv {
                            m -= self.local_pre[i]; // the changed copy came with the voice
                        }
                    }
                    q.push_back((m * lv).clamp(-1.0, 1.0));
                }
                let cap = self.engine.sr as usize / 4;
                while q.len() > cap {
                    q.pop_front();
                }
            }
        }
    }
}

fn build_output<T>(
    dev: &cpal::Device,
    cfg: &cpal::StreamConfig,
    mut st: OutState,
    sh: Arc<Shared>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
{
    let ch = cfg.channels as usize;
    dev.build_output_stream(
        cfg,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / ch;
            st.render(frames, &sh);
            for (f, v) in data.chunks_mut(ch).zip(&st.mono) {
                for s in f {
                    *s = T::from_sample(*v);
                }
            }
        },
        |e| note!("output error: {e}"),
        None,
    )
}

fn build_monitor<T>(
    dev: &cpal::Device,
    cfg: &cpal::StreamConfig,
    ring: Arc<Mutex<VecDeque<f32>>>,
    ratio: f64,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
{
    let ch = cfg.channels as usize;
    let mut rs = Resampler::new(ratio);
    let target = (cfg.sample_rate.0 as f64 * 0.05) as usize;
    dev.build_output_stream(
        cfg,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut starved = false;
            if let Ok(mut q) = ring.lock() {
                if q.len() > target * 3 {
                    let d = q.len() - target;
                    q.drain(0..d);
                }
                for f in data.chunks_mut(ch) {
                    let v = rs.next(&mut q, &mut starved);
                    for s in f {
                        *s = T::from_sample(v);
                    }
                }
            }
        },
        |e| note!("monitor error: {e}"),
        None,
    )
}

macro_rules! by_format {
    ($fmt:expr, $f:ident, $($args:expr),*) => {
        match $fmt {
            SampleFormat::F32 => $f::<f32>($($args),*),
            SampleFormat::I16 => $f::<i16>($($args),*),
            SampleFormat::U16 => $f::<u16>($($args),*),
            SampleFormat::I32 => $f::<i32>($($args),*),
            other => return Err(format!("unsupported sample format {other:?}")),
        }
    };
}

impl AudioSystem {
    pub fn start(input: &str, output: &str, monitor: &str, sh: Arc<Shared>) -> Result<AudioSystem, String> {
        let out_dev = find_output(output).ok_or("No speaker or output device found")?;
        let out_cfg = out_dev.default_output_config().map_err(|e| e.to_string())?;
        let out_rate = out_cfg.sample_rate().0;
        sh.engine_rate.store(out_rate, Ordering::Relaxed);
        let mut streams = Vec::new();
        let mic = Arc::new(Mutex::new(VecDeque::<f32>::new()));
        let mut in_rate = out_rate as f64;
        let mut lines: Vec<(&'static str, String)> = Vec::new();
        if let Some(in_dev) = find_input(input) {
            match in_dev.default_input_config() {
                Ok(in_cfg) => {
                    in_rate = in_cfg.sample_rate().0 as f64;
                    let c = in_cfg.config();
                    let s = by_format!(in_cfg.sample_format(), build_input, &in_dev, &c, mic.clone(), sh.clone())
                        .map_err(|e| e.to_string())?;
                    s.play().map_err(|e| e.to_string())?;
                    streams.push(s);
                    lines.push(("Mic", in_dev.name().unwrap_or_default()));
                }
                Err(e) => lines.push(("Mic", format!("unavailable ({e})"))),
            }
        } else {
            lines.push(("Mic", "none found".into()));
        }
        let mon_ring = if monitor.is_empty() { None } else { Some(Arc::new(Mutex::new(VecDeque::new()))) };
        let st = OutState {
            engine: Engine::new(out_rate as f32),
            mixer: Mixer::new(out_rate as f32),
            mic_rs: Resampler::new(in_rate / out_rate as f64),
            mic,
            mon: mon_ring.clone(),
            local_fx: Engine::new(out_rate as f32),
            in_rate,
            mono: Vec::new(),
            raw: Vec::new(),
            pre: Vec::new(),
            post: Vec::new(),
            local: Vec::new(),
            local_pre: Vec::new(),
            params: sh.params.lock().map(|p| p.clone()).unwrap_or_default(),
        };
        let oc = out_cfg.config();
        let s = by_format!(out_cfg.sample_format(), build_output, &out_dev, &oc, st, sh.clone()).map_err(|e| e.to_string())?;
        s.play().map_err(|e| e.to_string())?;
        streams.push(s);
        lines.push(("Out", format!("{} · {} kHz", out_dev.name().unwrap_or_default(), out_rate / 1000)));
        if let Some(ring) = mon_ring {
            if let Some(md) = find_output(monitor) {
                if let Ok(mc) = md.default_output_config() {
                    let ratio = out_rate as f64 / mc.sample_rate().0 as f64;
                    let c = mc.config();
                    if let Ok(s) = by_format!(mc.sample_format(), build_monitor, &md, &c, ring, ratio) {
                        if s.play().is_ok() {
                            streams.push(s);
                            lines.push(("Hear on", md.name().unwrap_or_default()));
                        }
                    }
                }
            }
        }
        Ok(AudioSystem { _streams: streams, lines })
    }
}

// ---------------------------------------------------------------- files

pub fn decode_file(path: &Path) -> Result<(Vec<f32>, u32), String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| e.to_string())?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("no audio track")?;
    let track_id = track.id;
    let mut sr = track.codec_params.sample_rate.unwrap_or(44100);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = *buf.spec();
                sr = spec.rate;
                let ch = spec.channels.count().max(1);
                let mut sb = SampleBuffer::<f32>::new(buf.capacity() as u64, spec);
                sb.copy_interleaved_ref(buf);
                for f in sb.samples().chunks(ch) {
                    out.push(f.iter().sum::<f32>() / ch as f32);
                }
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(_) => break,
        }
    }
    if out.is_empty() {
        return Err("no audio in file".into());
    }
    Ok((out, sr))
}

pub fn resample(data: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || data.is_empty() {
        return data.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (data.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * ratio;
            let k = p as usize;
            let f = (p - k as f64) as f32;
            let a = data[k.min(data.len() - 1)];
            let b = data[(k + 1).min(data.len() - 1)];
            a + (b - a) * f
        })
        .collect()
}

pub fn load_clip(path: &Path, rate: u32) -> Result<Clip, String> {
    let (d, sr) = decode_file(path)?;
    Ok(Arc::new(resample(&d, sr, rate)))
}

/// Small sounds made in code, so the board is never empty.
pub fn synth_sound(name: &str, rate: u32) -> Clip {
    let sr = rate as f32;
    let tone = |dur: f32, f: &dyn Fn(f32) -> f32| -> Vec<f32> {
        let n = (dur * sr) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let env = (1.0 - t / dur).max(0.0).min((t * 200.0).min(1.0));
                f(t) * env
            })
            .collect()
    };
    let two_pi = std::f32::consts::TAU;
    let v = match name {
        "Ding" => tone(1.2, &|t| ((two_pi * 1318.5 * t).sin() * 0.5 + (two_pi * 2637.0 * t).sin() * 0.2) * (-3.0 * t).exp()),
        "Buzzer" => tone(0.8, &|t| if (t * 140.0).fract() < 0.5 { 0.35 } else { -0.35 }),
        "Laser" => tone(0.5, &|t| (two_pi * (1800.0 * (1.0 - t * 1.6)).max(80.0) * t).sin() * 0.5),
        "Drum roll" => tone(1.5, &move |t| {
            let mut s = ((t * 96000.0) as u32).wrapping_mul(2654435761);
            s ^= s >> 15;
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            let noise = ((s >> 9) as f32 / 8388608.0) * 2.0 - 1.0;
            noise * 0.4 * (0.5 + 0.5 * (two_pi * 18.0 * t).sin().abs())
        }),
        "Clap" => tone(2.0, &move |t| {
            let mut s = ((t * 48000.0) as u32).wrapping_mul(2246822519);
            s ^= s >> 13;
            s = s.wrapping_mul(3266489917);
            let noise = ((s >> 8) as f32 / 16777216.0) * 2.0 - 1.0;
            let claps = 0.4 + 0.6 * ((t * 23.0).fract() < 0.3) as u8 as f32;
            noise * 0.35 * claps
        }),
        "Boing" => tone(0.9, &|t| (two_pi * (220.0 + 180.0 * (t * 18.0).sin() * (-3.0 * t).exp()) * t).sin() * 0.5),
        _ => tone(0.4, &|t| (two_pi * 880.0 * t).sin() * 0.4), // Beep
    };
    Arc::new(v)
}

pub const SYNTH_SOUNDS: [&str; 7] = ["Beep", "Ding", "Buzzer", "Laser", "Drum roll", "Clap", "Boing"];
