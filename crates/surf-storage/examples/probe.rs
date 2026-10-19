//! Connectivity check: lists the bucket using the embedded credentials.
//!
//! ```sh
//! cargo run -p surf-storage --example probe            # bucket root
//! cargo run -p surf-storage --example probe groups/v1/ # a folder
//! ```

use surf_storage::{Bucket, R2Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let creds = surf_credentials::embedded();
    let config = R2Config {
        endpoint: creds.endpoint,
        access_key_id: creds.access_key_id,
        secret_access_key: creds.secret_access_key,
        bucket: creds.bucket,
    };
    let prefix = std::env::args().nth(1).unwrap_or_default();
    let bucket = Bucket::r2(&config)?;
    println!("Listing {}/{prefix}", config.endpoint);
    let listing = bucket.list_dir(&prefix).await?;
    for e in &listing.entries {
        match e.size {
            Some(size) => println!("  {:<40} {:>12} bytes  ({:?})", e.name, size, e.kind),
            None => println!("  {}/  ({:?})", e.name, e.kind),
        }
    }
    if let Some(readme) = listing.readme() {
        println!("\nDescription file: {}", readme.key);
    }
    Ok(())
}
