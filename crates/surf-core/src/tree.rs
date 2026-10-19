//! Classification of bucket listings into things the UI understands.

use crate::path::{Version, extension, last_segment, parent_name};

/// What a listed entry represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// An ordinary folder (e.g. a dataset such as `groups/`).
    Folder,
    /// A version folder such as `v2/`.
    VersionFolder(Version),
    /// A parquet table.
    Parquet,
    /// A markdown document.
    Markdown,
    /// Anything else.
    Other,
}

impl EntryKind {
    pub fn is_folder(self) -> bool {
        matches!(self, EntryKind::Folder | EntryKind::VersionFolder(_))
    }
}

/// A single item inside a folder listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Full key for files, or full prefix (ending in `/`) for folders.
    pub key: String,
    /// Display name (last path segment).
    pub name: String,
    pub kind: EntryKind,
    /// Size in bytes (files only).
    pub size: Option<u64>,
}

impl Entry {
    /// Build a folder entry from a common prefix.
    pub fn folder(prefix: impl Into<String>) -> Self {
        let mut key = prefix.into();
        if !key.ends_with('/') {
            key.push('/');
        }
        let name = last_segment(&key).to_owned();
        let kind = match name.parse::<Version>() {
            Ok(v) => EntryKind::VersionFolder(v),
            Err(()) => EntryKind::Folder,
        };
        Entry {
            key,
            name,
            kind,
            size: None,
        }
    }

    /// Build a file entry from an object key.
    pub fn file(key: impl Into<String>, size: u64) -> Self {
        let key = key.into();
        let name = last_segment(&key).to_owned();
        let kind = match extension(&key).as_deref() {
            Some("parquet") => EntryKind::Parquet,
            Some("md" | "markdown") => EntryKind::Markdown,
            _ => EntryKind::Other,
        };
        Entry {
            key,
            name,
            kind,
            size: Some(size),
        }
    }
}

/// The contents of one folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub prefix: String,
    pub entries: Vec<Entry>,
}

impl Listing {
    /// Create a listing with entries in display order: folders first
    /// (versions numerically), then files alphabetically.
    pub fn new(prefix: impl Into<String>, mut entries: Vec<Entry>) -> Self {
        entries.sort_by_key(sort_key);
        Listing {
            prefix: prefix.into(),
            entries,
        }
    }

    /// The dataset description for a version folder, if present in this listing.
    pub fn readme(&self) -> Option<&Entry> {
        let expected = readme_key_for(&self.prefix);
        self.entries
            .iter()
            .find(|e| Some(&e.key) == expected.as_ref())
            .or_else(|| self.entries.iter().find(|e| e.kind == EntryKind::Markdown))
    }
}

fn sort_key(e: &Entry) -> (u8, u32, String) {
    match e.kind {
        EntryKind::VersionFolder(v) => (0, v.0, String::new()),
        EntryKind::Folder => (1, 0, e.name.to_lowercase()),
        EntryKind::Markdown => (2, 0, e.name.to_lowercase()),
        EntryKind::Parquet => (3, 0, e.name.to_lowercase()),
        EntryKind::Other => (4, 0, e.name.to_lowercase()),
    }
}

/// The conventional description key for a version folder:
/// `groups/v1/` -> `groups/v1/groups.md`.
pub fn readme_key_for(version_prefix: &str) -> Option<String> {
    let name = parent_name(version_prefix)?;
    let base = version_prefix.trim_end_matches('/');
    Some(format!("{base}/{name}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_entries() {
        assert_eq!(
            Entry::folder("groups/v3").kind,
            EntryKind::VersionFolder(Version(3))
        );
        assert_eq!(Entry::folder("groups/v3").key, "groups/v3/");
        assert_eq!(Entry::folder("groups/").kind, EntryKind::Folder);
        assert_eq!(Entry::file("g/v1/a.parquet", 1).kind, EntryKind::Parquet);
        assert_eq!(Entry::file("g/v1/g.md", 1).kind, EntryKind::Markdown);
        assert_eq!(Entry::file("g/v1/x.txt", 1).kind, EntryKind::Other);
    }

    #[test]
    fn orders_listing() {
        let l = Listing::new(
            "groups/",
            vec![
                Entry::file("groups/z.parquet", 1),
                Entry::folder("groups/v10/"),
                Entry::file("groups/a.md", 1),
                Entry::folder("groups/v2/"),
            ],
        );
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["v2", "v10", "a.md", "z.parquet"]);
    }

    #[test]
    fn finds_readme() {
        assert_eq!(
            readme_key_for("groups/v1/").as_deref(),
            Some("groups/v1/groups.md")
        );
        assert_eq!(readme_key_for("v1/"), None);
        let l = Listing::new(
            "groups/v1/",
            vec![
                Entry::file("groups/v1/notes.md", 1),
                Entry::file("groups/v1/groups.md", 1),
                Entry::file("groups/v1/groups.parquet", 1),
            ],
        );
        assert_eq!(l.readme().unwrap().name, "groups.md");
        let l = Listing::new("groups/v1/", vec![Entry::file("groups/v1/other.md", 1)]);
        assert_eq!(l.readme().unwrap().name, "other.md");
    }
}
