//! The static version feed the updater reads.
//!
//! There is no server: a plain JSON file is published on the project's GitHub
//! Pages site (and mirrored on the latest release). A check is a single GET of
//! that file with no identifiers attached — see `net::fetch`. The file lists
//! versions newest-first; each entry carries its release notes and the
//! downloadable asset's URL and SHA-256.

use crate::version::Version;

/// Default location of the version feed. Overridable with `FUMI_APPCAST_URL`
/// for testing and for forks.
pub const DEFAULT_APPCAST_URL: &str = "https://bubblebrainbb.github.io/fumi/appcast.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub url: String,
    /// Lowercase hex SHA-256 of the downloaded file.
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// CFBundleVersion of the build, used as a tie-breaker.
    pub build: u64,
    pub date: String,
    /// Markdown/plain release notes shown in the update dialog.
    pub notes: String,
    /// Apple silicon asset, which is all Fumi ships today.
    pub asset: Asset,
    /// Minimum macOS version, if the release needs a newer one.
    pub min_os: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedError {
    InvalidJson(String),
    MissingField(&'static str),
    BadVersion(String),
    Empty,
}

impl std::fmt::Display for FeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeedError::InvalidJson(s) => write!(f, "invalid update feed JSON: {s}"),
            FeedError::MissingField(k) => write!(f, "update feed is missing `{k}`"),
            FeedError::BadVersion(s) => write!(f, "invalid version in update feed: {s}"),
            FeedError::Empty => write!(f, "update feed has no releases"),
        }
    }
}

/// Parses a feed from JSON. Uses `serde_json::Value` directly so the shape is
/// tolerant of extra fields a future release might add.
pub fn parse(json: &str) -> Result<Vec<Release>, FeedError> {
    let root: serde_json::Value = serde_json::from_str(json).map_err(|e| FeedError::InvalidJson(e.to_string()))?;
    let versions = root.get("versions").and_then(|v| v.as_array()).ok_or(FeedError::MissingField("versions"))?;
    if versions.is_empty() {
        return Err(FeedError::Empty);
    }
    let mut out = Vec::with_capacity(versions.len());
    for entry in versions {
        let version_str = entry.get("version").and_then(|v| v.as_str()).ok_or(FeedError::MissingField("version"))?;
        let version = Version::parse(version_str).ok_or_else(|| FeedError::BadVersion(version_str.to_string()))?;
        let build = match entry.get("build") {
            Some(v) => v.as_u64().ok_or(FeedError::MissingField("build"))?,
            None => 0,
        };
        let str_field = |key: &'static str, default: &str| -> String {
            entry.get(key).and_then(|v| v.as_str()).unwrap_or(default).to_string()
        };
        let asset_obj = entry.get("asset").ok_or(FeedError::MissingField("asset"))?;
        let url = asset_obj.get("url").and_then(|v| v.as_str()).ok_or(FeedError::MissingField("asset.url"))?;
        let sha256 = asset_obj
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or(FeedError::MissingField("asset.sha256"))?;
        let min_os = entry.get("min_os").and_then(|v| v.as_str()).map(str::to_string);
        out.push(Release {
            version,
            build,
            date: str_field("date", ""),
            notes: str_field("notes", ""),
            asset: Asset { url: url.to_string(), sha256: sha256.to_lowercase() },
            min_os,
        });
    }
    Ok(out)
}

/// Finds the newest release strictly newer than the running version. When the
/// numeric version ties, a higher build number is treated as newer.
pub fn newest_after<'a>(releases: &'a [Release], current: &Version, current_build: u64) -> Option<&'a Release> {
    releases
        .iter()
        .filter(|r| r.version.is_newer_than(current) || (r.version == *current && r.build > current_build))
        .max_by(|a, b| a.version.cmp(&b.version).then(a.build.cmp(&b.build)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"{
        "versions": [
            {
                "version": "0.4.2",
                "build": 42,
                "date": "2026-11-15",
                "notes": "- Fixed the rename dialog\n- Faster search",
                "asset": {
                    "url": "https://github.com/BubbleBrainBB/fumi/releases/download/v0.4.2/Fumi-0.4.2-arm64.dmg",
                    "sha256": "ABCDEF0123456789"
                },
                "min_os": "14.0"
            },
            {
                "version": "0.4.1",
                "build": 41,
                "date": "2026-11-01",
                "notes": "Previous",
                "asset": { "url": "https://example.com/old.dmg", "sha256": "00" }
            }
        ]
    }"#;

    #[test]
    fn parses_a_feed() {
        let releases = parse(FEED).unwrap();
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].version.to_string(), "0.4.2");
        assert_eq!(releases[0].build, 42);
        assert_eq!(releases[0].notes, "- Fixed the rename dialog\n- Faster search");
        assert_eq!(releases[0].asset.sha256, "abcdef0123456789");
        assert_eq!(releases[0].min_os.as_deref(), Some("14.0"));
    }

    #[test]
    fn picks_the_newest_release() {
        let releases = parse(FEED).unwrap();
        let current = Version::parse("0.4.1").unwrap();
        let update = newest_after(&releases, &current, 41).unwrap();
        assert_eq!(update.version.to_string(), "0.4.2");
    }

    #[test]
    fn no_update_when_up_to_date() {
        let releases = parse(FEED).unwrap();
        let current = Version::parse("0.4.2").unwrap();
        assert!(newest_after(&releases, &current, 42).is_none());
        // A version beyond everything in the feed yields no update.
        let future = Version::parse("9.9.9").unwrap();
        assert!(newest_after(&releases, &future, 0).is_none());
    }

    #[test]
    fn reports_bad_feeds() {
        assert!(matches!(parse("not json"), Err(FeedError::InvalidJson(_))));
        assert!(matches!(parse(r#"{"versions":[]}"#), Err(FeedError::Empty)));
        assert!(matches!(parse(r#"{"versions":[{"version":"x"}]}"#), Err(FeedError::BadVersion(_))));
    }
}
