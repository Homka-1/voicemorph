//! Offline text to speech with the voices built into Windows.
//! The text is spoken into a temporary WAV file, which is then played into the mic.

use std::path::PathBuf;
use std::process::Command;

#[cfg(windows)]
fn powershell() -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-Command"]);
    c.creation_flags(0x0800_0000); // no console window
    c
}

#[cfg(not(windows))]
fn powershell() -> Command {
    let mut c = Command::new("pwsh");
    c.args(["-NoProfile", "-NonInteractive", "-Command"]);
    c
}

pub fn list_voices() -> Vec<String> {
    let script = "Add-Type -AssemblyName System.Speech; \
        (New-Object System.Speech.Synthesis.SpeechSynthesizer).GetInstalledVoices() | \
        ForEach-Object { $_.VoiceInfo.Name }";
    match powershell().arg(script).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Speak `text` into a WAV file and return its path.
pub fn speak_to_file(text: &str, voice: &str, rate: i32) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("voicemorph-tts-{}.wav", std::process::id()));
    let script = "Add-Type -AssemblyName System.Speech; \
        $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
        if ($env:VM_TTS_VOICE) { try { $s.SelectVoice($env:VM_TTS_VOICE) } catch {} }; \
        $s.Rate = [int]$env:VM_TTS_RATE; \
        $s.SetOutputToWaveFile($env:VM_TTS_OUT); \
        $s.Speak($env:VM_TTS_TEXT); $s.Dispose()";
    let out = powershell()
        .arg(script)
        .env("VM_TTS_TEXT", text)
        .env("VM_TTS_VOICE", voice)
        .env("VM_TTS_RATE", rate.clamp(-10, 10).to_string())
        .env("VM_TTS_OUT", &path)
        .output()
        .map_err(|e| format!("Windows speech isn't available: {e}"))?;
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("Speech failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}
