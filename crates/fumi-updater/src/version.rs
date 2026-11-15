//! Semantic version parsing and comparison for the updater.
//!
//! Fumi versions are `major.minor.patch` (e.g. `0.4.1`), optionally with a
//! leading `v` and/or a pre-release suffix (`0.5.0-beta.1`). The comparison is
//! deliberately strict and dependency-free so it can be unit-tested and used
//! to decide "is the published version newer than the running one" without
//! pulling in a semver crate.

use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// `None` is a stable release and sorts above any pre-release of the same
    /// numeric triple. `Some("beta.1")` sorts below the stable release.
    pub pre: Option<String>,
}

impl Version {
    /// Parses a version string. Returns `None` when the numeric core is absent
    /// or malformed. A leading `v`, a pre-release (`-…`) and build metadata
    /// (`+…`) are accepted; build metadata is ignored for ordering.
    pub fn parse(input: &str) -> Option<Version> {
        let s = input.trim();
        let s = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
        // Build metadata (`+…`) does not take part in ordering.
        let core_and_pre = s.split('+').next()?;
        if core_and_pre.is_empty() {
            return None;
        }
        let (core, pre) = match core_and_pre.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_string())),
            None => (core_and_pre, None),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next()?.parse::<u64>().ok()?;
        let patch = parts.next()?.parse::<u64>().ok()?;
        if parts.next().is_some() {
            return None; // more than three numeric components
        }
        Some(Version { major, minor, patch, pre })
    }

    /// True when `self` is strictly newer than `other`.
    pub fn is_newer_than(&self, other: &Version) -> bool {
        self.cmp(other) == Ordering::Greater
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                // A stable release is newer than its pre-releases.
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => compare_pre(a, b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compares pre-release identifiers the way semver does: dot-separated,
/// numeric identifiers compare numerically and sort below non-numeric ones.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut ai = a.split('.');
    let mut bi = b.split('.');
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(n), Ok(m)) => n.cmp(&m),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_versions() {
        let v = Version::parse("0.4.1").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (0, 4, 1));
        assert_eq!(v.pre, None);
        assert_eq!(v.to_string(), "0.4.1");
    }

    #[test]
    fn accepts_v_prefix_and_metadata() {
        let v = Version::parse("v1.2.3-beta.2+build.9").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.pre.as_deref(), Some("beta.2"));
        assert_eq!(v.to_string(), "1.2.3-beta.2");
    }

    #[test]
    fn rejects_malformed_versions() {
        for s in ["", "v", "1", "1.2", "1.2.3.4", "a.b.c", "1.2.x"] {
            assert!(Version::parse(s).is_none(), "should reject {s:?}");
        }
    }

    #[test]
    fn compares_numeric_triples() {
        assert!(Version::parse("0.4.2").unwrap().is_newer_than(&Version::parse("0.4.1").unwrap()));
        assert!(Version::parse("1.0.0").unwrap().is_newer_than(&Version::parse("0.9.9").unwrap()));
        assert!(!Version::parse("0.4.1").unwrap().is_newer_than(&Version::parse("0.4.1").unwrap()));
        assert!(!Version::parse("0.4.0").unwrap().is_newer_than(&Version::parse("0.4.1").unwrap()));
    }

    #[test]
    fn stable_is_newer_than_its_prerelease() {
        let stable = Version::parse("0.5.0").unwrap();
        let beta = Version::parse("0.5.0-beta.1").unwrap();
        assert!(stable.is_newer_than(&beta));
        assert!(!beta.is_newer_than(&stable));
    }

    #[test]
    fn prerelease_identifiers_compare_numerically() {
        let b2 = Version::parse("0.5.0-beta.2").unwrap();
        let b10 = Version::parse("0.5.0-beta.10").unwrap();
        assert!(b10.is_newer_than(&b2));
    }
}
