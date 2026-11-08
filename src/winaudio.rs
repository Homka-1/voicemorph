//! Windows plumbing for the virtual microphone:
//!  * list microphones (with their endpoint ids) and find a virtual cable,
//!  * switch Windows' default microphone (and switch it back),
//!  * install / remove the microphone add-on (needs admin, so the app re-runs
//!    itself elevated with --apo-install / --apo-uninstall).
//!
//! The add-on is hooked in the way the original does it (MECHANICS, exe 0x40dc50):
//! appended to the endpoint's composite endpoint-effect list in its FxProperties,
//! with the old values backed up first and restored on removal.
#![allow(non_snake_case)]

const APO_CLSID_STR: &str = "{7B3F5C2E-9A41-4D8E-B6F0-3C5A2E9D71A4}";
use std::ffi::c_void;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::*;

pub const CAPTURE_ROOT: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Capture";
pub const BACKUP_ROOT: &str = r"SOFTWARE\VoiceMorph\Backup";
/// Composite endpoint-effect list (REG_MULTI_SZ), the slot the original uses.
pub const COMPOSITE_EFX: &str = "{d3993a3f-99c2-4402-b5ec-a92a0367664b},7";


/// Which node the effects attach to (KSNODETYPE_ANY when set to zeros).
pub const FX_ASSOC: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},0";
/// "Disable all enhancements" switch in the endpoint's Properties.
pub const DISABLE_SYSFX: &str = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5";
const NAME_KEY: &str = "{a45c254e-df1c-4efd-8020-67d146a850e0},2";
const DESC_KEY: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},6";


const MISSING: &str = "!KEY";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------- registry

fn get_raw(sub: &str, name: &str) -> Option<(u32, Vec<u8>)> {
    unsafe {
        let (sub_w, name_w) = (w(sub), w(name));
        let mut ty = 0u32;
        let mut size = 0u32;
        let flags = RRF_RT_ANY | RRF_SUBKEY_WOW6464KEY;
        if RegGetValueW(HKEY_LOCAL_MACHINE, sub_w.as_ptr(), name_w.as_ptr(), flags, &mut ty, std::ptr::null_mut(), &mut size) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u8; size as usize + 4];
        let mut size2 = buf.len() as u32;
        if RegGetValueW(HKEY_LOCAL_MACHINE, sub_w.as_ptr(), name_w.as_ptr(), flags, &mut ty, buf.as_mut_ptr() as *mut c_void, &mut size2) != ERROR_SUCCESS {
            return None;
        }
        buf.truncate(size2 as usize);
        Some((ty, buf))
    }
}

fn utf16_strings(bytes: &[u8]) -> Vec<String> {
    let u: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    u.split(|c| *c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
}

pub fn get_strings(sub: &str, name: &str) -> Option<Vec<String>> {
    let (ty, b) = get_raw(sub, name)?;
    if ty == REG_SZ || ty == REG_MULTI_SZ || ty == REG_EXPAND_SZ {
        Some(utf16_strings(&b))
    } else {
        None
    }
}

pub fn get_dword(sub: &str, name: &str) -> Option<u32> {
    let (ty, b) = get_raw(sub, name)?;
    (ty == REG_DWORD && b.len() >= 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn subkeys(sub: &str) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut h: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, w(sub).as_ptr(), 0, KEY_READ | KEY_WOW64_64KEY, &mut h) != ERROR_SUCCESS {
            return out;
        }
        let mut i = 0;
        loop {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            let r = RegEnumKeyExW(h, i, name.as_mut_ptr(), &mut len, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            if r != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&name[..len as usize]));
            i += 1;
        }
        RegCloseKey(h);
    }
    out
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

/// Turn on the admin privileges that let us edit protected keys
/// (the original enables SeTakeOwnershipPrivilege the same way).
fn enable_privileges() {
    use windows_sys::Win32::Security::*;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut tok = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut tok) == 0 {
            return;
        }
        for name in ["SeTakeOwnershipPrivilege", "SeRestorePrivilege", "SeBackupPrivilege"] {
            let mut tp: TOKEN_PRIVILEGES = std::mem::zeroed();
            if LookupPrivilegeValueW(std::ptr::null(), w(name).as_ptr(), &mut tp.Privileges[0].Luid) != 0 {
                tp.PrivilegeCount = 1;
                tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
                AdjustTokenPrivileges(tok, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut());
            }
        }
        windows_sys::Win32::Foundation::CloseHandle(tok);
    }
}

