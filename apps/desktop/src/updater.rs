//! Sparkle's updater, loaded from the app bundle at runtime (`apps/desktop/scripts/embed-sparkle.sh`): it checks the
//! feed in Info.plist (`SUFeedURL`) for a newer build and installs it. The framework is only in Developer ID builds of
//! the release app; dev builds and unpackaged runs have no updater, so these do nothing.

use std::ffi::CString;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};

/// Sparkle's controller, alive for the process once [`start`] made it.
static CONTROLLER: AtomicPtr<AnyObject> = AtomicPtr::new(ptr::null_mut());

/// Whether this bundle carries Sparkle, so the menu can offer to check.
pub fn available() -> bool {
    framework().is_some()
}

/// Starts Sparkle's scheduled checks. Called once the first frame is up.
pub fn start() {
    if furiten_core::is_dev_build() {
        return;
    }
    if let Err(error) = start_inner() {
        eprintln!("furiten-desktop: {error}");
    }
}

/// Opens Sparkle's update window now.
pub fn check_for_updates() {
    let controller = CONTROLLER.load(Ordering::Relaxed);
    if controller.is_null() {
        return;
    }
    // SAFETY: `start_inner` stored a process-lifetime controller, and this runs on the main thread, from the menu.
    unsafe {
        let _: () = msg_send![controller, checkForUpdates: ptr::null_mut::<AnyObject>()];
    }
}

fn framework() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let contents = executable.parent()?.parent()?;
    let framework = contents.join("Frameworks/Sparkle.framework/Sparkle");
    framework.is_file().then_some(framework)
}

fn start_inner() -> Result<(), String> {
    let Some(framework) = framework() else { return Ok(()) };
    let framework = CString::new(framework.to_string_lossy().as_bytes())
        .map_err(|_| "the Sparkle framework path has a null byte")?;
    // SAFETY: Sparkle is a pinned, signed framework in the bundle; the handle and controller live for the process.
    unsafe {
        let handle = libc::dlopen(framework.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if handle.is_null() {
            return Err(format!("couldn't load Sparkle: {}", dlerror()));
        }
        let class = AnyClass::get(c"SPUStandardUpdaterController")
            .ok_or_else(|| "Sparkle's updater class is missing".to_owned())?;
        let controller: *mut AnyObject = msg_send![class, alloc];
        let controller: *mut AnyObject = msg_send![controller,
            initWithStartingUpdater: true,
            updaterDelegate: ptr::null_mut::<AnyObject>(),
            userDriverDelegate: ptr::null_mut::<AnyObject>()
        ];
        if controller.is_null() {
            return Err("Sparkle's updater didn't start".to_owned());
        }
        CONTROLLER.store(controller, Ordering::Relaxed);
    }
    Ok(())
}

fn dlerror() -> String {
    // SAFETY: `dlerror` returns null or a process-owned, null-terminated string.
    let error = unsafe { libc::dlerror() };
    if error.is_null() {
        return "unknown dynamic loader error".to_owned();
    }
    // SAFETY: checked for null; follows `dlerror`'s string contract.
    unsafe { std::ffi::CStr::from_ptr(error) }.to_string_lossy().into_owned()
}
