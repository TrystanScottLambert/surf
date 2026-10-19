//! Read-only access to the bucket.
//!
//! [`Bucket`] wraps any [`ObjectStore`], so production code talks to R2 while
//! tests use an in-memory store. The query layer reuses the same store through
//! [`Bucket::object_store`].

mod download;
mod r2;

use std::sync::Arc;

use futures::TryStreamExt;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt};
use surf_core::{Entry, Listing};

pub use download::download_to;
pub use object_store;
pub use r2::R2Config;

/// Errors from the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("could not reach the bucket: {0}")]
    Store(#[from] object_store::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("`{key}` is larger than {limit} bytes and can't be displayed")]
    TooLarge { key: String, limit: u64 },
    #[error("`{0}` is not valid UTF-8 text")]
    NotText(String),
}

pub type Result<T, E = StorageError> = std::result::Result<T, E>;

/// Largest text document we will load for display.
pub const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;

/// A read-only handle on a bucket.
#[derive(Clone, Debug)]
pub struct Bucket {
    store: Arc<dyn ObjectStore>,
    name: String,
    url: String,
}

impl Bucket {
    /// Connect to a Cloudflare R2 bucket.
    pub fn r2(config: &R2Config) -> Result<Self> {
        let store = r2::build(config)?;
        Ok(Bucket {
            store,
            name: config.bucket.clone(),
            url: format!("s3://{}", config.bucket),
        })
    }

    /// Wrap an arbitrary store (used by tests). `url` must be a URL the query
    /// engine can register the store under, e.g. `memory://`.
    pub fn from_store(
        store: Arc<dyn ObjectStore>,
        name: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Bucket {
            store,
            name: name.into(),
            url: url.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Base URL under which the query engine should register [`Self::object_store`].
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn object_store(&self) -> Arc<dyn ObjectStore> {
        self.store.clone()
    }

    /// List the immediate children of a folder. Use `""` for the bucket root.
    pub async fn list_dir(&self, prefix: &str) -> Result<Listing> {
        let path = (!prefix.is_empty()).then(|| ObjectPath::from(prefix));
        let result = self.store.list_with_delimiter(path.as_ref()).await?;
        let own = prefix.trim_end_matches('/');
        let mut entries: Vec<Entry> = result
            .common_prefixes
            .iter()
            .map(|p| Entry::folder(p.as_ref()))
            .collect();
        entries.extend(
            result
                .objects
                .iter()
                // Skip zero-byte "folder marker" objects some tools create.
                .filter(|o| o.location.as_ref() != own && !o.location.as_ref().ends_with('/'))
                .map(|o| Entry::file(o.location.as_ref(), o.size)),
        );
        tracing::debug!(prefix, count = entries.len(), "listed folder");
        Ok(Listing::new(prefix, entries))
    }

    /// Size in bytes of an object.
    pub async fn size(&self, key: &str) -> Result<u64> {
        Ok(self.store.head(&ObjectPath::from(key)).await?.size)
    }

    /// Load a (small) text object such as a markdown description.
    pub async fn read_text(&self, key: &str) -> Result<String> {
        let path = ObjectPath::from(key);
        let size = self.store.head(&path).await?.size;
        if size > MAX_TEXT_BYTES {
            return Err(StorageError::TooLarge {
                key: key.to_owned(),
                limit: MAX_TEXT_BYTES,
            });
        }
        let bytes = self.store.get(&path).await?.bytes().await?;
        String::from_utf8(bytes.to_vec()).map_err(|_| StorageError::NotText(key.to_owned()))
    }

    /// Stream an object to a local file. See [`download_to`].
    pub async fn download(
        &self,
        key: &str,
        dest: &std::path::Path,
        on_progress: impl FnMut(u64, u64) + Send,
    ) -> Result<u64> {
        download_to(self.store.as_ref(), key, dest, on_progress).await
    }

    /// Recursively count everything under a prefix (diagnostics only).
    pub async fn count_objects(&self, prefix: &str) -> Result<usize> {
        let path = (!prefix.is_empty()).then(|| ObjectPath::from(prefix));
        Ok(self
            .store
            .list(path.as_ref())
            .try_collect::<Vec<_>>()
            .await?
            .len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::PutPayload;
    use object_store::memory::InMemory;
    use surf_core::EntryKind;

    async fn fixture() -> Bucket {
        let store = Arc::new(InMemory::new());
        for (key, body) in [
            ("groups/v1/groups.md", "# Groups\nhello"),
            ("groups/v1/groups.parquet", "PAR1"),
            ("groups/v1/galaxies.parquet", "PAR1"),
            ("groups/v2/groups.md", "# v2"),
            ("groups/v10/groups.md", "# v10"),
            ("stars/v1/stars.md", "# Stars"),
        ] {
            store
                .put(&ObjectPath::from(key), PutPayload::from(body))
                .await
                .unwrap();
        }
        Bucket::from_store(store, "test", "memory://")
    }

    #[tokio::test]
    async fn lists_root_and_versions() {
        let b = fixture().await;
        let root = b.list_dir("").await.unwrap();
        let names: Vec<_> = root.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["groups", "stars"]);

        let versions = b.list_dir("groups/").await.unwrap();
        let names: Vec<_> = versions.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["v1", "v2", "v10"]);
        assert!(matches!(
            versions.entries[0].kind,
            EntryKind::VersionFolder(_)
        ));

        let v1 = b.list_dir("groups/v1/").await.unwrap();
        assert_eq!(v1.entries.len(), 3);
        assert_eq!(v1.readme().unwrap().key, "groups/v1/groups.md");
    }

    #[tokio::test]
    async fn reads_text_and_downloads() {
        let b = fixture().await;
        assert_eq!(
            b.read_text("groups/v1/groups.md").await.unwrap(),
            "# Groups\nhello"
        );
        assert!(b.read_text("missing.md").await.is_err());

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.parquet");
        let mut last = (0, 0);
        let n = b
            .download("groups/v1/groups.parquet", &dest, |done, total| {
                last = (done, total)
            })
            .await
            .unwrap();
        assert_eq!(n, 4);
        assert_eq!(last, (4, 4));
        assert_eq!(std::fs::read(&dest).unwrap(), b"PAR1");
        assert_eq!(b.count_objects("groups/").await.unwrap(), 5);
    }
}
