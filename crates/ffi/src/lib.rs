//! Furiten's core behind a C ABI (`include/furiten.h`), for the mobile apps that link it in. It hosts an
//! [`Embedded`] core and passes the protocol's JSON lines through untouched, so the apps speak exactly what the
//! desktop socket does.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;
use std::ptr;

use furiten_core::Error;
use furiten_core::server::Embedded;

pub struct FuritenCore(Embedded);

pub type FuritenChanged = Option<extern "C" fn(*mut c_void)>;

/// The host's callback and context, called from core threads. The host promises both stay valid, and are safe to
/// call from any thread, until `furiten_close` returns.
struct Listener {
    on_change: FuritenChanged,
    ctx: *mut c_void,
}

unsafe impl Send for Listener {}
unsafe impl Sync for Listener {}

impl Listener {
    fn call(&self) {
        if let Some(on_change) = self.on_change {
            on_change(self.ctx);
        }
    }
}

/// # Safety
/// `home` must be a NUL-terminated UTF-8 path, and `error`, if not null, writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn furiten_open(
    home: *const c_char,
    on_change: FuritenChanged,
    ctx: *mut c_void,
    error: *mut *mut c_char,
) -> *mut FuritenCore {
    let listener = Listener { on_change, ctx };
    let opened = unsafe { str_arg(home) }.and_then(|home| Embedded::open(Path::new(home), move || listener.call()));
    match opened {
        Ok(core) => Box::into_raw(Box::new(FuritenCore(core))),
        Err(e) => {
            if !error.is_null() {
                unsafe { *error = into_c(serde_json::json!(e).to_string()) };
            }
            ptr::null_mut()
        }
    }
}

/// # Safety
/// `core` must come from `furiten_open` and not be closed; `request` must be NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn furiten_request(core: *mut FuritenCore, request: *const c_char) -> *mut c_char {
    let core = unsafe { &*core };
    let reply = match unsafe { str_arg(request) } {
        Ok(line) => core.0.request(line),
        Err(e) => serde_json::json!({ "id": null, "error": e }).to_string(),
    };
    into_c(reply)
}

/// # Safety
/// `core` must come from `furiten_open` and not be closed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn furiten_sync_soon(core: *mut FuritenCore) {
    unsafe { &*core }.0.sync_soon();
}

/// # Safety
/// `s` must come from this library, once, or be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn furiten_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// # Safety
/// `core` must come from `furiten_open`, once, or be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn furiten_close(core: *mut FuritenCore) {
    if !core.is_null() {
        drop(unsafe { Box::from_raw(core) });
    }
}

unsafe fn str_arg<'a>(s: *const c_char) -> Result<&'a str, Error> {
    if s.is_null() {
        return Err(Error::invalid("null string"));
    }
    unsafe { CStr::from_ptr(s) }.to_str().map_err(|_| Error::invalid("not UTF-8"))
}

fn into_c(s: String) -> *mut c_char {
    // JSON never holds a raw NUL, but don't trust it with the process.
    CString::new(s)
        .unwrap_or_else(|_| c"{\"id\":null,\"error\":{\"code\":\"internal\",\"message\":\"NUL in reply\"}}".into())
        .into_raw()
}

#[cfg(test)]
mod tests;
