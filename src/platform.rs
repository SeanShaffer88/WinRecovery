//! Platform layer: the only place Windows APIs are touched.
//!
//! On non-Windows targets every operation returns
//! [`WR_ERR_NOT_SUPPORTED`] with a clear message — the parsers in the crate
//! root still run and are tested there. On Windows, bindings are declared by
//! hand (zero-dependency design); each `unsafe` block states its invariants.

use crate::{
    InstalledProgram, Protection, RecycleBinInfo, RestorePoint, ServiceEntry, StartupEntry,
    WR_ERR_NOT_SUPPORTED, WR_ERR_PLATFORM,
};

/// A platform result: value, or (error code, human message).
pub type PResult<T> = Result<T, (i32, String)>;

pub fn not_supported<T>(what: &str) -> PResult<T> {
    Err((
        WR_ERR_NOT_SUPPORTED,
        format!("{what} is only available on Windows"),
    ))
}

pub fn platform_err<T>(msg: impl Into<String>) -> PResult<T> {
    Err((WR_ERR_PLATFORM, msg.into()))
}

// ---------------------------------------------------------------------------
// Public surface (dispatched to the Windows implementation or the stub)
// ---------------------------------------------------------------------------

pub fn list_restore_points() -> PResult<Vec<RestorePoint>> {
    #[cfg(windows)]
    {
        windows::list_restore_points()
    }
    #[cfg(not(windows))]
    {
        not_supported("System Restore")
    }
}

pub fn create_restore_point(description: &str) -> PResult<()> {
    #[cfg(windows)]
    {
        windows::create_restore_point(description)
    }
    #[cfg(not(windows))]
    {
        let _ = description;
        not_supported("System Restore")
    }
}

pub fn delete_restore_point(sequence: u32) -> PResult<()> {
    #[cfg(windows)]
    {
        windows::delete_restore_point(sequence)
    }
    #[cfg(not(windows))]
    {
        let _ = sequence;
        not_supported("System Restore")
    }
}

pub fn restore_point_protection() -> PResult<Protection> {
    #[cfg(windows)]
    {
        windows::restore_point_protection()
    }
    #[cfg(not(windows))]
    {
        not_supported("System Protection status")
    }
}

pub fn recycle_bin_info(drive: Option<&str>) -> PResult<RecycleBinInfo> {
    #[cfg(windows)]
    {
        windows::recycle_bin_info(drive)
    }
    #[cfg(not(windows))]
    {
        let _ = drive;
        not_supported("Recycle Bin")
    }
}

pub fn recycle_bin_empty(drive: Option<&str>, no_confirmation: bool) -> PResult<()> {
    #[cfg(windows)]
    {
        windows::recycle_bin_empty(drive, no_confirmation)
    }
    #[cfg(not(windows))]
    {
        let _ = (drive, no_confirmation);
        not_supported("Recycle Bin")
    }
}

pub fn list_installed_programs() -> PResult<Vec<InstalledProgram>> {
    #[cfg(windows)]
    {
        windows::list_installed_programs()
    }
    #[cfg(not(windows))]
    {
        not_supported("Installed-program enumeration")
    }
}

pub fn list_startup_entries() -> PResult<Vec<StartupEntry>> {
    #[cfg(windows)]
    {
        windows::list_startup_entries()
    }
    #[cfg(not(windows))]
    {
        not_supported("Startup enumeration")
    }
}

