//! The query engine facade used by the GUI.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use datafusion::execution::object_store::ObjectStoreUrl;
use datafusion::prelude::{DataFrame, ParquetReadOptions, SessionConfig, SessionContext};
use surf_core::{ExportFormat, Selection};
use surf_storage::Bucket;

use crate::metadata::{TableInfo, read_table_info};
use crate::preview::{Preview, to_rows};
use crate::{Result, export, plan};

/// Reads parquet tables from a [`Bucket`]. Cheap to clone.
#[derive(Clone)]
pub struct Engine {
    ctx: SessionContext,
    bucket: Bucket,
    tables: Arc<Mutex<HashMap<String, DataFrame>>>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("bucket", &self.bucket.name())
            .finish_non_exhaustive()
    }
}

impl Engine {
    pub fn new(bucket: Bucket) -> Result<Self> {
        let config = SessionConfig::new()
            // Prune pages/rows as well as row groups when filters allow it.
            .set_bool("datafusion.execution.parquet.pushdown_filters", true)
            .set_bool("datafusion.execution.parquet.reorder_filters", true);
        let ctx = SessionContext::new_with_config(config);
        let url = ObjectStoreUrl::parse(bucket.url())?;
        ctx.register_object_store(url.as_ref(), bucket.object_store());
        Ok(Engine {
            ctx,
            bucket,
            tables: Arc::default(),
        })
    }

    pub fn bucket(&self) -> &Bucket {
        &self.bucket
    }

    /// Schema, sizes and embedded metadata, read from the footer only.
    pub async fn table_info(&self, key: &str) -> Result<TableInfo> {
        read_table_info(&self.bucket, key).await
    }

    /// Validate a selection without running it (fast, no data read).
    pub async fn validate(&self, key: &str, selection: &Selection) -> Result<()> {
        plan::apply(self.table(key).await?, selection).map(drop)
    }

    /// First `limit` matching rows, plus the total number of matching rows.
    pub async fn preview(&self, key: &str, selection: &Selection, limit: usize) -> Result<Preview> {
        let (sample, count) = futures::try_join!(
            self.sample(key, selection, limit),
            self.count(key, selection)
        )?;
        Ok(Preview {
            matching_rows: count,
            ..sample
        })
    }

    /// The first `limit` matching rows. Stops reading as soon as it has them,
    /// so it is quick even on very large tables. `matching_rows` is left at 0.
    pub async fn sample(&self, key: &str, selection: &Selection, limit: usize) -> Result<Preview> {
        let df = plan::apply(self.table(key).await?, selection)?;
        let batches = df.limit(0, Some(limit))?.collect().await?;
        let (mut columns, rows) = to_rows(&batches)?;
        if columns.is_empty() {
            columns = self.selected_columns(key, selection).await?;
        }
        Ok(Preview {
            columns,
            rows,
            matching_rows: 0,
        })
    }

    /// Number of rows matching the selection. Without filters this comes from
    /// the parquet statistics; with filters it scans the filtered columns.
    pub async fn count(&self, key: &str, selection: &Selection) -> Result<u64> {
        let df = plan::apply(self.table(key).await?, selection)?;
        Ok(df.count().await? as u64)
    }

    /// Write the selection to a local file, reporting rows written.
    pub async fn export(
        &self,
        key: &str,
        selection: &Selection,
        format: ExportFormat,
        dest: &Path,
        on_progress: impl FnMut(u64) + Send,
    ) -> Result<u64> {
        let df = plan::apply(self.table(key).await?, selection)?;
        let maml = if format == ExportFormat::Parquet {
            self.table_info(key).await.ok().and_then(|i| i.maml)
        } else {
            None
        };
        export::write(df, format, dest, maml, on_progress).await
    }

    async fn selected_columns(&self, key: &str, selection: &Selection) -> Result<Vec<String>> {
        let df = plan::apply(self.table(key).await?, selection)?;
        Ok(df
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect())
    }

    /// A cached DataFrame over the whole table.
    async fn table(&self, key: &str) -> Result<DataFrame> {
        if let Some(df) = self.tables.lock().expect("cache lock").get(key) {
            return Ok(df.clone());
        }
        let url = format!("{}/{}", self.bucket.url().trim_end_matches('/'), key);
        let df = self
            .ctx
            .read_parquet(url, ParquetReadOptions::default())
            .await?;
        self.tables
            .lock()
            .expect("cache lock")
            .insert(key.to_owned(), df.clone());
        Ok(df)
    }
}
