//! Streaming object downloads with progress and safe cancellation.

use std::path::{Path, PathBuf};

use futures::StreamExt;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt};
use tokio::io::AsyncWriteExt;

use crate::Result;

/// Stream `key` into `dest`, calling `on_progress(done, total)` as bytes arrive.
///
/// Data is written to `<dest>.part` and renamed on success, so a failed or
/// cancelled (dropped) download never leaves a truncated file at `dest`.
pub async fn download_to(
    store: &dyn ObjectStore,
    key: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64, u64) + Send,
) -> Result<u64> {
    let result = store.get(&ObjectPath::from(key)).await?;
    let total = result.meta.size;
    let partial = PartFile::new(dest);
    let mut file = tokio::fs::File::create(&partial.path).await?;
    let mut stream = result.into_stream();
    let mut done = 0u64;
    on_progress(0, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        on_progress(done, total);
    }
    file.flush().await?;
    drop(file);
    partial.commit(dest)?;
    tracing::info!(key, dest = %dest.display(), bytes = done, "download complete");
    Ok(done)
}

/// A temporary `.part` file that is deleted on drop unless committed.
struct PartFile {
    path: PathBuf,
    committed: bool,
}

impl PartFile {
    fn new(dest: &Path) -> Self {
        let mut name = dest.file_name().unwrap_or_default().to_os_string();
        name.push(".part");
        PartFile {
            path: dest.with_file_name(name),
            committed: false,
        }
    }

    fn commit(mut self, dest: &Path) -> std::io::Result<()> {
        std::fs::rename(&self.path, dest)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for PartFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
