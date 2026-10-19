//! Query layer: parquet metadata, filtered previews and exports.
//!
//! [`Engine`] reads parquet files directly from the bucket with DataFusion.
//! Column projection and filter pushdown mean only the needed byte ranges
//! (and row groups) are fetched over the network.

mod engine;
mod error;
mod export;
mod metadata;
mod plan;
mod preview;

pub use engine::Engine;
pub use error::{QueryError, Result};
pub use metadata::{ColumnInfo, MAML_KEY, TableInfo};
pub use preview::Preview;
