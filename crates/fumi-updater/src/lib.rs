//! In-app updates for Fumi.
//!
//! A quiet check reads a static JSON feed, compares it with the running app's
//! version, and — with the user's go-ahead — downloads, verifies and installs a
//! signed DMG, then relaunches. No server and no user identifiers are involved.
//!
//! The crate is split so the decision logic (`appcast`, `version`, `net`,
//! `install`) is pure and unit-testable, while `update` reads preferences and
//! `prompt` draws the GPUI dialog.

#![allow(unexpected_cfgs)]

pub mod appcast;
pub mod install;
pub mod net;
pub mod prompt;
pub mod update;
pub mod version;

pub use appcast::{Asset, Release};
pub use version::Version;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// One lock for every test that mutates process-wide state (environment
/// variables and `NSUserDefaults`). Cargo runs tests in parallel; without
/// this, the check harness and the preference round-trips race on the
/// skipped-version key and one flakes.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

/// The outcome of a single update check, ready for the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckResult {
    /// A newer version was found and hasn't been skipped.
    Available(Box<Release>),
    /// The feed was read successfully but nothing newer applies.
    UpToDate,
    /// The check failed (offline, bad feed, …). Silent in the UI.
    Failed(String),
}

static CHECKING: AtomicBool = AtomicBool::new(false);
static LAST_RESULT: Mutex<Option<CheckResult>> = Mutex::new(None);

/// Whether a check is currently in flight (guards against duplicate timers).
pub fn is_checking() -> bool {
    CHECKING.load(Ordering::Relaxed)
}

pub fn last_result() -> Option<CheckResult> {
    LAST_RESULT.lock().ok().and_then(|g| g.clone())
}

fn set_result(result: CheckResult) {
    if let Ok(mut g) = LAST_RESULT.lock() {
        *g = Some(result);
    }
}

/// The feed URL, overridable for tests and forks.
pub fn appcast_url() -> String {
    std::env::var("FUMI_APPCAST_URL").unwrap_or_else(|_| appcast::DEFAULT_APPCAST_URL.to_string())
}

/// Runs one check synchronously. Safe to call off the main thread: it does a
/// plain GET and updates shared state; callers notify the UI afterwards.
pub fn check_now() -> CheckResult {
    if !CHECKING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        return last_result().unwrap_or(CheckResult::UpToDate);
    }
    let result = perform_check();
    CHECKING.store(false, Ordering::SeqCst);
    set_result(result.clone());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    /// Serves one static appcast over HTTP and checks the whole pipeline:
    /// fetch → parse → compare → available. Confirms a real check discovers a
    /// newer version, not just the parsing unit tests.
    #[test]
    fn end_to_end_check_finds_a_newer_version() {
        let _guard = TEST_LOCK.lock().unwrap();
        let feed = r#"{"versions":[{
            "version":"9.9.9","build":9999,"date":"2026-11-15",
            "notes":"end to end","asset":{"url":"https://example.com/a.dmg","sha256":"ab"}
        }]}"#;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = feed.to_string();
        thread::spawn(move || {
            for _ in 0..2 {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        unsafe { std::env::set_var("FUMI_APPCAST_URL", format!("http://127.0.0.1:{port}/appcast.json")); }
        // Ensure this high version was never skipped by another test.
        update::set_skipped_version(None);
        let result = check_now();
        unsafe { std::env::remove_var("FUMI_APPCAST_URL"); }
        match result {
            CheckResult::Available(release) => assert_eq!(release.version.to_string(), "9.9.9"),
            other => panic!("expected an available update, got {other:?}"),
        }
    }

    #[test]
    fn offline_check_fails_quietly() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe { std::env::set_var("FUMI_APPCAST_URL", "http://127.0.0.1:1/appcast.json"); }
        let result = check_now();
        unsafe { std::env::remove_var("FUMI_APPCAST_URL"); }
        assert!(matches!(result, CheckResult::Failed(_)));
    }
}

fn perform_check() -> CheckResult {
    update::set_last_check_now();
    let url = appcast_url();
    let fetched = match net::fetch(&url) {
        Ok(f) => f,
        Err(e) => return CheckResult::Failed(e.to_string()),
    };
    let releases = match appcast::parse(&String::from_utf8_lossy(&fetched.bytes)) {
        Ok(r) => r,
        Err(e) => return CheckResult::Failed(e.to_string()),
    };
    let current = match Version::parse(&update::current_version()) {
        Some(v) => v,
        None => return CheckResult::Failed("couldn't read the running version".into()),
    };
    let build = update::current_build();
    let Some(found) = appcast::newest_after(&releases, &current, build) else {
        return CheckResult::UpToDate;
    };
    // A version the user explicitly skipped stays skipped until they ask again.
    if update::skipped_version().as_deref() == Some(found.version.to_string().as_str()) {
        return CheckResult::UpToDate;
    }
    CheckResult::Available(Box::new(found.clone()))
}
