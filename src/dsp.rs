//! Voice processing. Every number here comes from MECHANICS.md (decompiled from the
//! original's audio-processing module) unless marked "estimated" there.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Length of one processing step used by the time-varying effects (the original
/// updates them once per audio period). MECHANICS: "update period" (estimated 10 ms).
pub const STEP_MS: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
pub enum Voice {
    None,
    Alien,
    Atari,
    Clone,
    Mutation,
    FastMutation,
    SlowMutation,
    Male,
    Female,
    Helium,
    Baby,
    Radio,
    Robot,
    Custom,
    Silence,
    Chainer,
    // New in this version
    Deep,
    Demon,
    Telephone,
    Megaphone,
    Underwater,
}

impl Voice {
    /// Order the original cycles through with the previous/next hotkeys (table 0x4c8c50),
    /// followed by the new effects.
    pub const ALL: [Voice; 21] = [
        Voice::None,
        Voice::Alien,
        Voice::Atari,
        Voice::Clone,
        Voice::Mutation,
        Voice::FastMutation,
        Voice::SlowMutation,
        Voice::Male,
        Voice::Female,
        Voice::Helium,
        Voice::Baby,
        Voice::Radio,
        Voice::Robot,
        Voice::Custom,
        Voice::Silence,
        Voice::Chainer,
        Voice::Deep,
        Voice::Demon,
        Voice::Telephone,
        Voice::Megaphone,
        Voice::Underwater,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Voice::None => "Clean",
            Voice::Alien => "Alien",
            Voice::Atari => "8-bit",
            Voice::Clone => "Clone",
            Voice::Mutation => "Mutation",
            Voice::FastMutation => "Fast mutation",
            Voice::SlowMutation => "Slow mutation",
            Voice::Male => "Male",
            Voice::Female => "Female",
            Voice::Helium => "Helium",
            Voice::Baby => "Baby",
            Voice::Radio => "Old radio",
            Voice::Robot => "Robot",
            Voice::Custom => "Custom pitch",
            Voice::Silence => "Mute",
            Voice::Chainer => "Chain",
            Voice::Deep => "Deep",
            Voice::Demon => "Demon",
            Voice::Telephone => "Telephone",
            Voice::Megaphone => "Megaphone",
            Voice::Underwater => "Underwater",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Voice::None => "🎤",
            Voice::Alien => "👽",
            Voice::Atari => "👾",
            Voice::Clone => "👥",
            Voice::Mutation => "🌀",
            Voice::FastMutation => "⚡",
            Voice::SlowMutation => "🐌",
            Voice::Male => "♂",
            Voice::Female => "♀",
            Voice::Helium => "🎈",
            Voice::Baby => "👶",
            Voice::Radio => "📻",
            Voice::Robot => "🔌",
            Voice::Custom => "↕",
            Voice::Silence => "🔇",
            Voice::Chainer => "🔗",
            Voice::Deep => "🐻",
            Voice::Demon => "😈",
            Voice::Telephone => "📞",
            Voice::Megaphone => "📢",
            Voice::Underwater => "🌊",
        }
    }

    pub fn is_new(self) -> bool {
        matches!(
            self,
            Voice::Deep | Voice::Demon | Voice::Telephone | Voice::Megaphone | Voice::Underwater
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Room {
    None,
    Cave,
    CityHall,
    Chorus,
    Ghost,
    Reverb,
}

impl Room {
    pub const ALL: [Room; 6] = [Room::None, Room::Cave, Room::CityHall, Room::Chorus, Room::Ghost, Room::Reverb];
    pub fn name(self) -> &'static str {
        match self {
            Room::None => "None",
            Room::Cave => "Cave",
            Room::CityHall => "City hall",
            Room::Chorus => "Chorus",
            Room::Ghost => "Ghost",
            Room::Reverb => "Big room",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Room::None => "⭕",
            Room::Cave => "🌋",
            Room::CityHall => "🏢",
            Room::Chorus => "🎶",
            Room::Ghost => "👻",
            Room::Reverb => "🎪",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum CarrierKind {
    Saw,
    Chord,
    Buzz,
    Whisper,
    File,
}

impl CarrierKind {
    pub const ALL: [CarrierKind; 5] = [CarrierKind::Saw, CarrierKind::Chord, CarrierKind::Buzz, CarrierKind::Whisper, CarrierKind::File];
    pub fn name(self) -> &'static str {
        match self {
            CarrierKind::Saw => "Synth",
            CarrierKind::Chord => "Choir chord",
            CarrierKind::Buzz => "Buzz",
            CarrierKind::Whisper => "Whisper",
            CarrierKind::File => "Sound file",
        }
    }
}

/// Everything the audio thread needs to know. Copied once per audio callback.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Params {
    pub enabled: bool,
    pub voice: Voice,
    pub custom_pitch: f32,
    pub chain: [Voice; 4],
    pub room: Room,
    pub room_mix: f32,
    pub vocoder: bool,
    pub carrier: CarrierKind,
    pub carrier_note: f32,
    pub vocoder_mix: f32,
    pub bass_db: f32,
    pub treble_db: f32,
    pub denoise: bool,
    pub denoise_strength: f32,
    pub gate: u8,
    pub mic_gain: f32,
    pub boost: u8,
    pub sound_volume: f32,
    pub music_volume: f32,
    pub effects_on_sounds: bool,
    /// Original setting APPLY_MUSIC_EFFECTS (exe 0x450c0f): voice effect also changes the music.
    pub effects_on_music: bool,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            enabled: true,
            voice: Voice::None,
            custom_pitch: 4.5,
            chain: [Voice::Male, Voice::Robot, Voice::None, Voice::None],
            room: Room::None,
            room_mix: 1.0,
            vocoder: false,
            carrier: CarrierKind::Chord,
            carrier_note: 48.0,
            vocoder_mix: 1.0,
            bass_db: 0.0,
            treble_db: 0.0,
            denoise: false,
            denoise_strength: 1.0,
            gate: 0,
            mic_gain: 1.0,
            boost: 0,
            sound_volume: 0.8,
            music_volume: 0.5,
            effects_on_sounds: false,
            effects_on_music: false,
        }
    }
}

