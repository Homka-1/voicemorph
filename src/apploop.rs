//! "App sound into my microphone": record the sound one app plays (with its child
//! processes, so browsers work) and hand it to the mixer.
//!
//! Uses Windows' per-app loopback (Windows 10 2004 or later): the virtual device
//! "VAD\Process_Loopback" activated with the app's process id. The app keeps playing
//! on your speakers as usual; we get a copy.
#![allow(non_snake_case)]

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One app that can be picked.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioApp {
    /// The top process of the app ("firefox.exe" with all its helpers).
    pub pid: u32,
    /// Program file name, e.g. "firefox.exe". This is what the settings remember.
    pub exe: String,
    /// Shown name ("Firefox").
    pub name: String,
    /// It has an audio session (it played sound since it started).
    pub has_audio: bool,
}

pub fn pretty_name(exe: &str) -> String {
    let stem = exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(exe);
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Every running process: pid → (parent pid, exe name).
fn processes() -> HashMap<u32, (u32, String)> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
    let mut map = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap as isize == -1 {
            return map;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
                map.insert(e.th32ProcessID, (e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..len])));
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        windows_sys::Win32::Foundation::CloseHandle(snap);
    }
    map
}

/// Walk up while the parent is the same program (browser helpers → the browser).
fn root_of(pid: u32, procs: &HashMap<u32, (u32, String)>) -> u32 {
    let mut cur = pid;
    for _ in 0..16 {
        let Some((parent, exe)) = procs.get(&cur) else { break };
        match procs.get(parent) {
            Some((_, pexe)) if pexe.eq_ignore_ascii_case(exe) && *parent != cur => cur = *parent,
            _ => break,
        }
    }
    cur
}

/// Process ids that have an audio session on any speaker.
fn session_pids() -> Vec<u32> {
    run_mta(|| unsafe {
        use windows::Win32::Media::Audio::*;
        use windows::Win32::System::Com::*;
        use windows_core::Interface;
        let mut out = Vec::new();
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return out };
        let Ok(devs) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return out };
        let n = devs.GetCount().unwrap_or(0);
        for i in 0..n {
            let Ok(d) = devs.Item(i) else { continue };
            let Ok(mgr) = d.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(se) = mgr.GetSessionEnumerator() else { continue };
            for k in 0..se.GetCount().unwrap_or(0) {
                let Ok(s) = se.GetSession(k) else { continue };
                if let Ok(s2) = s.cast::<IAudioSessionControl2>() {
                    if let Ok(pid) = s2.GetProcessId() {
                        if pid != 0 {
                            out.push(pid);
                        }
                    }
                }
            }
        }
        out
    })
}

/// Programs that aren't apps anyone wants to share.
const HIDDEN: [&str; 28] = [
    "applicationframehost.exe", "explorer.exe", "textinputhost.exe", "shellexperiencehost.exe",
    "system", "[system process]", "registry", "smss.exe", "csrss.exe", "wininit.exe", "services.exe", "lsass.exe", "svchost.exe", "fontdrvhost.exe",
    "winlogon.exe", "dwm.exe", "conhost.exe", "sihost.exe", "taskhostw.exe", "runtimebroker.exe", "audiodg.exe", "searchindexer.exe", "ctfmon.exe",
    "dllhost.exe", "wmiprvse.exe", "memory compression", "securityhealthservice.exe", "spoolsv.exe",
];

/// Process ids that own a visible, titled window (what people think of as "open apps").
fn window_pids() -> Vec<u32> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
        let out = &mut *(l as *mut Vec<u32>);
        if IsWindowVisible(h) != 0 && GetWindowTextLengthW(h) > 0 && GetWindow(h, GW_OWNER).is_null() {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if pid != 0 && !out.contains(&pid) {
                out.push(pid);
            }
        }
        1
    }
    let mut out: Vec<u32> = Vec::new();
    unsafe {
        EnumWindows(Some(each), &mut out as *mut Vec<u32> as LPARAM);
    }
    out
}

/// Apps that play sound first, then every other app with a window (one entry per program).
pub fn list_apps() -> Vec<AudioApp> {
    let procs = processes();
    let me = std::process::id();
    let audio: Vec<u32> = session_pids();
    let windows = window_pids();
    let mut out: Vec<AudioApp> = Vec::new();
    let push = |pid: u32, has_audio: bool, out: &mut Vec<AudioApp>| {
        let root = root_of(pid, &procs);
        let Some((_, exe)) = procs.get(&root) else { return };
        if root == me || HIDDEN.contains(&exe.to_ascii_lowercase().as_str()) || exe.eq_ignore_ascii_case("voicemorph.exe") {
            return;
        }
        if let Some(a) = out.iter_mut().find(|a| a.exe.eq_ignore_ascii_case(exe)) {
            if has_audio && !a.has_audio {
                a.pid = root;
            }
            a.has_audio |= has_audio;
            return;
        }
        out.push(AudioApp { pid: root, exe: exe.clone(), name: pretty_name(exe), has_audio });
    };
    for pid in &audio {
        push(*pid, true, &mut out);
    }
    let mut rest: Vec<u32> = windows.into_iter().filter(|p| !audio.contains(p) && *p > 4).collect();
    rest.sort();
    for pid in rest {
        push(pid, false, &mut out);
    }
    out.sort_by(|a, b| b.has_audio.cmp(&a.has_audio).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Find the running app with this program name (its top process).
pub fn find_pid(exe: &str) -> Option<u32> {
    let procs = processes();
    let audio = session_pids();
    // prefer a process of that app that actually has sound
    if let Some(p) = audio.iter().find(|p| procs.get(p).map(|(_, e)| e.eq_ignore_ascii_case(exe)).unwrap_or(false)) {
        return Some(root_of(*p, &procs));
    }
    let mut pids: Vec<u32> = procs.iter().filter(|(_, (_, e))| e.eq_ignore_ascii_case(exe)).map(|(p, _)| *p).collect();
    pids.sort();
    pids.first().map(|p| root_of(*p, &procs))
}

fn run_mta<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        f()
    })
    .join()
    .expect("COM thread")
}