pub fn list_services(drivers_only: bool) -> PResult<Vec<ServiceEntry>> {
    #[cfg(windows)]
    {
        windows::list_services(drivers_only)
    }
    #[cfg(not(windows))]
    {
        let _ = drivers_only;
        not_supported("Service enumeration")
    }
}

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::{
        normalize_install_date, parse_restore_points, parse_shadowstorage, sanitize_description,
    };
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::process::{Command, Stdio};

    type Hkey = isize;
    type Handle = isize;
    const HKEY_LOCAL_MACHINE: Hkey = 0x80000002u32 as i32 as isize;
    const HKEY_CURRENT_USER: Hkey = 0x80000001u32 as i32 as isize;
    const KEY_READ: u32 = 0x20019;
    const KEY_WOW64_64KEY: u32 = 0x0100;
    const KEY_WOW64_32KEY: u32 = 0x0200;
    const REG_SZ: u32 = 1;
    const REG_EXPAND_SZ: u32 = 2;
    const REG_DWORD: u32 = 4;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    fn from_wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        std::ffi::OsString::from_wide(&buf[..end])
            .to_string_lossy()
            .into_owned()
    }

    extern "system" {
        fn RegOpenKeyExW(hkey: Hkey, sub: *const u16, opts: u32, sam: u32, out: *mut Hkey) -> i32;
        fn RegCloseKey(hkey: Hkey) -> i32;
        fn RegEnumKeyExW(
            hkey: Hkey,
            index: u32,
            name: *mut u16,
            name_len: *mut u32,
            reserved: *mut u32,
            class: *mut u16,
            class_len: *mut u32,
            last_write: *mut c_void,
        ) -> i32;
        fn RegQueryValueExW(
            hkey: Hkey,
            name: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            data_len: *mut u32,
        ) -> i32;
        fn RegEnumValueW(
            hkey: Hkey,
            index: u32,
            name: *mut u16,
            name_len: *mut u32,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            data_len: *mut u32,
        ) -> i32;
        fn LoadLibraryW(name: *const u16) -> Handle;
        fn FreeLibrary(lib: Handle) -> i32;
        fn GetProcAddress(lib: Handle, name: *const u8) -> Option<unsafe extern "system" fn()>;
        fn GetLastError() -> u32;
    }

    // shell32 — Recycle Bin. SHQUERYRBINFO layout per Win32 docs:
    // cbSize (DWORD), i64Size (i64), i64NumItems (i64).
    #[repr(C)]
    struct ShQueryRbInfo {
        cb_size: u32,
        size: i64,
        num_items: i64,
    }
    extern "system" {
        fn SHQueryRecycleBinW(root: *const u16, info: *mut ShQueryRbInfo) -> i32;
        fn SHEmptyRecycleBinW(hwnd: Handle, root: *const u16, flags: u32) -> i32;
    }
    const SHERB_NOCONFIRMATION: u32 = 0x1;
    const SHERB_NOPROGRESSUI: u32 = 0x2;
    const SHERB_NOSOUND: u32 = 0x4;

    // advapi32 — Service Control Manager.
    extern "system" {
        fn OpenSCManagerW(machine: *const u16, db: *const u16, access: u32) -> Handle;
        fn CloseServiceHandle(h: Handle) -> i32;
        fn EnumServicesStatusExW(
            scm: Handle,
            level: u32,
            svc_type: u32,
            state: u32,
            buffer: *mut u8,
            buf_size: u32,
            bytes_needed: *mut u32,
            returned: *mut u32,
            resume: *mut u32,
            group: *const u16,
        ) -> i32;
    }
    const SC_MANAGER_ENUMERATE_SERVICE: u32 = 0x0004;
    const SERVICE_WIN32: u32 = 0x30;
    const SERVICE_DRIVER: u32 = 0x0B;
    const SERVICE_STATE_ALL: u32 = 3;

    #[repr(C)]
    struct ServiceStatusProcess {
        service_type: u32,
        current_state: u32,
        controls_accepted: u32,
        win32_exit_code: u32,
        service_specific_exit_code: u32,
        check_point: u32,
        wait_hint: u32,
        process_id: u32,
        service_flags: u32,
    }
    #[repr(C)]
    struct EnumServiceStatusProcessW {
        service_name: *mut u16,
        display_name: *mut u16,
        status: ServiceStatusProcess,
    }

    fn run_hidden(program: &str, args: &[&str]) -> PResult<(i32, String)> {
        use std::os::windows::process::CommandExt;
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| (WR_ERR_PLATFORM, format!("could not run {program}: {e}")))?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        Ok((out.status.code().unwrap_or(-1), text))
    }

    fn powershell(script: &str) -> PResult<String> {
        let (code, out) = run_hidden(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ],
        )?;
        if code != 0 {
            return platform_err(out.trim().to_string());
        }
        Ok(out)
    }

    // -- System Restore ----------------------------------------------------

    pub fn list_restore_points() -> PResult<Vec<RestorePoint>> {
        let out = powershell(
            "Get-ComputerRestorePoint | Select-Object SequenceNumber,Description,CreationTime,RestorePointType | ConvertTo-Json -Compress",
        )?;
        Ok(parse_restore_points(&out))
    }

    pub fn create_restore_point(description: &str) -> PResult<()> {
        let safe = sanitize_description(description);
        // Same approach as InstallScope's restore.rs: temporarily lift the
        // 24-hour creation-frequency limit for this request, restore it after.
        let script = format!(
            "$k='HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore'; \
             $old=(Get-ItemProperty -Path \"Registry::$k\" -Name SystemRestorePointCreationFrequency -ErrorAction SilentlyContinue).SystemRestorePointCreationFrequency; \
             New-ItemProperty -Path \"Registry::$k\" -Name SystemRestorePointCreationFrequency -Value 0 -PropertyType DWord -Force | Out-Null; \
             try {{ Checkpoint-Computer -Description '{safe}' -RestorePointType 'MODIFY_SETTINGS' -ErrorAction Stop }} \
             finally {{ if ($null -eq $old) {{ Remove-ItemProperty -Path \"Registry::$k\" -Name SystemRestorePointCreationFrequency -ErrorAction SilentlyContinue }} else {{ Set-ItemProperty -Path \"Registry::$k\" -Name SystemRestorePointCreationFrequency -Value $old }} }}"
        );
        powershell(&script).map(|_| ()).map_err(|(c, m)| {
            (
                c,
                format!("restore point failed: {m} (administrator rights are usually required)"),
            )
        })
    }

    pub fn delete_restore_point(sequence: u32) -> PResult<()> {
        // srclient.dll is loaded at runtime (InstallScope pattern): it is not
        // present on every edition, and a missing DLL must be a clean error,
        // not a loader failure for the whole DLL.
        let name = wide("srclient.dll");
        unsafe {
            // Invariant: `name` is a valid NUL-terminated wide string for the call.
            let lib = LoadLibraryW(name.as_ptr());
            if lib == 0 {
                return platform_err("System Restore (srclient.dll) is not available");
            }
            // Invariant: the export name is a valid NUL-terminated C string.
            let proc = GetProcAddress(lib, c"SRRemoveRestorePoint".as_ptr() as *const u8);
            let result = match proc {
                Some(f) => {
                    let remove: unsafe extern "system" fn(u32) -> u32 = std::mem::transmute(f);
                    remove(sequence)
                }
                None => {
                    FreeLibrary(lib);
                    return platform_err("SRRemoveRestorePoint is not available");
                }
            };
            FreeLibrary(lib);
            if result == 0 {
                Ok(())
            } else {
                platform_err(format!(
                    "Windows could not delete the restore point (error {result})"
                ))
            }
        }
    }

    pub fn restore_point_protection() -> PResult<Protection> {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let mut p = Protection {
            drive: drive.clone(),
            ..Default::default()
        };
        if let Ok(out) = powershell(
            "(Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SPP\\Clients' -ErrorAction SilentlyContinue).'{09F7EDC5-294E-4180-AF6A-FB0E6A0E9513}' -join \"`n\"",
        ) {
            let enc = format!("({}%3A", drive.trim_end_matches(':'));
            p.enabled = out.to_ascii_uppercase().contains(&enc.to_ascii_uppercase());
        }
        if let Ok((0, out)) = run_hidden("vssadmin", &["list", "shadowstorage"]) {
            let (used, max) = parse_shadowstorage(&out, &drive);
            p.used_bytes = used;
            p.max_bytes = max;
        }
        Ok(p)
    }

    // -- Recycle Bin ---------------------------------------------------------

    pub fn recycle_bin_info(drive: Option<&str>) -> PResult<RecycleBinInfo> {
        let root = drive.map(wide);
        let mut info = ShQueryRbInfo {
            cb_size: std::mem::size_of::<ShQueryRbInfo>() as u32,
            size: 0,
            num_items: 0,
        };
        // Invariant: `info` is a valid writable SHQUERYRBINFO with cbSize set;
        // `root` (when present) is NUL-terminated for the duration of the call.
        let hr = unsafe {
            SHQueryRecycleBinW(
                root.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                &mut info,
            )
        };
        if hr != 0 {
            return platform_err(format!(
                "SHQueryRecycleBin failed (HRESULT 0x{:08X})",
                hr as u32
            ));
        }
        Ok(RecycleBinInfo {
            drive: drive.unwrap_or("").to_string(),
            item_count: info.num_items.max(0) as u64,
            size_bytes: info.size.max(0) as u64,
        })
    }

    pub fn recycle_bin_empty(drive: Option<&str>, no_confirmation: bool) -> PResult<()> {
        let root = drive.map(wide);
        let mut flags = SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        if no_confirmation {
            flags |= SHERB_NOCONFIRMATION;
        }
        // Invariant: hwnd 0 = no owner window (documented); root as above.
        let hr = unsafe {
            SHEmptyRecycleBinW(
                0,
                root.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                flags,
            )
        };
        if hr != 0 {
            return platform_err(format!(
                "SHEmptyRecycleBin failed (HRESULT 0x{:08X})",
                hr as u32
            ));
        }
        Ok(())
    }

    // -- Registry helpers ----------------------------------------------------

    struct KeyGuard(Hkey);
    impl Drop for KeyGuard {
        fn drop(&mut self) {
            // Invariant: handle came from RegOpenKeyExW and is closed once.
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }

    fn open_key(root: Hkey, sub: &str, wow: u32) -> Option<KeyGuard> {
        let w = wide(sub);
        let mut h: Hkey = 0;
        // Invariant: `w` is NUL-terminated; `h` is a valid out-slot.
        let rc = unsafe { RegOpenKeyExW(root, w.as_ptr(), 0, KEY_READ | wow, &mut h) };
        if rc == 0 {
            Some(KeyGuard(h))
        } else {
            None
        }
    }

    fn subkeys(key: &KeyGuard) -> Vec<String> {
        let mut out = Vec::new();
        let mut index = 0;
        loop {
            let mut buf = vec![0u16; 512];
            let mut len = buf.len() as u32;
            // Invariant: buf/len describe a writable 512-WCHAR buffer.
            let rc = unsafe {
                RegEnumKeyExW(
                    key.0,
                    index,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if rc != 0 {
                break;
            }
            buf.truncate(len as usize);
            out.push(from_wide(&buf));
            index += 1;
        }
        out
    }

    fn value_string(key: &KeyGuard, name: &str) -> Option<String> {
        let w = wide(name);
        let mut kind: u32 = 0;
        let mut len: u32 = 0;
        // First call sizes the buffer (data = null is documented for sizing).
        let rc = unsafe {
            RegQueryValueExW(
                key.0,
                w.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if rc != 0 || len == 0 {
            return None;
        }
        if kind == REG_DWORD {
            if len < 4 {
                return None;
            }
            let mut data = vec![0u8; len as usize];
            let rc = unsafe {
                RegQueryValueExW(
                    key.0,
                    w.as_ptr(),
                    std::ptr::null_mut(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut len,
                )
            };
            if rc != 0 {
                return None;
            }
            let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            return Some(v.to_string());
        }
        if kind != REG_SZ && kind != REG_EXPAND_SZ {
            return None;
        }
        let mut data = vec![0u8; len as usize];
        let rc = unsafe {
            RegQueryValueExW(
                key.0,
                w.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                data.as_mut_ptr(),
                &mut len,
            )
        };
        if rc != 0 {
            return None;
        }
        let units: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = from_wide(&units);
        if kind == REG_EXPAND_SZ {
            Some(expand_env(&s))
        } else {
            Some(s)
        }
    }

    fn value_dword(key: &KeyGuard, name: &str) -> Option<u32> {
        value_string(key, name)?.parse().ok()
    }

    fn enum_values(key: &KeyGuard) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut index = 0;
        loop {
            let mut name_buf = vec![0u16; 512];
            let mut name_len = name_buf.len() as u32;
            let mut kind: u32 = 0;
            let mut data = vec![0u8; 8192];
            let mut data_len = data.len() as u32;
            // Invariant: both buffers are sized as described by their lengths.
            let rc = unsafe {
                RegEnumValueW(
                    key.0,
                    index,
                    name_buf.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null_mut(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut data_len,
                )
            };
            if rc != 0 {
                break;
            }
            name_buf.truncate(name_len as usize);
            let name = from_wide(&name_buf);
            let value = if kind == REG_SZ || kind == REG_EXPAND_SZ {
                let units: Vec<u16> = data[..data_len as usize]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let s = from_wide(&units);
                if kind == REG_EXPAND_SZ {
                    expand_env(&s)
                } else {
                    s
                }
            } else {
                String::new()
            };
            out.push((name, value));
            index += 1;
        }
        out
    }

    fn expand_env(s: &str) -> String {
        // Small hand expansion for the common %VAR% form using the process
        // environment (sufficient for uninstall/startup display strings).
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(start) = rest.find('%') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('%') {
                Some(end) => {
                    let var = &after[..end];
                    match std::env::var(var) {
                        Ok(v) => out.push_str(&v),
                        Err(_) => {
                            out.push('%');
                            out.push_str(var);
                            out.push('%');
                        }
                    }
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    // -- Installed programs ----------------------------------------------------

    pub fn list_installed_programs() -> PResult<Vec<InstalledProgram>> {
        const UNINSTALL: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
        let roots: [(Hkey, &str, u32, &str); 3] = [
            (HKEY_LOCAL_MACHINE, UNINSTALL, KEY_WOW64_64KEY, "machine64"),
            (HKEY_LOCAL_MACHINE, UNINSTALL, KEY_WOW64_32KEY, "machine32"),
            (HKEY_CURRENT_USER, UNINSTALL, KEY_WOW64_64KEY, "user"),
        ];
        let mut out = Vec::new();
        for (root, path, wow, source) in roots {
            let Some(key) = open_key(root, path, wow) else {
                continue;
            };
            for leaf in subkeys(&key) {
                let sub_path = format!("{path}\\{leaf}");
                let Some(sub) = open_key(root, &sub_path, wow) else {
                    continue;
                };
                let Some(name) = value_string(&sub, "DisplayName") else {
                    continue;
                };
                if name.trim().is_empty() {
                    continue;
                }
                out.push(InstalledProgram {
                    key: leaf,
                    name,
                    publisher: value_string(&sub, "Publisher").unwrap_or_default(),
                    version: value_string(&sub, "DisplayVersion").unwrap_or_default(),
                    install_date: normalize_install_date(
                        &value_string(&sub, "InstallDate").unwrap_or_default(),
                    ),
                    install_location: value_string(&sub, "InstallLocation").unwrap_or_default(),
                    uninstall_string: value_string(&sub, "UninstallString").unwrap_or_default(),
                    size_kb: value_dword(&sub, "EstimatedSize").unwrap_or(0) as u64,
                    source: match source {
                        "machine64" => "machine64",
                        "machine32" => "machine32",
                        _ => "user",
                    },
                    system_component: value_dword(&sub, "SystemComponent").unwrap_or(0) != 0,
                });
            }
        }
        out.sort_by_key(|a| a.name.to_lowercase());
        Ok(out)
    }

    // -- Startup entries -------------------------------------------------------

    pub fn list_startup_entries() -> PResult<Vec<StartupEntry>> {
        let mut out = Vec::new();
        let run_keys: [(Hkey, &str, &str); 4] = [
            (
                HKEY_LOCAL_MACHINE,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
                "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
            ),
            (
                HKEY_LOCAL_MACHINE,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
                "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
            ),
            (
                HKEY_CURRENT_USER,
                "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            ),
            (
                HKEY_CURRENT_USER,
                "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce",
            ),
        ];
        for (root, path, label) in run_keys {
            if let Some(key) = open_key(root, path, KEY_WOW64_64KEY) {
                for (name, command) in enum_values(&key) {
                    if !command.is_empty() {
                        out.push(StartupEntry {
                            name,
                            command,
                            location: label.to_string(),
                            source: "registry",
                        });
                    }
                }
            }
        }
        // Startup folders (per-user + common).
        let mut folders: Vec<String> = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            folders.push(format!(
                "{appdata}\\Microsoft\\Windows\\Start Menu\\Programs\\Startup"
            ));
        }
        if let Ok(program_data) = std::env::var("ProgramData") {
            folders.push(format!(
                "{program_data}\\Microsoft\\Windows\\Start Menu\\Programs\\Startup"
            ));
        }
        for folder in folders {
            if let Ok(rd) = std::fs::read_dir(&folder) {
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.eq_ignore_ascii_case("desktop.ini") {
                        continue;
                    }
                    out.push(StartupEntry {
                        name,
                        command: entry.path().to_string_lossy().into_owned(),
                        location: folder.clone(),
                        source: "folder",
                    });
                }
            }
        }
        Ok(out)
    }

    // -- Services & drivers ------------------------------------------------------

    pub fn list_services(drivers_only: bool) -> PResult<Vec<ServiceEntry>> {
        // Invariant: null machine/database names select the local SCM (documented).
        let scm = unsafe {
            OpenSCManagerW(
                std::ptr::null(),
                std::ptr::null(),
                SC_MANAGER_ENUMERATE_SERVICE,
            )
        };
        if scm == 0 {
            // SAFETY: GetLastError is thread-local and read immediately.
            let code = unsafe { GetLastError() };
            return platform_err(format!(
                "could not open the Service Control Manager (error {code})"
            ));
        }
        let svc_type = if drivers_only {
            SERVICE_DRIVER
        } else {
            SERVICE_WIN32 | SERVICE_DRIVER
        };
        let mut needed: u32 = 0;
        let mut returned: u32 = 0;
        let mut resume: u32 = 0;
        // Sizing call: null buffer with size 0 is the documented pattern;
        // it fails with ERROR_MORE_DATA and reports `needed`.
        unsafe {
            EnumServicesStatusExW(
                scm,
                0,
                svc_type,
                SERVICE_STATE_ALL,
                std::ptr::null_mut(),
                0,
                &mut needed,
                &mut returned,
                &mut resume,
                std::ptr::null(),
            );
        }
        if needed == 0 {
            unsafe {
                CloseServiceHandle(scm);
            }
            return Ok(Vec::new());
        }
        let mut buf = vec![0u8; needed as usize];
        let mut needed2: u32 = 0;
        let mut returned2: u32 = 0;
        let mut resume2: u32 = 0;
        // Invariant: buf holds `needed` writable bytes as reported above.
        let ok = unsafe {
            EnumServicesStatusExW(
                scm,
                0,
                svc_type,
                SERVICE_STATE_ALL,
                buf.as_mut_ptr(),
                needed,
                &mut needed2,
                &mut returned2,
                &mut resume2,
                std::ptr::null(),
            )
        };
        let entries = if ok != 0 {
            // Invariant: on success the buffer holds `returned2` consecutive
            // ENUM_SERVICE_STATUS_PROCESSW records whose name pointers point
            // into the same buffer (documented SCM behavior).
            let count = returned2 as usize;
            let stride = std::mem::size_of::<EnumServiceStatusProcessW>();
            let mut v = Vec::with_capacity(count);
            for i in 0..count {
                let rec =
                    unsafe { &*(buf.as_ptr().add(i * stride) as *const EnumServiceStatusProcessW) };
                let read = |p: *mut u16| -> String {
                    if p.is_null() {
                        return String::new();
                    }
                    let mut len = 0usize;
                    unsafe {
                        while *p.add(len) != 0 {
                            len += 1;
                        }
                        from_wide(std::slice::from_raw_parts(p, len))
                    }
                };
                v.push(ServiceEntry {
                    name: read(rec.service_name),
                    display_name: read(rec.display_name),
                    state: ServiceEntry::state_label(rec.status.current_state),
                    process_id: rec.status.process_id,
                    is_driver: rec.status.service_type & SERVICE_DRIVER != 0,
                });
            }
            v
        } else {
            Vec::new()
        };
        unsafe {
            CloseServiceHandle(scm);
        }
        let mut entries: Vec<ServiceEntry> = entries
            .into_iter()
            .filter(|e| !drivers_only || e.is_driver)
            .collect();
        entries.sort_by_key(|a| a.name.to_lowercase());
        Ok(entries)
    }
}
