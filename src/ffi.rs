//! C ABI surface — everything declared in `include/winrecovery.h`.
//!
//! Conventions:
//! - Functions returning `*mut c_char` hand ownership to the caller, who
//!   must release the string with [`wr_free_string`].
//! - On failure those functions return null; call [`wr_last_error`] (a
//!   thread-local message, valid until the next WinRecovery call on the same
//!   thread) and [`wr_last_error_code`] for details.
//! - Integer functions return `WR_OK` (0) or one of the `WR_ERR_*` codes.
//! - No function ever unwinds across the FFI boundary: all bodies run inside
//!   `catch_unwind`, and the release profile also sets `panic = "abort"` as
//!   a second line of defense (abort can only happen if a bug escapes the
//!   catch, which the tests exercise via null-argument paths).

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use crate::platform;
use crate::{WR_ERR_INTERNAL, WR_ERR_INVALID_ARG, WR_OK};

thread_local! {
    static LAST_ERROR: RefCell<(i32, CString)> =
        RefCell::new((WR_OK, CString::new("").unwrap()));
}

fn set_error(code: i32, msg: &str) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = (
            code,
            CString::new(msg).unwrap_or_else(|_| CString::new("error").unwrap()),
        );
    });
}

fn clear_error() {
    set_error(WR_OK, "");
}

fn into_c_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => {
            set_error(WR_ERR_INTERNAL, "result contained an interior NUL byte");
            std::ptr::null_mut()
        }
    }
}

/// Run a fallible platform op returning JSON; map its result to the C ABI.
fn json_op<T, F>(f: F, to_json: impl Fn(&T) -> String) -> *mut c_char
where
    F: FnOnce() -> platform::PResult<T>,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(value)) => {
            clear_error();
            into_c_string(to_json(&value))
        }
        Ok(Err((code, msg))) => {
            set_error(code, &msg);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_error(WR_ERR_INTERNAL, "internal panic caught at the FFI boundary");
            std::ptr::null_mut()
        }
    }
}

/// Run a fallible unit platform op; map its result to an integer code.
fn code_op<F>(f: F) -> c_int
where
    F: FnOnce() -> platform::PResult<()>,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(())) => {
            clear_error();
            WR_OK as c_int
        }
        Ok(Err((code, msg))) => {
            set_error(code, &msg);
            code as c_int
        }
        Err(_) => {
            set_error(WR_ERR_INTERNAL, "internal panic caught at the FFI boundary");
            WR_ERR_INTERNAL as c_int
        }
    }
}

fn opt_c_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        // SAFETY: caller contract (header) — pointer is a valid
        // NUL-terminated UTF-8 string or null.
        unsafe { CStr::from_ptr(p) }.to_str().ok()
    }
}

fn require_c_str(p: *const c_char) -> Result<String, (i32, String)> {
    if p.is_null() {
        return Err((WR_ERR_INVALID_ARG, "argument must not be null".into()));
    }
    // SAFETY: same caller contract as above.
    let s = unsafe { CStr::from_ptr(p) };
    s.to_str()
        .map(|v| v.to_string())
        .map_err(|_| (WR_ERR_INVALID_ARG, "argument is not valid UTF-8".into()))
}

// ---------------------------------------------------------------------------
// Version & errors
// ---------------------------------------------------------------------------

/// Static version string; do NOT free.
#[no_mangle]
pub extern "C" fn wr_version() -> *const c_char {
    // Library-owned static: nothing to allocate, nothing to free. Derived from
    // [`crate::VERSION`] at compile time rather than repeating the version
    // literal here, so the two cannot drift at the next bump.
    //
    // Deliberately avoids `concat!(crate::VERSION, "\0")`: passing a `const`
    // path to `concat!` is not something this crate needs to depend on, and
    // the array form below compiles on any Rust that can build this crate at
    // all. `VERSION` is a plain ASCII version string, so `as_bytes()` cannot
    // split a character, and `len() + 1` is the exact array length.
    static VERSION_C: [u8; crate::VERSION.len() + 1] = {
        let bytes = crate::VERSION.as_bytes();
        let mut buf = [0u8; crate::VERSION.len() + 1];
        let mut i = 0;
        while i < bytes.len() {
            buf[i] = bytes[i];
            i += 1;
        }
        buf // trailing byte stays 0: the NUL terminator
    };
    VERSION_C.as_ptr() as *const c_char
}

#[no_mangle]
pub extern "C" fn wr_version_major() -> u32 {
    crate::VERSION_MAJOR
}

#[no_mangle]
pub extern "C" fn wr_version_minor() -> u32 {
    crate::VERSION_MINOR
}

#[no_mangle]
pub extern "C" fn wr_version_patch() -> u32 {
    crate::VERSION_PATCH
}

/// Thread-local last-error message; do NOT free. Valid until the next
/// WinRecovery call on this thread.
#[no_mangle]
pub extern "C" fn wr_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().1.as_ptr())
}

#[no_mangle]
pub extern "C" fn wr_last_error_code() -> c_int {
    LAST_ERROR.with(|e| e.borrow().0) as c_int
}

/// Free a string returned by a `*_json` function. Null is a no-op.
///
/// # Safety (caller contract)
/// `p` must be null or a pointer returned by a WinRecovery `*_json`
/// function that has not been freed yet — the standard C ownership rule,
/// documented in the header. Extern "C" entry points cannot be `unsafe fn`
/// without breaking C callers, so the contract lives here and in the header.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn wr_free_string(p: *mut c_char) {
    if !p.is_null() {
        // SAFETY: caller contract — pointer came from this library's
        // CString::into_raw and has not been freed before.
        unsafe {
            drop(CString::from_raw(p));
        }
    }
}