// ---------------------------------------------------------------- capture

#[windows::core::implement(windows::Win32::Media::Audio::IActivateAudioInterfaceCompletionHandler)]
struct Done {
    tx: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl windows::Win32::Media::Audio::IActivateAudioInterfaceCompletionHandler_Impl for Done_Impl {
    fn ActivateCompleted(&self, _op: Option<&windows::Win32::Media::Audio::IActivateAudioInterfaceAsyncOperation>) -> windows_core::Result<()> {
        if let Ok(mut t) = self.tx.lock() {
            if let Some(tx) = t.take() {
                let _ = tx.send(());
            }
        }
        Ok(())
    }
}

/// PROPVARIANT holding a blob (VT_BLOB), laid out as Windows expects.
#[repr(C)]
struct BlobVariant {
    vt: u16,
    r1: u16,
    r2: u16,
    r3: u16,
    size: u32,
    data: *const u8,
}

/// A running capture of one app. Dropping it stops the capture.
pub struct AppCapture {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for AppCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// How much app sound may wait in the queue before the oldest is dropped (keeps it in sync).
pub const APP_QUEUE_MAX_MS: f32 = 120.0;

impl AppCapture {
    /// Start recording `pid` (and its child processes) as mono at `rate` into `ring`.
    pub fn start(pid: u32, rate: u32, ring: Arc<Mutex<VecDeque<f32>>>) -> Result<AppCapture, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let st = stop.clone();
        let thread = std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            }
            match unsafe { open_client(pid, rate) } {
                Ok((client, cap)) => {
                    let _ = ready_tx.send(Ok(()));
                    unsafe { pump(&client, &cap, rate, &ring, &st) };
                    unsafe {
                        let _ = client.Stop();
                    }
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        });
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(AppCapture { stop, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err("Windows didn't answer in time".into())
            }
        }
    }
}

unsafe fn open_client(pid: u32, rate: u32) -> Result<(windows::Win32::Media::Audio::IAudioClient, windows::Win32::Media::Audio::IAudioCaptureClient), String> {
    use windows::Win32::Media::Audio::*;
    use windows_core::Interface;
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS { TargetProcessId: pid, ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE },
        },
    };
    let blob = BlobVariant {
        vt: 65, // VT_BLOB
        r1: 0,
        r2: 0,
        r3: 0,
        size: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        data: &params as *const _ as *const u8,
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = Done { tx: Mutex::new(Some(tx)) }.into();
    let op = ActivateAudioInterfaceAsync(
        VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
        &IAudioClient::IID,
        Some(&blob as *const BlobVariant as *const windows_core::PROPVARIANT),
        &handler,
    )
    .map_err(|e| format!("this version of Windows can't record one app ({e})"))?;
    rx.recv_timeout(Duration::from_secs(4)).map_err(|_| "Windows didn't open the app's sound".to_string())?;
    let mut hr = windows_core::HRESULT(0);
    let mut unk: Option<windows_core::IUnknown> = None;
    op.GetActivateResult(&mut hr, &mut unk).map_err(|e| e.to_string())?;
    hr.ok().map_err(|e| format!("couldn't open the app's sound ({e})"))?;
    let client: IAudioClient = unk.ok_or("no audio client")?.cast().map_err(|e| e.to_string())?;
    let fmt = WAVEFORMATEX {
        wFormatTag: 3, // IEEE float
        nChannels: 2,
        nSamplesPerSec: rate,
        nAvgBytesPerSec: rate * 8,
        nBlockAlign: 8,
        wBitsPerSample: 32,
        cbSize: 0,
    };
    client
        .Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | 0x0800_0000, 2_000_000, 0, &fmt, None)
        .map_err(|e| format!("couldn't start recording the app ({e})"))?;
    let cap: IAudioCaptureClient = client.GetService().map_err(|e| e.to_string())?;
    client.Start().map_err(|e| e.to_string())?;
    Ok((client, cap))
}

unsafe fn pump(
    _client: &windows::Win32::Media::Audio::IAudioClient,
    cap: &windows::Win32::Media::Audio::IAudioCaptureClient,
    rate: u32,
    ring: &Arc<Mutex<VecDeque<f32>>>,
    stop: &AtomicBool,
) {
    let max = (rate as f32 * APP_QUEUE_MAX_MS / 1000.0) as usize;
    let mut block: Vec<f32> = Vec::with_capacity(4096);
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(5));
        loop {
            let Ok(n) = cap.GetNextPacketSize() else { return };
            if n == 0 {
                break;
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            if cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() {
                return;
            }
            block.clear();
            let silent = flags & 0x2 != 0 || data.is_null(); // AUDCLNT_BUFFERFLAGS_SILENT
            if silent {
                block.resize(frames as usize, 0.0);
            } else {
                let s = std::slice::from_raw_parts(data as *const f32, frames as usize * 2);
                block.extend(s.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5));
            }
            let _ = cap.ReleaseBuffer(frames);
            if let Ok(mut q) = ring.lock() {
                q.extend(block.iter().copied());
                if q.len() > max {
                    let d = q.len() - max;
                    q.drain(0..d);
                }
            }
        }
    }
}
