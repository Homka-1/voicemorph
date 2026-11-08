//! Notification-area (tray) icon. The original has one too (it imports
//! Shell_NotifyIconW). Closing the window hides it here; click the icon to bring it
//! back, right-click for a small menu. The icon runs on its own thread with its own
//! message loop, so it keeps working while the main window is hidden.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub enum TrayCmd {
    Toggle,
    StopAll,
}

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static QUIT: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static HIDDEN: AtomicBool = AtomicBool::new(false);
/// Voice changing on/off, shown as a tick in the menu.
pub static VOICE_ON: AtomicBool = AtomicBool::new(true);
static SENDER: OnceLock<Sender<TrayCmd>> = OnceLock::new();
static CTX: OnceLock<egui::Context> = OnceLock::new();

const WM_TRAY: u32 = 0x8000 + 41; // WM_APP + 41
const ID_SHOW: usize = 1;
const ID_TOGGLE: usize = 2;
const ID_STOP: usize = 3;
const ID_QUIT: usize = 4;

/// The app's round icon (purple to cyan, white microphone), RGBA.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    let smooth = |d: f32| (0.5 - d * s).clamp(0.0, 1.0); // d in units of the icon, <0 inside
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / s;
            let v = (y as f32 + 0.5) / s;
            let (dx, dy) = (u - 0.5, v - 0.5);
            let disc = (dx * dx + dy * dy).sqrt() - 0.48;
            let a_disc = smooth(disc);
            // gradient purple (0x7c5cff) -> cyan (0x22d3ee) along the diagonal
            let t = ((u + v) / 2.0).clamp(0.0, 1.0);
            let (r0, g0, b0) = (124.0, 92.0, 255.0);
            let (r1, g1, b1) = (34.0, 211.0, 238.0);
            let mut r = r0 + (r1 - r0) * t * 0.7;
            let mut g = g0 + (g1 - g0) * t * 0.7;
            let mut b = b0 + (b1 - b0) * t * 0.7;
            // microphone: capsule, cradle arc, stem and foot
            let cap = {
                let cy = dy.clamp(-0.20, -0.04);
                ((dx * dx + (dy - cy) * (dy - cy)).sqrt()) - 0.095
            };
            let arc = {
                let cy = -0.06;
                let d = (dx * dx + (dy - cy) * (dy - cy)).sqrt();
                let ring = (d - 0.17).abs() - 0.028;
                if dy > cy { ring } else { 1.0 }
            };
            let stem = if dy > 0.1 && dy < 0.24 { dx.abs() - 0.026 } else { 1.0 };
            let foot = if (dy - 0.24).abs() < 0.026 { dx.abs() - 0.1 } else { 1.0 };
            let m = cap.min(arc).min(stem).min(foot);
            let a_mic = smooth(m);
            r += (255.0 - r) * a_mic;
            g += (255.0 - g) * a_mic;
            b += (255.0 - b) * a_mic;
            let i = ((y * size + x) * 4) as usize;
            px[i] = r as u8;
            px[i + 1] = g as u8;
            px[i + 2] = b as u8;
            px[i + 3] = (a_disc * 255.0) as u8;
        }
    }
    px
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_wide(dst: &mut [u16], s: &str) {
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().chain(std::iter::once(0))) {
        *d = c;
    }
    if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

pub fn set_main_window(hwnd: isize) {
    MAIN_HWND.store(hwnd, Ordering::Relaxed);
}

pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

pub fn quitting() -> bool {
    QUIT.load(Ordering::Relaxed)
}

