//! The updater's preferences and the running app's version.
//!
//! Settings reuse the Swift app's key names via `fumi_ui::settings` so they
//! live in the same `NSUserDefaults` domain and survive reinstalls.

use fumi_ui::settings::{self, keys};
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::CStr;

/// New keys, alongside the Swift app's, for the two choices the updater adds.
pub mod pref {
    /// The auto-download key is shared with the settings sheet.
    pub const AUTO_DOWNLOAD: &str = fumi_ui::settings::keys::AUTO_DOWNLOAD_UPDATES;
    pub const LAST_CHECK: &str = "fumi.lastUpdateCheck";
    pub const SKIPPED_VERSION: &str = "fumi.skippedUpdateVersion";
}

/// Whether Fumi should check the feed on launch and periodically. Default on,
/// matching the Swift app's `autoUpdates` default.
pub fn auto_check() -> bool {
    settings::get_bool(keys::AUTO_UPDATES, true)
}

pub fn set_auto_check(value: bool) {
    settings::set_bool(keys::AUTO_UPDATES, value);
}

/// Whether a discovered update downloads itself, or waits for the user to
/// press Download. Default off — a check alone stays quiet and lightweight.
pub fn auto_download() -> bool {
    settings::get_bool(pref::AUTO_DOWNLOAD, false)
}

pub fn set_auto_download(value: bool) {
    settings::set_bool(pref::AUTO_DOWNLOAD, value);
}

pub fn skipped_version() -> Option<String> {
    settings::get_string(pref::SKIPPED_VERSION)
}

pub fn set_skipped_version(version: Option<&str>) {
    match version {
        Some(v) => settings::set_string(pref::SKIPPED_VERSION, v),
        None => settings::remove(pref::SKIPPED_VERSION),
    }
}

pub fn last_check() -> i64 {
    settings::get_i64(pref::LAST_CHECK, 0)
}

pub fn set_last_check_now() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    settings::set_i64(pref::LAST_CHECK, now);
}

fn info_string(key: &str) -> Option<String> {
    unsafe {
        let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
        let info: *mut Object = msg_send![bundle, infoDictionary];
        if info.is_null() {
            return None;
        }
        let k: *mut Object = fumi_ui::native::ns(key);
        let value: *mut Object = msg_send![info, objectForKey: k];
        if value.is_null() {
            return None;
        }
        let utf8: *const std::os::raw::c_char = msg_send![value, UTF8String];
        (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}

/// The running version from the main bundle, e.g. `0.4.1`.
pub fn current_version() -> String {
    info_string("CFBundleShortVersionString").unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// The running build number (`CFBundleVersion`), 0 when absent.
pub fn current_build() -> u64 {
    info_string("CFBundleVersion").and_then(|s| s.parse().ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_version_round_trips() {
        let _guard = crate::TEST_LOCK.lock().unwrap();
        set_skipped_version(Some("0.4.2"));
        assert_eq!(skipped_version().as_deref(), Some("0.4.2"));
        set_skipped_version(None);
        assert_eq!(skipped_version(), None);
    }

    #[test]
    fn auto_download_defaults_off_and_round_trips() {
        let _guard = crate::TEST_LOCK.lock().unwrap();
        settings::remove(pref::AUTO_DOWNLOAD);
        assert!(!auto_download());
        set_auto_download(true);
        assert!(auto_download());
        set_auto_download(false);
        assert!(!auto_download());
    }
}