// ---------------------------------------------------------------- building blocks

/// Ring buffer delay line.
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Delay { buf: vec![0.0; len.max(1)], pos: 0 }
    }
    /// Read the sample written `len` steps ago (the full length of the line), then store x.
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.buf[self.pos];
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        y
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }
}

#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn from(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Biquad {
            b0: (b0 / a0) as f32,
            b1: (b1 / a0) as f32,
            b2: (b2 / a0) as f32,
            a1: (a1 / a0) as f32,
            a2: (a2 / a0) as f32,
            z1: 0.0,
            z2: 0.0,
        }
    }
    fn identity() -> Self {
        Biquad { b0: 1.0, ..Default::default() }
    }
    pub fn bandpass(sr: f32, f: f32, q: f32) -> Self {
        let w = 2.0 * std::f64::consts::PI * (f as f64) / sr as f64;
        let alpha = w.sin() / (2.0 * q as f64);
        Self::from(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * w.cos(), 1.0 - alpha)
    }
    pub fn lowpass(sr: f32, f: f32, q: f32) -> Self {
        let w = 2.0 * std::f64::consts::PI * (f as f64) / sr as f64;
        let alpha = w.sin() / (2.0 * q as f64);
        let c = w.cos();
        Self::from((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }
    pub fn highpass(sr: f32, f: f32, q: f32) -> Self {
        let w = 2.0 * std::f64::consts::PI * (f as f64) / sr as f64;
        let alpha = w.sin() / (2.0 * q as f64);
        let c = w.cos();
        Self::from((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }
    /// Shelf filters with the slope form used by the original's bass/treble stage.
    fn shelf(sr: f32, f: f32, gain_db: f32, slope: f64, high: bool) -> Self {
        if gain_db.abs() < 0.01 {
            return Self::identity();
        }
        let w = 2.0 * std::f64::consts::PI * f as f64 / sr as f64;
        let a = (10f64.ln() * gain_db as f64 / 40.0).exp();
        let b = ((a * a + 1.0) / slope - (a - 1.0) * (a - 1.0)).max(0.0).sqrt();
        let (c, s) = (w.cos(), w.sin());
        if !high {
            Self::from(
                a * ((a + 1.0) - (a - 1.0) * c + b * s),
                2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                a * ((a + 1.0) - (a - 1.0) * c - b * s),
                (a + 1.0) + (a - 1.0) * c + b * s,
                -2.0 * ((a - 1.0) + (a + 1.0) * c),
                (a + 1.0) + (a - 1.0) * c - b * s,
            )
        } else {
            Self::from(
                a * ((a + 1.0) + (a - 1.0) * c + b * s),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                a * ((a + 1.0) + (a - 1.0) * c - b * s),
                (a + 1.0) - (a - 1.0) * c + b * s,
                2.0 * ((a - 1.0) - (a + 1.0) * c),
                (a + 1.0) - (a - 1.0) * c - b * s,
            )
        }
    }
    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    fn set_coeffs(&mut self, o: &Biquad) {
        self.b0 = o.b0;
        self.b1 = o.b1;
        self.b2 = o.b2;
        self.a1 = o.a1;
        self.a2 = o.a2;
    }
}

/// Low-latency pitch shifter: two read taps sweep a delay line and cross-fade.
/// Window length follows the original's time-stretch sequence (40 ms).
pub struct PitchShifter {
    buf: Vec<f32>,
    mask: usize,
    w: usize,
    phase: f32,
    win: f32,
    ratio: f32,
}

impl PitchShifter {
    pub fn new(sr: f32) -> Self {
        let win = (SEQUENCE_MS / 1000.0 * sr).max(64.0);
        let size = ((win as usize) + 16).next_power_of_two();
        PitchShifter { buf: vec![0.0; size], mask: size - 1, w: 0, phase: 0.0, win, ratio: 1.0 }
    }
    pub fn set_semitones(&mut self, st: f32) {
        self.ratio = 2f32.powf(st / 12.0);
    }
    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let pos = self.w as f32 - delay;
        let pos = if pos < 0.0 { pos + self.buf.len() as f32 } else { pos };
        let i = pos as usize;
        let frac = pos - i as f32;
        let a = self.buf[i & self.mask];
        let b = self.buf[(i + 1) & self.mask];
        a + (b - a) * frac
    }
    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        self.buf[self.w] = x;
        if (self.ratio - 1.0).abs() < 1e-4 {
            self.w = (self.w + 1) & self.mask;
            return x;
        }
        self.phase += (1.0 - self.ratio) / self.win;
        self.phase -= self.phase.floor();
        let p2 = (self.phase + 0.5) % 1.0;
        let d1 = 2.0 + self.phase * self.win;
        let d2 = 2.0 + p2 * self.win;
        let g1 = (PI * self.phase).sin();
        let g2 = (PI * p2).sin();
        let y = self.read(d1) * g1 * g1 + self.read(d2) * g2 * g2;
        self.w = (self.w + 1) & self.mask;
        y
    }
}

// ---------------------------------------------------------------- constants (MECHANICS.md)

pub const SEQUENCE_MS: f32 = 40.0;
pub const PITCH_MALE: f32 = -3.0;
pub const PITCH_FEMALE: f32 = 4.0;
pub const PITCH_HELIUM: f32 = 8.0;
pub const PITCH_BABY: f32 = 12.0;
pub const MUTATION_MIN: f32 = -4.0;
pub const MUTATION_MAX: f32 = 13.0;
pub const MUTATION_STEP: f32 = 0.1;
pub const MUTATION_SLOW_STEP: f32 = 0.01;
pub const MUTATION_FAST_STEP: f32 = 0.3;
pub const ATARI_LOW: f32 = -3.0;
pub const ATARI_HIGH: f32 = 4.0;
pub const ATARI_UP_PITCH: f32 = 6.0;
pub const ATARI_DOWN_PITCH: f32 = -3.0;
pub const ROBOT_HZ: f32 = 25.0;
pub const CLONE_DELAY_SAMPLES: usize = 6000;
pub const CLONE_GAIN: f32 = 0.5;
pub const ALIEN_STEPS: usize = 20;
pub const CAVE_DELAY_MS: f32 = 62.5;
pub const CAVE_GAIN: f32 = 0.5;
pub const HALL_DELAY_MS: f32 = 250.0;
pub const HALL_GAIN: f32 = 0.25;
pub const CHORUS_DELAY_SAMPLES: usize = 400;
pub const GHOST_DELAY_MS: f32 = 500.0;
pub const GHOST_GAIN: f32 = 0.5;
pub const BASS_HZ: f32 = 250.0;
pub const TREBLE_HZ: f32 = 4000.0;
pub const SHELF_SLOPE: f64 = 0.4;
pub const GATE_HOLD_STEPS: u32 = 10;
pub const BOOST_PER_STEP: f32 = 0.05;
pub const DEEP_PITCH: f32 = -6.0;
pub const DEMON_PITCH: f32 = -7.0;

// ---------------------------------------------------------------- one voice effect

pub struct VoiceProc {
    sr: f32,
    kind: Voice,
    pitch: PitchShifter,
    // stepping effects
    step_len: usize,
    step_count: usize,
    sweep: f32,
    sweep_step: f32,
    atari_pitch: f32,
    // robot
    robot_phase: f32,
    // alien
    alien_a: Vec<f32>,
    alien_b: Vec<f32>,
    alien_i: usize,
    alien_fade: usize,
    // clone
    clone: Delay,
    // filters for new effects
    f1: Biquad,
    f2: Biquad,
    lfo: f32,
}

impl VoiceProc {
    pub fn new(sr: f32) -> Self {
        let step_len = ((STEP_MS / 1000.0) * sr) as usize;
        let alien_len = step_len * ALIEN_STEPS;
        VoiceProc {
            sr,
            kind: Voice::None,
            pitch: PitchShifter::new(sr),
            step_len: step_len.max(1),
            step_count: 0,
            sweep: 0.0,
            sweep_step: MUTATION_STEP,
            atari_pitch: ATARI_UP_PITCH,
            robot_phase: 0.0,
            alien_a: vec![0.0; alien_len],
            alien_b: vec![0.0; alien_len],
            alien_i: 0,
            alien_fade: step_len.max(1),
            clone: Delay::new(CLONE_DELAY_SAMPLES),
            f1: Biquad::identity(),
            f2: Biquad::identity(),
            lfo: 0.0,
        }
    }

    fn set_kind(&mut self, kind: Voice, custom: f32) {
        if kind != self.kind {
            self.kind = kind;
            self.sweep = 0.0;
            self.sweep_step = match kind {
                Voice::SlowMutation => MUTATION_SLOW_STEP,
                Voice::FastMutation => MUTATION_FAST_STEP,
                Voice::Atari => 1.0,
                _ => MUTATION_STEP,
            };
            self.clone.clear();
            let sr = self.sr;
            match kind {
                Voice::Telephone => {
                    self.f1 = Biquad::highpass(sr, 300.0, 0.7);
                    self.f2 = Biquad::lowpass(sr, 3400.0, 0.7);
                }
                Voice::Megaphone => {
                    self.f1 = Biquad::highpass(sr, 600.0, 0.9);
                    self.f2 = Biquad::lowpass(sr, 4000.0, 0.9);
                }
                Voice::Underwater => {
                    self.f1 = Biquad::lowpass(sr, 700.0, 0.8);
                    self.f2 = Biquad::lowpass(sr, 900.0, 0.7);
                }
                Voice::Demon => {
                    self.f1 = Biquad::lowpass(sr, 3000.0, 0.7);
                    self.f2 = Biquad::identity();
                }
                _ => {}
            }
        }
        let st = match kind {
            Voice::Male => PITCH_MALE,
            Voice::Female => PITCH_FEMALE,
            Voice::Helium => PITCH_HELIUM,
            Voice::Baby => PITCH_BABY,
            Voice::Custom => custom,
            Voice::Deep => DEEP_PITCH,
            Voice::Demon => DEMON_PITCH,
            _ => return,
        };
        self.pitch.set_semitones(st);
    }

    /// One processing step for the effects that change over time.
    fn step(&mut self) {
        match self.kind {
            Voice::Mutation | Voice::SlowMutation | Voice::FastMutation => {
                self.sweep += self.sweep_step;
                if self.sweep < MUTATION_MIN {
                    self.sweep = MUTATION_MIN;
                    self.sweep_step = self.sweep_step.abs();
                } else if self.sweep > MUTATION_MAX {
                    self.sweep = MUTATION_MAX;
                    self.sweep_step = -self.sweep_step.abs();
                }
                self.pitch.set_semitones(self.sweep);
            }
            Voice::Atari => {
                self.sweep += self.sweep_step;
                if self.sweep < ATARI_LOW {
                    self.sweep_step = 1.0;
                    self.atari_pitch = ATARI_UP_PITCH;
                } else if self.sweep > ATARI_HIGH {
                    self.sweep_step = -1.0;
                    self.atari_pitch = ATARI_DOWN_PITCH;
                }
                self.pitch.set_semitones(self.atari_pitch);
            }
            _ => {}
        }
    }

    pub fn process(&mut self, buf: &mut [f32], kind: Voice, custom: f32) {
        self.set_kind(kind, custom);
        let sr = self.sr;
        for x in buf.iter_mut() {
            self.step_count += 1;
            if self.step_count >= self.step_len {
                self.step_count = 0;
                self.step();
            }
            let v = *x;
            *x = match kind {
                Voice::None | Voice::Chainer => v,
                Voice::Male
                | Voice::Female
                | Voice::Helium
                | Voice::Baby
                | Voice::Custom
                | Voice::Mutation
                | Voice::SlowMutation
                | Voice::FastMutation
                | Voice::Atari
                | Voice::Deep => self.pitch.run(v),
                Voice::Robot => {
                    self.robot_phase += ROBOT_HZ / sr;
                    if self.robot_phase >= 1.0 {
                        self.robot_phase -= 1.0;
                    }
                    v * (2.0 * PI * self.robot_phase).sin()
                }
                Voice::Radio => v.max(0.0),
                Voice::Silence => 0.0,
                Voice::Clone => {
                    let h = v * CLONE_GAIN;
                    h + self.clone.tick(h)
                }
                Voice::Alien => {
                    let n = self.alien_a.len();
                    let i = self.alien_i;
                    let fade = self.alien_fade;
                    let g = if i < fade {
                        i as f32 / fade as f32
                    } else if i > n - fade {
                        (n - i) as f32 / fade as f32
                    } else {
                        1.0
                    };
                    self.alien_a[i] = v * g;
                    let y = self.alien_b[n - 1 - i];
                    self.alien_i += 1;
                    if self.alien_i >= n {
                        self.alien_i = 0;
                        std::mem::swap(&mut self.alien_a, &mut self.alien_b);
                    }
                    y
                }
                Voice::Demon => {
                    let p = self.pitch.run(v);
                    let d = (p * 2.5).tanh() * 0.6;
                    self.f1.run(d)
                }
                Voice::Telephone => {
                    let y = self.f2.run(self.f1.run(v));
                    (y * 1.8).clamp(-0.8, 0.8)
                }
                Voice::Megaphone => {
                    let y = self.f2.run(self.f1.run(v));
                    (y * 4.0).tanh() * 0.7
                }
                Voice::Underwater => {
                    self.lfo += 5.0 / sr;
                    if self.lfo > 1.0 {
                        self.lfo -= 1.0;
                    }
                    let wob = 1.0 + 0.35 * (2.0 * PI * self.lfo).sin();
                    let y = self.f2.run(self.f1.run(v));
                    y * wob
                }
            };
        }
    }
}

// ---------------------------------------------------------------- rooms

pub struct RoomProc {
    sr: f32,
    kind: Room,
    cave: Delay,
    hall: Delay,
    chorus: Delay,
    ghost: Vec<f32>,
    ghost_w: usize,
    ghost_r: usize,
    combs: Vec<(Delay, f32)>,
    allpass: Vec<Delay>,
}

impl RoomProc {
    pub fn new(sr: f32) -> Self {
        let combs = [1557usize, 1617, 1491, 1422]
            .iter()
            .map(|&l| (Delay::new(l * sr as usize / 44100), 0.0f32))
            .collect();
        let allpass = [556usize, 441].iter().map(|&l| Delay::new(l * sr as usize / 44100)).collect();
        let ghost_len = (GHOST_DELAY_MS / 1000.0 * sr) as usize;
        RoomProc {
            sr,
            kind: Room::None,
            cave: Delay::new((CAVE_DELAY_MS / 1000.0 * sr) as usize),
            hall: Delay::new((HALL_DELAY_MS / 1000.0 * sr) as usize),
            chorus: Delay::new(CHORUS_DELAY_SAMPLES),
            ghost: vec![0.0; ghost_len.max(1)],
            ghost_w: 0,
            ghost_r: ghost_len.saturating_sub(1),
            combs,
            allpass,
        }
    }

    pub fn process(&mut self, buf: &mut [f32], kind: Room, mix: f32) {
        if kind != self.kind {
            self.kind = kind;
            self.cave.clear();
            self.hall.clear();
            self.chorus.clear();
            self.ghost.iter_mut().for_each(|v| *v = 0.0);
            for (c, s) in self.combs.iter_mut() {
                c.clear();
                *s = 0.0;
            }
            self.allpass.iter_mut().for_each(|a| a.clear());
        }
        let _ = self.sr;
        for x in buf.iter_mut() {
            let v = *x;
            let wet = match kind {
                Room::None => 0.0,
                Room::Cave => self.cave.tick(v * CAVE_GAIN),
                Room::CityHall => self.hall.tick(v * HALL_GAIN),
                Room::Chorus => {
                    // (x + delayed) * 0.5, expressed as an added wet part
                    let d = self.chorus.tick(v);
                    (v + d) * 0.5 - v
                }
                Room::Ghost => {
                    let n = self.ghost.len();
                    let y = self.ghost[self.ghost_r];
                    self.ghost[self.ghost_w] = v * GHOST_GAIN;
                    self.ghost_w = (self.ghost_w + 1) % n;
                    self.ghost_r = if self.ghost_r == 0 { n - 1 } else { self.ghost_r - 1 };
                    y
                }
                Room::Reverb => {
                    let mut acc = 0.0;
                    for (c, state) in self.combs.iter_mut() {
                        let out = c.buf[c.pos];
                        *state = out * 0.8 + *state * 0.2;
                        let _ = c.tick(v * 0.3 + *state * 0.82);
                        acc += out;
                    }
                    let mut y = acc * 0.25;
                    for a in self.allpass.iter_mut() {
                        let b = a.buf[a.pos];
                        let _ = a.tick(y + b * 0.5);
                        y = b - y * 0.5;
                    }
                    y
                }
            };
            *x = v + wet * mix;
        }
    }
}

// ---------------------------------------------------------------- tone, gate, denoise

pub struct Tone {
    sr: f32,
    bass: Biquad,
    treble: Biquad,
    bass_db: f32,
    treble_db: f32,
}

impl Tone {
    pub fn new(sr: f32) -> Self {
        Tone { sr, bass: Biquad::identity(), treble: Biquad::identity(), bass_db: 0.0, treble_db: 0.0 }
    }
    pub fn process(&mut self, buf: &mut [f32], bass_db: f32, treble_db: f32) {
        if bass_db != self.bass_db {
            self.bass_db = bass_db;
            let c = Biquad::shelf(self.sr, BASS_HZ, bass_db, SHELF_SLOPE, false);
            self.bass.set_coeffs(&c);
        }
        if treble_db != self.treble_db {
            self.treble_db = treble_db;
            let c = Biquad::shelf(self.sr, TREBLE_HZ, treble_db, SHELF_SLOPE, true);
            self.treble.set_coeffs(&c);
        }
        if bass_db.abs() < 0.01 && treble_db.abs() < 0.01 {
            return;
        }
        for x in buf.iter_mut() {
            *x = self.treble.run(self.bass.run(*x));
        }
    }
}

/// Silence gate: if a step's loudest sample stays under the threshold for more than
/// ten steps in a row, the voice is muted until it gets louder again.
pub struct Gate {
    step_len: usize,
    count: usize,
    peak: f32,
    quiet_steps: u32,
    open: f32,
}

impl Gate {
    pub fn new(sr: f32) -> Self {
        Gate { step_len: ((STEP_MS / 1000.0) * sr) as usize, count: 0, peak: 0.0, quiet_steps: 0, open: 1.0 }
    }
    pub fn process(&mut self, buf: &mut [f32], threshold: f32) {
        if threshold <= 0.0 {
            self.open = 1.0;
            return;
        }
        for x in buf.iter_mut() {
            self.peak = self.peak.max(x.abs());
            self.count += 1;
            if self.count >= self.step_len {
                if self.peak < threshold {
                    self.quiet_steps += 1;
                } else {
                    self.quiet_steps = 0;
                }
                self.count = 0;
                self.peak = 0.0;
            }
            let target = if self.quiet_steps > GATE_HOLD_STEPS { 0.0 } else { 1.0 };
            // short ramp so the gate doesn't click
            self.open += (target - self.open) * 0.01;
            *x *= self.open;
        }
    }
}

/// Spectral noise remover: tracks the noise floor per frequency and turns it down.
pub struct Denoiser {
    n: usize,
    hop: usize,
    window: Vec<f32>,
    inbuf: Vec<f32>,
    outbuf: Vec<f32>,
    outq: std::collections::VecDeque<f32>,
    noise: Vec<f32>,
    gains: Vec<f32>,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    ifft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    frames: u32,
}

impl Denoiser {
    pub fn new(_sr: f32) -> Self {
        let n = 512;
        let mut planner = rustfft::FftPlanner::<f32>::new();
        let window = (0..n).map(|i| (PI * (i as f32 + 0.5) / n as f32).sin()).collect();
        Denoiser {
            n,
            hop: n / 2,
            window,
            inbuf: Vec::with_capacity(n),
            outbuf: vec![0.0; n],
            outq: std::iter::repeat(0.0).take(n / 2).collect(),
            noise: vec![1e-4; n / 2 + 1],
            gains: vec![1.0; n / 2 + 1],
            fft: planner.plan_fft_forward(n),
            ifft: planner.plan_fft_inverse(n),
            frames: 0,
        }
    }

    fn frame(&mut self, strength: f32) {
        use rustfft::num_complex::Complex;
        let n = self.n;
        let mut spec: Vec<Complex<f32>> =
            (0..n).map(|i| Complex::new(self.inbuf[i] * self.window[i], 0.0)).collect();
        self.fft.process(&mut spec);
        self.frames += 1;
        for k in 0..=n / 2 {
            let mag = spec[k].norm();
            let nz = &mut self.noise[k];
            if self.frames < 10 {
                *nz = nz.max(mag);
            } else if mag < *nz {
                *nz = *nz * 0.8 + mag * 0.2;
            } else {
                *nz *= 1.004;
            }
            let snr = mag / (*nz + 1e-9);
            let g = (1.0 - strength * 1.5 / (snr + 1e-6)).max(0.08);
            let gs = &mut self.gains[k];
            *gs = if g > *gs { g } else { *gs * 0.6 + g * 0.4 };
            let gv = *gs;
            spec[k] *= gv;
            if k > 0 && k < n / 2 {
                spec[n - k] = spec[k].conj();
            }
        }
        self.ifft.process(&mut spec);
        let scale = 1.0 / n as f32;
        for i in 0..n {
            self.outbuf[i] += spec[i].re * scale * self.window[i];
        }
        for i in 0..self.hop {
            self.outq.push_back(self.outbuf[i]);
        }
        self.outbuf.copy_within(self.hop.., 0);
        for v in self.outbuf[n - self.hop..].iter_mut() {
            *v = 0.0;
        }
        self.inbuf.drain(0..self.hop);
    }

    pub fn process(&mut self, buf: &mut [f32], strength: f32) {
        for x in buf.iter_mut() {
            self.inbuf.push(*x);
            if self.inbuf.len() == self.n {
                self.frame(strength);
            }
            *x = self.outq.pop_front().unwrap_or(0.0);
        }
    }
}

// ---------------------------------------------------------------- vocoder

pub struct Vocoder {
    sr: f32,
    mod_f: Vec<[Biquad; 2]>,
    car_f: Vec<[Biquad; 2]>,
    env: Vec<f32>,
    att: f32,
    rel: f32,
    phase: [f32; 3],
    noise: u32,
    file_pos: usize,
}

impl Vocoder {
    pub fn new(sr: f32) -> Self {
        let bands = 18;
        let (lo, hi) = (110.0f32, (7500.0f32).min(sr * 0.45));
        let mut mod_f = Vec::new();
        let mut car_f = Vec::new();
        for b in 0..bands {
            let f = lo * (hi / lo).powf(b as f32 / (bands - 1) as f32);
            let q = 5.0;
            let bp = Biquad::bandpass(sr, f, q);
            mod_f.push([bp, bp]);
            car_f.push([bp, bp]);
        }
        Vocoder {
            sr,
            mod_f,
            car_f,
            env: vec![0.0; bands],
            att: 1.0 - (-1.0 / (0.004 * sr)).exp(),
            rel: 1.0 - (-1.0 / (0.03 * sr)).exp(),
            phase: [0.0; 3],
            noise: 22222,
            file_pos: 0,
        }
    }

    fn carrier(&mut self, kind: CarrierKind, note: f32, file: Option<&[f32]>) -> f32 {
        let f0 = 440.0 * 2f32.powf((note - 69.0) / 12.0);
        let sr = self.sr;
        let saw = |i: usize, f: f32, ph: &mut [f32; 3]| {
            ph[i] += f / sr;
            if ph[i] >= 1.0 {
                ph[i] -= 1.0;
            }
            2.0 * ph[i] - 1.0
        };
        match kind {
            CarrierKind::Saw => saw(0, f0, &mut self.phase),
            CarrierKind::Chord => {
                (saw(0, f0, &mut self.phase) + saw(1, f0 * 1.2599, &mut self.phase) + saw(2, f0 * 1.4983, &mut self.phase))
                    * 0.45
            }
            CarrierKind::Buzz => {
                let s = saw(0, f0, &mut self.phase);
                if s > 0.6 { 1.0 } else { -0.25 }
            }
            CarrierKind::Whisper => {
                self.noise ^= self.noise << 13;
                self.noise ^= self.noise >> 17;
                self.noise ^= self.noise << 5;
                (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0
            }
            CarrierKind::File => match file {
                Some(f) if !f.is_empty() => {
                    let v = f[self.file_pos % f.len()];
                    self.file_pos = (self.file_pos + 1) % f.len();
                    v * 2.0
                }
                _ => saw(0, f0, &mut self.phase),
            },
        }
    }

    pub fn process(&mut self, buf: &mut [f32], kind: CarrierKind, note: f32, mix: f32, file: Option<&[f32]>) {
        for x in buf.iter_mut() {
            let m = *x;
            let c = self.carrier(kind, note, file);
            let mut out = 0.0;
            for b in 0..self.env.len() {
                let m0 = self.mod_f[b][0].run(m);
                let mb = self.mod_f[b][1].run(m0);
                let a = mb.abs();
                let e = &mut self.env[b];
                *e += (a - *e) * if a > *e { self.att } else { self.rel };
                let c0 = self.car_f[b][0].run(c);
                let cb = self.car_f[b][1].run(c0);
                out += cb * *e;
            }
            let wet = out * 9.0;
            *x = m * (1.0 - mix) + wet * mix;
        }
    }
}

// ---------------------------------------------------------------- whole chain

pub struct Engine {
    pub sr: f32,
    main: VoiceProc,
    chain: Vec<VoiceProc>,
    room: RoomProc,
    tone: Tone,
    gate: Gate,
    denoise: Denoiser,
    vocoder: Vocoder,
}

impl Engine {
    pub fn new(sr: f32) -> Self {
        Engine {
            sr,
            main: VoiceProc::new(sr),
            chain: (0..4).map(|_| VoiceProc::new(sr)).collect(),
            room: RoomProc::new(sr),
            tone: Tone::new(sr),
            gate: Gate::new(sr),
            denoise: Denoiser::new(sr),
            vocoder: Vocoder::new(sr),
        }
    }

    /// Process a mono block in place. `pre` (sounds/music that should get the voice
    /// effect) is added before the effects, `post` (the rest) after them.
    pub fn process(&mut self, buf: &mut [f32], pre: &[f32], post: &[f32], p: &Params, carrier_file: Option<&[f32]>) {
        for x in buf.iter_mut() {
            *x *= p.mic_gain;
        }
        if !p.enabled {
            for ((x, a), b) in buf.iter_mut().zip(pre).zip(post) {
                *x = (*x + a + b).clamp(-1.0, 1.0);
            }
            return;
        }
        if p.denoise {
            self.denoise.process(buf, p.denoise_strength);
        }
        self.gate.process(buf, p.gate as f32 / 100.0);
        for (x, s) in buf.iter_mut().zip(pre) {
            *x += s;
        }
        self.effects(buf, p, carrier_file);
        for (x, s) in buf.iter_mut().zip(post) {
            *x += s;
        }
        let gain = if p.boost > 0 { 1.0 + p.boost as f32 * BOOST_PER_STEP } else { 1.0 };
        for x in buf.iter_mut() {
            *x = (*x * gain).clamp(-1.0, 1.0);
        }
    }

    /// Only the voice effect, room, vocoder, tone and boost: no mic volume, noise
    /// removal or gate. Used for the copy of sounds/music you hear yourself when
    /// they go through your effect but your own voice isn't played back to you.
    pub fn process_effects_only(&mut self, buf: &mut [f32], p: &Params, carrier_file: Option<&[f32]>) {
        if !p.enabled {
            return;
        }
        self.effects(buf, p, carrier_file);
        let gain = if p.boost > 0 { 1.0 + p.boost as f32 * BOOST_PER_STEP } else { 1.0 };
        for x in buf.iter_mut() {
            *x = (*x * gain).clamp(-1.0, 1.0);
        }
    }

    fn effects(&mut self, buf: &mut [f32], p: &Params, carrier_file: Option<&[f32]>) {
        if p.voice == Voice::Chainer {
            for (i, proc_) in self.chain.iter_mut().enumerate() {
                let k = p.chain[i];
                if k != Voice::None && k != Voice::Chainer {
                    proc_.process(buf, k, p.custom_pitch);
                }
            }
        } else if p.voice != Voice::None {
            self.main.process(buf, p.voice, p.custom_pitch);
        }
        if p.room != Room::None {
            self.room.process(buf, p.room, p.room_mix);
        }
        if p.vocoder {
            self.vocoder.process(buf, p.carrier, p.carrier_note, p.vocoder_mix, carrier_file);
        }
        self.tone.process(buf, p.bass_db, p.treble_db);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_effects_run_and_stay_finite() {
        let sr = 48000.0;
        let mut e = Engine::new(sr);
        let mut p = Params::default();
        for v in Voice::ALL {
            for r in Room::ALL {
                p.voice = v;
                p.room = r;
                p.vocoder = matches!(v, Voice::Robot);
                p.denoise = matches!(v, Voice::Male);
                p.bass_db = 6.0;
                let mut buf: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
                e.process(&mut buf, &[], &[], &p, None);
                assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{:?} {:?}", v, r);
            }
        }
    }
}