/// Microphone keys belong to the system; even admins may only read them.
/// Like the original: become the key's owner, then give Administrators full access.
fn make_writable(sub: &str) -> Result<(), String> {
    use windows_sys::Win32::Security::Authorization::*;
    use windows_sys::Win32::Security::*;
    enable_privileges();
    unsafe {
        let mut sid = [0u8; 68];
        let mut sid_len = sid.len() as u32;
        if CreateWellKnownSid(WinBuiltinAdministratorsSid, std::ptr::null_mut(), sid.as_mut_ptr() as *mut c_void, &mut sid_len) == 0 {
            return Err("can't look up the Administrators group".into());
        }
        let psid = sid.as_mut_ptr() as *mut c_void;
        let mut name = w(&format!("MACHINE\\{sub}"));
        let r = SetNamedSecurityInfoW(name.as_mut_ptr(), SE_REGISTRY_KEY, OWNER_SECURITY_INFORMATION, psid, std::ptr::null_mut(), std::ptr::null(), std::ptr::null());
        if r != 0 {
            return Err(format!("can't take ownership of HKLM\\{sub} (error {r})"));
        }
        let mut old_dacl: *mut ACL = std::ptr::null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let r = GetNamedSecurityInfoW(name.as_ptr(), SE_REGISTRY_KEY, DACL_SECURITY_INFORMATION, std::ptr::null_mut(), std::ptr::null_mut(), &mut old_dacl, std::ptr::null_mut(), &mut sd);
        if r != 0 {
            return Err(format!("can't read permissions of HKLM\\{sub} (error {r})"));
        }
        let ea = EXPLICIT_ACCESS_W {
            grfAccessPermissions: KEY_ALL_ACCESS,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_GROUP,
                ptstrName: psid as *mut u16,
            },
        };
        let mut new_dacl: *mut ACL = std::ptr::null_mut();
        let r = SetEntriesInAclW(1, &ea, old_dacl, &mut new_dacl);
        windows_sys::Win32::Foundation::LocalFree(sd as _);
        if r != 0 {
            return Err(format!("can't add write permission to HKLM\\{sub} (error {r})"));
        }
        let r = SetNamedSecurityInfoW(name.as_mut_ptr(), SE_REGISTRY_KEY, DACL_SECURITY_INFORMATION, std::ptr::null_mut(), std::ptr::null_mut(), new_dacl, std::ptr::null());
        windows_sys::Win32::Foundation::LocalFree(new_dacl as _);
        if r != 0 {
            return Err(format!("can't change permissions of HKLM\\{sub} (error {r})"));
        }
    }
    Ok(())
}

/// Open (or create) a key for writing. When Windows refuses (error 5, the
/// microphone keys), first try the backup/restore privilege, then take
/// ownership of the key (or its parent, for a key that doesn't exist yet).
fn open_write(sub: &str, create: bool) -> Result<Key, String> {
    match open_write_once(sub, create) {
        Err((5, _)) => {}
        r => return r.map_err(|e| e.1),
    }
    enable_privileges();
    unsafe {
        let mut h: HKEY = std::ptr::null_mut();
        let r = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            w(sub).as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_BACKUP_RESTORE,
            KEY_ALL_ACCESS | KEY_WOW64_64KEY,
            std::ptr::null(),
            &mut h,
            std::ptr::null_mut(),
        );
        if r == ERROR_SUCCESS {
            return Ok(Key(h));
        }
    }
    let target = if key_exists(sub) { sub.to_string() } else { sub.rsplit_once('\\').map(|p| p.0.to_string()).unwrap_or_default() };
    make_writable(&target)?;
    open_write_once(sub, create).map_err(|e| e.1)
}