pub fn hidden() -> bool {
    HIDDEN.load(Ordering::Relaxed)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    static mut TASKBAR_CREATED: u32 = 0;
    const WM_SHOW_REQUEST: u32 = WM_APP + 2;

    /// If another copy is already running, ask it to show its window and return true.
    pub fn signal_existing() -> bool {
        let class = w("VoiceMorphTray");
        unsafe {
            let h = FindWindowW(class.as_ptr(), std::ptr::null());
            if h.is_null() {
                return false;
            }
            PostMessageW(h, WM_SHOW_REQUEST, 0, 0);
        }
        true
    }

    fn make_icon() -> HICON {
        let size = 32u32;
        let rgba = icon_rgba(size);
        let mut bgra = Vec::with_capacity(rgba.len());
        for p in rgba.chunks(4) {
            bgra.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        let and_mask = vec![0u8; (size * size / 8) as usize];
        unsafe { CreateIcon(GetModuleHandleW(std::ptr::null()), size as i32, size as i32, 1, 32, and_mask.as_ptr(), bgra.as_ptr()) }
    }

    fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
        let mut n: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        n.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        n.hWnd = hwnd;
        n.uID = 1;
        n
    }

    fn add_icon(hwnd: HWND, icon: HICON) {
        let mut n = nid(hwnd);
        n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        n.uCallbackMessage = WM_TRAY;
        n.hIcon = icon;
        copy_wide(&mut n.szTip, "VoiceMorph – voice changer");
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &n);
        }
    }

    pub fn show_main() {
        let h = MAIN_HWND.load(Ordering::Relaxed) as HWND;
        if h.is_null() {
            return;
        }
        unsafe {
            if IsIconic(h) != 0 {
                ShowWindow(h, SW_RESTORE);
            } else {
                ShowWindow(h, SW_SHOW);
            }
            SetForegroundWindow(h);
        }
        HIDDEN.store(false, Ordering::Relaxed);
        if let Some(c) = CTX.get() {
            c.request_repaint();
        }
    }

    pub fn hide_main() {
        let h = MAIN_HWND.load(Ordering::Relaxed) as HWND;
        if !h.is_null() {
            unsafe {
                ShowWindow(h, SW_HIDE);
            }
            HIDDEN.store(true, Ordering::Relaxed);
        }
    }

    pub fn quit() {
        QUIT.store(true, Ordering::Relaxed);
        show_main();
        let h = MAIN_HWND.load(Ordering::Relaxed) as HWND;
        if !h.is_null() {
            unsafe {
                PostMessageW(h, WM_CLOSE, 0, 0);
            }
        }
    }

    fn menu(hwnd: HWND) {
        unsafe {
            let m = CreatePopupMenu();
            let show = w(if hidden() { "Show VoiceMorph" } else { "Bring VoiceMorph to front" });
            let toggle = w("Voice changing");
            let stop = w("Stop sounds and music");
            let quit_t = w("Quit");
            AppendMenuW(m, MF_STRING | MF_DEFAULT, ID_SHOW, show.as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
            let checked = if VOICE_ON.load(Ordering::Relaxed) { MF_CHECKED } else { MF_UNCHECKED };
            AppendMenuW(m, MF_STRING | checked, ID_TOGGLE, toggle.as_ptr());
            AppendMenuW(m, MF_STRING, ID_STOP, stop.as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(m, MF_STRING, ID_QUIT, quit_t.as_ptr());
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, std::ptr::null());
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(m);
            match cmd as usize {
                ID_SHOW => show_main(),
                ID_TOGGLE => send(TrayCmd::Toggle),
                ID_STOP => send(TrayCmd::StopAll),
                ID_QUIT => quit(),
                _ => {}
            }
        }
    }

    fn send(c: TrayCmd) {
        if let Some(s) = SENDER.get() {
            let _ = s.send(c);
        }
    }

    static mut ICON: HICON = std::ptr::null_mut();

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_TRAY {
            match (lp as u32) & 0xFFFF {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => show_main(),
                WM_RBUTTONUP | WM_CONTEXTMENU => menu(hwnd),
                _ => {}
            }
            return 0;
        }
        if msg == WM_SHOW_REQUEST {
            // a second copy of the app was started: bring this one back instead
            show_main();
            return 0;
        }
        if msg != 0 && msg == TASKBAR_CREATED {
            // Explorer restarted: put the icon back
            add_icon(hwnd, ICON);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }

    pub fn start(tx: Sender<TrayCmd>, ctx: egui::Context) {
        let _ = SENDER.set(tx);
        let _ = CTX.set(ctx);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || unsafe {
            let inst = GetModuleHandleW(std::ptr::null());
            let class = w("VoiceMorphTray");
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(wndproc);
            wc.hInstance = inst;
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let title = w("VoiceMorph tray");
            let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), 0, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), inst, std::ptr::null::<c_void>());
            if hwnd.is_null() {
                let _ = ready_tx.send(false);
                return;
            }
            let tc = w("TaskbarCreated");
            TASKBAR_CREATED = RegisterWindowMessageW(tc.as_ptr());
            ICON = make_icon();
            add_icon(hwnd, ICON);
            TRAY_HWND.store(hwnd as isize, Ordering::Relaxed);
            ACTIVE.store(true, Ordering::Relaxed);
            let _ = ready_tx.send(true);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        let _ = ready_rx.recv_timeout(std::time::Duration::from_secs(2));
    }

    /// A small pop-up message next to the tray icon.
    pub fn balloon(title: &str, text: &str) {
        let h = TRAY_HWND.load(Ordering::Relaxed) as HWND;
        if h.is_null() {
            return;
        }
        let mut n = nid(h);
        n.uFlags = NIF_INFO;
        copy_wide(&mut n.szInfoTitle, title);
        copy_wide(&mut n.szInfo, text);
        n.dwInfoFlags = NIIF_INFO;
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &n);
        }
    }

    pub fn remove() {
        let h = TRAY_HWND.swap(0, Ordering::Relaxed) as HWND;
        if h.is_null() {
            return;
        }
        let n = nid(h);
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &n);
            PostMessageW(h, WM_QUIT, 0, 0);
        }
        ACTIVE.store(false, Ordering::Relaxed);
    }
}

#[cfg(windows)]
pub use imp::{balloon, hide_main, quit, remove, signal_existing, start};
#[cfg(not(windows))]
pub fn signal_existing() -> bool {
    false
}

#[cfg(not(windows))]
pub fn start(_tx: Sender<TrayCmd>, _ctx: egui::Context) {}
#[cfg(not(windows))]
pub fn balloon(_t: &str, _x: &str) {}
#[cfg(not(windows))]
pub fn hide_main() {}
#[cfg(not(windows))]
pub fn quit() {}
#[cfg(not(windows))]
pub fn remove() {}
