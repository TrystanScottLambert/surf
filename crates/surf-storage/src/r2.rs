//! Cloudflare R2 connection details.

use std::fmt;
use std::sync::Arc;

use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;

/// Everything needed to talk to an R2 bucket.
#[derive(Clone)]
pub struct R2Config {
    /// S3-compatible endpoint, e.g. `https://<account>.r2.cloudflarestorage.com`.
    pub endpoint: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
}

impl R2Config {
    /// The endpoint for a Cloudflare account ID.
    pub fn endpoint_for_account(account_id: &str) -> String {
        format!("https://{account_id}.r2.cloudflarestorage.com")
    }
}

impl fmt::Debug for R2Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("R2Config")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .finish_non_exhaustive()
    }
}

pub(crate) fn build(config: &R2Config) -> Result<Arc<dyn ObjectStore>, object_store::Error> {
    let store = AmazonS3Builder::new()
        .with_endpoint(&config.endpoint)
        .with_region("auto")
        .with_bucket_name(&config.bucket)
        .with_access_key_id(&config.access_key_id)
        .with_secret_access_key(&config.secret_access_key)
        .build()?;
    Ok(Arc::new(store))
}