fn key_exists(sub: &str) -> bool {
    unsafe {
        let mut h: HKEY = std::ptr::null_mut();
        let ok = RegOpenKeyExW(HKEY_LOCAL_MACHINE, w(sub).as_ptr(), 0, KEY_READ | KEY_WOW64_64KEY, &mut h) == ERROR_SUCCESS;
        if ok {
            RegCloseKey(h);
        }
        ok
    }
}

fn open_write_once(sub: &str, create: bool) -> Result<Key, (u32, String)> {
    unsafe {
        let mut h: HKEY = std::ptr::null_mut();
        let r = if create {
            RegCreateKeyExW(
                HKEY_LOCAL_MACHINE,
                w(sub).as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_QUERY_VALUE | KEY_CREATE_SUB_KEY | KEY_WOW64_64KEY,
                std::ptr::null(),
                &mut h,
                std::ptr::null_mut(),
            )
        } else {
            RegOpenKeyExW(HKEY_LOCAL_MACHINE, w(sub).as_ptr(), 0, KEY_SET_VALUE | KEY_QUERY_VALUE | KEY_WOW64_64KEY, &mut h)
        };
        if r != ERROR_SUCCESS {
            return Err((r, format!("can't write HKLM\\{sub} (error {r})")));
        }
        Ok(Key(h))
    }
}

#[allow(dead_code)]
impl Key {
    fn set(&self, name: &str, ty: u32, data: &[u8]) -> Result<(), String> {
        let r = unsafe { RegSetValueExW(self.0, w(name).as_ptr(), 0, ty, data.as_ptr(), data.len() as u32) };
        if r != ERROR_SUCCESS {
            return Err(format!("can't set {name} (error {r})"));
        }
        Ok(())
    }
    fn set_sz(&self, name: &str, v: &str) -> Result<(), String> {
        let b: Vec<u8> = w(v).iter().flat_map(|c| c.to_le_bytes()).collect();
        self.set(name, REG_SZ, &b)
    }
    fn set_multi(&self, name: &str, v: &[String]) -> Result<(), String> {
        let mut u: Vec<u16> = Vec::new();
        for s in v {
            u.extend(s.encode_utf16());
            u.push(0);
        }
        u.push(0);
        let b: Vec<u8> = u.iter().flat_map(|c| c.to_le_bytes()).collect();
        self.set(name, REG_MULTI_SZ, &b)
    }
    fn set_dword(&self, name: &str, v: u32) -> Result<(), String> {
        self.set(name, REG_DWORD, &v.to_le_bytes())
    }
    fn delete(&self, name: &str) {
        unsafe {
            RegDeleteValueW(self.0, w(name).as_ptr());
        }
    }
}

fn delete_tree(sub: &str) {
    unsafe {
        let mut h: HKEY = std::ptr::null_mut();
        let parent = sub.rsplit_once('\\').map(|p| p.0).unwrap_or("");
        let leaf = sub.rsplit_once('\\').map(|p| p.1).unwrap_or(sub);
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, w(parent).as_ptr(), 0, KEY_ALL_ACCESS | KEY_WOW64_64KEY, &mut h) == ERROR_SUCCESS {
            RegDeleteTreeW(h, w(leaf).as_ptr());
            RegDeleteKeyExW(h, w(leaf).as_ptr(), KEY_WOW64_64KEY, 0);
            RegCloseKey(h);
        }
    }
}

// ---------------------------------------------------------------- microphones

#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    /// "{guid}" under MMDevices\Audio\Capture
    pub guid: String,
    /// "{0.0.1.00000000}.{guid}", the id Windows uses for this microphone
    pub id: String,
    /// "Microphone (USB headset)": same as the device lists show
    pub name: String,
    pub hooked: bool,
    pub enhancements_off: bool,
}

