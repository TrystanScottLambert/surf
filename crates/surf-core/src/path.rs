//! Helpers for working with `/`-delimited object keys.

use std::fmt;
use std::str::FromStr;

/// A dataset version such as `v3`, parsed from a folder name.
///
/// Ordering is numeric, so `v2 < v10`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32);

impl FromStr for Version {
    type Err = ();

    /// Accepts `v1`, `V1` and `v1/`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim_end_matches('/');
        let digits = s.strip_prefix(['v', 'V']).ok_or(())?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(());
        }
        digits.parse().map(Version).map_err(|_| ())
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// The last path segment of a key or prefix, ignoring a trailing `/`.
///
/// `"groups/v1/"` -> `"v1"`, `"groups/v1/a.parquet"` -> `"a.parquet"`.
pub fn last_segment(key: &str) -> &str {
    let trimmed = key.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

/// The name of the folder that contains `prefix`.
///
/// `"groups/v1/"` -> `Some("groups")`, `"v1/"` -> `None`.
pub fn parent_name(prefix: &str) -> Option<&str> {
    let trimmed = prefix.trim_end_matches('/');
    let (parent, _) = trimmed.rsplit_once('/')?;
    Some(last_segment(parent))
}

/// The file name without its final extension. `"galaxies.parquet"` -> `"galaxies"`.
pub fn file_stem(key: &str) -> &str {
    let name = last_segment(key);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// Lower-cased extension of the last segment, if any.
pub fn extension(key: &str) -> Option<String> {
    let name = last_segment(key);
    name.rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, ext)| ext.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!("v1".parse(), Ok(Version(1)));
        assert_eq!("V12/".parse(), Ok(Version(12)));
        assert!("v".parse::<Version>().is_err());
        assert!("version1".parse::<Version>().is_err());
        assert!("v1a".parse::<Version>().is_err());
        assert!("groups".parse::<Version>().is_err());
    }

    #[test]
    fn versions_sort_numerically() {
        let mut v: Vec<Version> = ["v10", "v2", "v1"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        v.sort();
        assert_eq!(v, vec![Version(1), Version(2), Version(10)]);
    }

    #[test]
    fn segments() {
        assert_eq!(last_segment("groups/v1/"), "v1");
        assert_eq!(last_segment("groups/v1/a.parquet"), "a.parquet");
        assert_eq!(last_segment("top"), "top");
        assert_eq!(parent_name("groups/v1/"), Some("groups"));
        assert_eq!(parent_name("a/b/groups/v1"), Some("groups"));
        assert_eq!(parent_name("v1/"), None);
        assert_eq!(file_stem("x/galaxies.parquet"), "galaxies");
        assert_eq!(file_stem(".hidden"), ".hidden");
        assert_eq!(extension("x/A.PARQUET").as_deref(), Some("parquet"));
        assert_eq!(extension("x/README"), None);
    }
}
