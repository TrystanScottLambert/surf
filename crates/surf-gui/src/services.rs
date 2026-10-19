//! Construction of the backend services from the embedded credentials.

use std::path::PathBuf;

use surf_query::Engine;
use surf_storage::{Bucket, R2Config};

/// Handles to everything the UI talks to. Cheap to clone into tasks.
#[derive(Clone, Debug)]
pub struct Services {
    pub engine: Engine,
}

impl Services {
    /// Connect to R2 with the embedded credentials, or — when the
    /// `SURF_LOCAL_DIR` environment variable is set — to a local folder laid
    /// out like the bucket (useful for development and testing).
    pub fn connect() -> Result<Self, String> {
        if let Some(dir) = std::env::var_os("SURF_LOCAL_DIR") {
            tracing::info!(dir = ?dir, "using local folder instead of R2");
            let bucket = Bucket::local(std::path::Path::new(&dir)).map_err(|e| e.to_string())?;
            let engine = Engine::new(bucket).map_err(|e| e.to_string())?;
            return Ok(Services { engine });
        }
        let creds = surf_credentials::embedded();
        let config = R2Config {
            endpoint: creds.endpoint,
            access_key_id: creds.access_key_id,
            secret_access_key: creds.secret_access_key,
            bucket: creds.bucket,
        };
        let bucket = Bucket::r2(&config).map_err(|e| e.to_string())?;
        let engine = Engine::new(bucket).map_err(|e| e.to_string())?;
        Ok(Services { engine })
    }

    pub fn bucket(&self) -> &Bucket {
        self.engine.bucket()
    }
}

fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("org", "WAVES", "surf")
}

/// Where log files are written.
pub fn log_dir() -> Option<PathBuf> {
    let dir = project_dirs()?.data_local_dir().join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}