pub fn capture_endpoints() -> Vec<Endpoint> {
    let clsid = APO_CLSID_STR.to_ascii_lowercase();
    let mut v: Vec<Endpoint> = subkeys(CAPTURE_ROOT)
        .into_iter()
        .filter_map(|guid| {
            let key = format!("{CAPTURE_ROOT}\\{guid}");
            if get_dword(&key, "DeviceState") != Some(1) {
                return None;
            }
            let props = format!("{key}\\Properties");
            let name = get_strings(&props, NAME_KEY).and_then(|s| s.into_iter().next()).unwrap_or_else(|| "Microphone".into());
            let desc = get_strings(&props, DESC_KEY).and_then(|s| s.into_iter().next());
            let full = match desc {
                Some(d) => format!("{name} ({d})"),
                None => name,
            };
            let fx = format!("{key}\\FxProperties");
            let hooked = get_strings(&fx, COMPOSITE_EFX).map(|l| l.iter().any(|s| s.to_ascii_lowercase() == clsid)).unwrap_or(false);
            let enhancements_off = get_dword(&props, DISABLE_SYSFX).unwrap_or(0) != 0;
            Some(Endpoint { id: format!("{{0.0.1.00000000}}.{guid}"), guid, name: full, hooked, enhancements_off })
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

pub fn is_virtual(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("cable") || n.contains("voicemeeter") || n.contains("virtual")
}

/// A virtual cable: the speaker side to send the voice to, and the microphone side
/// apps will record from.
pub fn find_cable(outputs: &[String], mics: &[Endpoint]) -> Option<(String, Endpoint)> {
    for out in outputs.iter().filter(|o| is_virtual(o)) {
        let want = out.replacen("Input", "Output", 1);
        if let Some(m) = mics.iter().find(|m| m.name == want) {
            return Some((out.clone(), m.clone()));
        }
    }
    let out = outputs.iter().find(|o| o.contains("CABLE Input"))?;
    let mic = mics.iter().find(|m| m.name.starts_with("CABLE Output"))?;
    Some((out.clone(), mic.clone()))
}

// ---------------------------------------------------------------- default microphone

#[windows_core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: windows_core::IUnknown {
    fn GetMixFormat(&self, id: *const u16, f: *mut *mut c_void) -> windows_core::HRESULT;
    fn GetDeviceFormat(&self, id: *const u16, d: i32, f: *mut *mut c_void) -> windows_core::HRESULT;
    fn ResetDeviceFormat(&self, id: *const u16) -> windows_core::HRESULT;
    fn SetDeviceFormat(&self, id: *const u16, a: *mut c_void, b: *mut c_void) -> windows_core::HRESULT;
    fn GetProcessingPeriod(&self, id: *const u16, d: i32, a: *mut i64, b: *mut i64) -> windows_core::HRESULT;
    fn SetProcessingPeriod(&self, id: *const u16, a: *mut i64) -> windows_core::HRESULT;
    fn GetShareMode(&self, id: *const u16, m: *mut c_void) -> windows_core::HRESULT;
    fn SetShareMode(&self, id: *const u16, m: *mut c_void) -> windows_core::HRESULT;
    fn GetPropertyValue(&self, id: *const u16, k: *const c_void, v: *mut c_void) -> windows_core::HRESULT;
    fn SetPropertyValue(&self, id: *const u16, k: *const c_void, v: *mut c_void) -> windows_core::HRESULT;
    fn SetDefaultEndpoint(&self, id: *const u16, role: i32) -> windows_core::HRESULT;
    fn SetEndpointVisibility(&self, id: *const u16, visible: i32) -> windows_core::HRESULT;
}

/// Run COM work on its own thread (the window's thread already chose its COM mode).
fn on_com_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        f()
    })
    .join()
    .expect("COM thread")
}

/// The id of Windows' default microphone (role 0 = console, 2 = communications).
pub fn default_capture_id(role: i32) -> Option<String> {
    on_com_thread(move || unsafe {
        use windows::Win32::Media::Audio::*;
        use windows::Win32::System::Com::*;
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(eCapture, ERole(role)).ok()?;
        let id = dev.GetId().ok()?;
        let s = id.to_string().ok();
        CoTaskMemFree(Some(id.0 as *const c_void));
        s
    })
}