// ---------------------------------------------------------------------------
// System Restore
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn wr_list_restore_points_json() -> *mut c_char {
    json_op(platform::list_restore_points, |points| {
        let items: Vec<String> = points.iter().map(|p| p.to_json()).collect();
        crate::json_array(&items)
    })
}

#[no_mangle]
pub extern "C" fn wr_create_restore_point(description: *const c_char) -> c_int {
    let desc = match require_c_str(description) {
        Ok(d) => d,
        Err((c, m)) => {
            set_error(c, &m);
            return c as c_int;
        }
    };
    code_op(|| platform::create_restore_point(&desc))
}

#[no_mangle]
pub extern "C" fn wr_delete_restore_point(sequence: u32) -> c_int {
    code_op(|| platform::delete_restore_point(sequence))
}

#[no_mangle]
pub extern "C" fn wr_restore_point_protection_json() -> *mut c_char {
    json_op(platform::restore_point_protection, |p| p.to_json())
}

// ---------------------------------------------------------------------------
// Recycle Bin
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn wr_recycle_bin_info_json(drive: *const c_char) -> *mut c_char {
    let d = opt_c_str(drive);
    json_op(|| platform::recycle_bin_info(d), |info| info.to_json())
}

#[no_mangle]
pub extern "C" fn wr_recycle_bin_empty(drive: *const c_char, no_confirmation: c_int) -> c_int {
    let d = opt_c_str(drive).map(|s| s.to_string());
    code_op(|| platform::recycle_bin_empty(d.as_deref(), no_confirmation != 0))
}

// ---------------------------------------------------------------------------
// Enumeration
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn wr_list_installed_programs_json() -> *mut c_char {
    json_op(platform::list_installed_programs, |progs| {
        let items: Vec<String> = progs.iter().map(|p| p.to_json()).collect();
        crate::json_array(&items)
    })
}

#[no_mangle]
pub extern "C" fn wr_list_startup_entries_json() -> *mut c_char {
    json_op(platform::list_startup_entries, |entries| {
        let items: Vec<String> = entries.iter().map(|e| e.to_json()).collect();
        crate::json_array(&items)
    })
}

#[no_mangle]
pub extern "C" fn wr_list_services_json() -> *mut c_char {
    json_op(
        || platform::list_services(false),
        |svcs| {
            let items: Vec<String> = svcs.iter().map(|s| s.to_json()).collect();
            crate::json_array(&items)
        },
    )
}

#[no_mangle]
pub extern "C" fn wr_list_drivers_json() -> *mut c_char {
    json_op(
        || platform::list_services(true),
        |svcs| {
            let items: Vec<String> = svcs.iter().map(|s| s.to_json()).collect();
            crate::json_array(&items)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn read_and_free(p: *mut c_char) -> String {
        assert!(!p.is_null());
        // SAFETY: p came from into_c_string in this crate's tests.
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_string();
        wr_free_string(p);
        s
    }

    #[test]
    fn version_exports() {
        let v = wr_version();
        // SAFETY: static NUL-terminated string.
        let s = unsafe { CStr::from_ptr(v) }.to_str().unwrap();
        assert_eq!(s, crate::VERSION);
        assert_eq!(wr_version_major(), 0);

        // Guard the derivation in wr_version(): the byte exactly past the
        // string must be the NUL, and none before it may be. Without this, a
        // truncating or off-by-one version of that array would still read back
        // as a shorter-but-valid string.
        let nul_at = crate::VERSION.len();
        // SAFETY: wr_version() returns a pointer to a static
        // [u8; VERSION.len() + 1], so both offsets are in bounds.
        let tail_zero = unsafe { *v.add(nul_at) } == 0;
        assert!(tail_zero, "missing NUL terminator after the version string");
        assert_eq!(
            unsafe { CStr::from_ptr(v) }.to_bytes().len(),
            nul_at,
            "version string is truncated or contains an interior NUL"
        );
    }

    #[test]
    fn null_description_is_invalid_arg() {
        let rc = wr_create_restore_point(std::ptr::null());
        assert_eq!(rc, WR_ERR_INVALID_ARG as c_int);
        assert_eq!(wr_last_error_code(), WR_ERR_INVALID_ARG as c_int);
        let msg = wr_last_error();
        // SAFETY: thread-local static message.
        let s = unsafe { CStr::from_ptr(msg) }.to_str().unwrap();
        assert!(s.contains("null"));
    }

    #[test]
    fn free_null_is_noop() {
        wr_free_string(std::ptr::null_mut());
    }

    #[test]
    #[cfg(not(windows))]
    fn platform_calls_report_not_supported() {
        let p = wr_list_restore_points_json();
        assert!(p.is_null());
        assert_eq!(wr_last_error_code(), crate::WR_ERR_NOT_SUPPORTED as c_int);
        let rc = wr_delete_restore_point(1);
        assert_eq!(rc, crate::WR_ERR_NOT_SUPPORTED as c_int);
        // Recycle-bin JSON also follows the null + error contract.
        let p = wr_recycle_bin_info_json(std::ptr::null());
        assert!(p.is_null());
        let _ = read_and_free; // silence helper on this cfg path
    }
}
