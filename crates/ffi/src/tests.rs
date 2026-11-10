use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

static CHANGES: AtomicUsize = AtomicUsize::new(0);

extern "C" fn changed(ctx: *mut c_void) {
    assert_eq!(ctx as usize, 7);
    CHANGES.fetch_add(1, Ordering::SeqCst);
}

fn request(core: *mut FuritenCore, line: &str) -> String {
    let line = CString::new(line).unwrap();
    unsafe {
        let reply = furiten_request(core, line.as_ptr());
        let text = CStr::from_ptr(reply).to_str().unwrap().to_string();
        furiten_string_free(reply);
        text
    }
}

#[test]
fn the_c_abi_passes_protocol_lines_through() {
    let id = furiten_core::model::ulid();
    let home = std::env::temp_dir().join(format!("rc-ffi-{}", &id[16..]));
    let home_c = CString::new(home.to_str().unwrap()).unwrap();
    unsafe {
        let core = furiten_open(home_c.as_ptr(), Some(changed), 7 as *mut c_void, ptr::null_mut());
        assert!(!core.is_null());
        assert!(request(core, r#"{"id":1,"method":"decks.list"}"#).contains("Furiten basics"));
        assert!(request(core, r#"{"id":2,"method":"decks.create","params":{"name":"Spanish"}}"#).contains("Spanish"));
        assert_eq!(CHANGES.load(Ordering::SeqCst), 1);

        let mut error = ptr::null_mut();
        assert!(furiten_open(home_c.as_ptr(), None, ptr::null_mut(), &mut error).is_null());
        assert!(CStr::from_ptr(error).to_str().unwrap().contains("already_running"));
        furiten_string_free(error);
        furiten_close(core);
    }
    std::fs::remove_dir_all(home).unwrap();
}
