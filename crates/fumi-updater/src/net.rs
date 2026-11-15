//! Network I/O, deliberately small and privacy-preserving.
//!
//! A check is one plain GET of the static feed — no identifiers, no profile
//! data, no analytics. Downloads use the same `/usr/bin/curl` the media cache
//! uses, with strict protocol allow-listing, timeouts and a temp-then-rename
//! write so a partial file is never mistaken for a complete one.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Result of fetching a URL: the HTTP status and the bytes on success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    pub status: u16,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetError {
    Spawn(String),
    Status(u16),
    Io(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Spawn(s) => write!(f, "couldn't start the network helper: {s}"),
            NetError::Status(code) => write!(f, "update server returned HTTP {code}"),
            NetError::Io(s) => write!(f, "network error: {s}"),
        }
    }
}

/// Fetch a small payload (the feed) into memory. Uses `--max-time` and a
/// download-size cap so a slow or hostile endpoint can't hang or flood us.
pub fn fetch(url: &str) -> Result<Fetched, NetError> {
    let tmp = std::env::temp_dir().join(format!("fumi-feed-{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let output = Command::new("/usr/bin/curl")
        .args([
            "--silent",
            "--location",
            "--globoff",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--max-filesize",
            "1048576", // 1 MiB: a feed is a few KB
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--write-out",
            "%{http_code}",
            "--output",
        ])
        .arg(&tmp)
        .arg(url)
        .output()
        .map_err(|e| NetError::Spawn(e.to_string()))?;
    let status = String::from_utf8_lossy(&output.stdout).trim().parse::<u16>().unwrap_or(0);
    if !(200..300).contains(&status) {
        let _ = std::fs::remove_file(&tmp);
        return Err(NetError::Status(status));
    }
    let bytes = std::fs::read(&tmp).map_err(|e| NetError::Io(e.to_string()))?;
    let _ = std::fs::remove_file(&tmp);
    Ok(Fetched { status, bytes })
}

/// Downloads an asset to `dest`, returning the final path. Writes to a
/// sibling `.part` first and renames on success.
pub fn download(url: &str, dest: &Path) -> Result<PathBuf, NetError> {
    let part = PathBuf::from(format!("{}.part", dest.to_string_lossy()));
    let _ = std::fs::remove_file(&part);
    let output = Command::new("/usr/bin/curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--globoff",
            "--fail",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--connect-timeout",
            "15",
            "--speed-time",
            "30",
            "--speed-limit",
            "1024",
            "--max-time",
            "900",
            "--write-out",
            "%{http_code}",
            "--output",
        ])
        .arg(&part)
        .arg(url)
        .output()
        .map_err(|e| NetError::Spawn(e.to_string()))?;
    let status = String::from_utf8_lossy(&output.stdout).trim().parse::<u16>().unwrap_or(0);
    if !(200..300).contains(&status) {
        let _ = std::fs::remove_file(&part);
        return Err(NetError::Status(status));
    }
    std::fs::rename(&part, dest).map_err(|e| NetError::Io(e.to_string()))?;
    Ok(dest.to_path_buf())
}

/// A short timeout used by callers that poll a download's progress.
pub const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Computes the lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Computes the SHA-256 of a file on disk.
pub fn sha256_file(path: &Path) -> Result<String, NetError> {
    let bytes = std::fs::read(path).map_err(|e| NetError::Io(e.to_string()))?;
    Ok(sha256_hex(&bytes))
}

/// Compares a computed digest against the expected one, case-insensitively.
pub fn digest_matches(actual: &str, expected: &str) -> bool {
    actual.trim().eq_ignore_ascii_case(expected.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sha256_vectors() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn digest_comparison_ignores_case_and_space() {
        assert!(digest_matches(
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD",
            " ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad "
        ));
        assert!(!digest_matches("abc", "def"));
    }
}