/// Make `id` Windows' default microphone for every role (or only `roles`).
pub fn set_default_capture(id: &str, roles: &[i32]) -> Result<(), String> {
    let id = id.to_string();
    let roles = roles.to_vec();
    on_com_thread(move || unsafe {
        use windows::Win32::System::Com::*;
        let clsid = windows_core::GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
        let pc: IPolicyConfig = CoCreateInstance(&clsid, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let wid = w(&id);
        for r in roles {
            pc.SetDefaultEndpoint(wid.as_ptr(), r).ok().map_err(|e| e.to_string())?;
        }
        Ok(())
    })
}

// ---------------------------------------------------------------- admin helper

/// Re-run this program as administrator with `args` and wait. Returns its message.
pub fn run_elevated(args: &str) -> Result<String, String> {
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let result = std::env::temp_dir().join(format!("voicemorph-admin-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&result);
    let params = format!("{args} --result \"{}\"", result.display());
    let (verb, file, par) = (w("runas"), w(&exe.to_string_lossy()), w(&params));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = par.as_ptr();
        info.nShow = 0; // hidden
        if ShellExecuteExW(&mut info) == 0 {
            return Err("Windows didn't give admin permission (the prompt was closed or refused).".into());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        windows_sys::Win32::Foundation::CloseHandle(info.hProcess);
        let msg = std::fs::read_to_string(&result).unwrap_or_default();
        let _ = std::fs::remove_file(&result);
        if code == 0 {
            Ok(msg)
        } else {
            Err(if msg.is_empty() { format!("The installer stopped (code {code}).") } else { msg })
        }
    }
}

/// Handles --apo-uninstall when the app is re-run elevated: takes out the
/// microphone add-on an older version may have installed (the feature was removed).
/// Returns Some(exit code) when it was given.
pub fn admin_main(args: &[String]) -> Option<i32> {
    let pos = |flag: &str| args.iter().position(|a| a == flag);
    let result_path = pos("--result").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let outcome = if pos("--apo-uninstall").is_some() {
        uninstall()
    } else {
        return None;
    };
    let (code, msg) = match outcome {
        Ok(m) => (0, m),
        Err(e) => (1, e),
    };
    if let Some(p) = result_path {
        let _ = std::fs::write(p, msg);
    }
    Some(code)
}

fn quiet(cmd: &str, args: &[&str]) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(cmd).args(args).creation_flags(0x0800_0000).status().map(|s| s.success()).unwrap_or(false)
}

fn restart_audio_service() {
    quiet("net", &["stop", "audiosrv", "/y"]);
    quiet("net", &["start", "audiosrv"]);
}

fn install_dir() -> PathBuf {
    let pf = std::env::var_os("ProgramW6432").or_else(|| std::env::var_os("ProgramFiles")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    pf.join("VoiceMorph")
}

fn valid_guid(g: &str) -> bool {
    g.len() == 38 && g.starts_with('{') && g.ends_with('}') && g[1..37].chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn uninstall() -> Result<String, String> {
    let mut restored = 0;
    for guid in subkeys(BACKUP_ROOT) {
        if !valid_guid(&guid) {
            continue;
        }
        let backup = format!("{BACKUP_ROOT}\\{guid}");
        let fx = format!("{CAPTURE_ROOT}\\{guid}\\FxProperties");
        if let Ok(k) = open_write(&fx, false) {
            match get_strings(&backup, COMPOSITE_EFX) {
                Some(v) if v.first().map(|s| s == MISSING).unwrap_or(false) => k.delete(COMPOSITE_EFX),
                Some(v) => {
                    let _ = k.set_multi(COMPOSITE_EFX, &v);
                }
                None => {
                    // no backup value: just take ours out
                    if let Some(mut l) = get_strings(&fx, COMPOSITE_EFX) {
                        l.retain(|s| !s.eq_ignore_ascii_case(APO_CLSID_STR));
                        let _ = k.set_multi(COMPOSITE_EFX, &l);
                    }
                }
            }
            if let Some(v) = get_strings(&backup, FX_ASSOC) {
                if v.first().map(|s| s == MISSING).unwrap_or(false) {
                    k.delete(FX_ASSOC);
                }
            }
            restored += 1;
        }
        delete_tree(&backup);
    }
    delete_tree(&format!(r"SOFTWARE\Classes\AudioEngine\AudioProcessingObjects\{}", APO_CLSID_STR));
    delete_tree(&format!(r"SOFTWARE\Classes\CLSID\{}", APO_CLSID_STR));
    restart_audio_service();
    let _ = std::fs::remove_file(install_dir().join("voicemorph_apo.dll"));
    Ok(format!("Removed. {restored} microphone(s) are back to normal."))
}
