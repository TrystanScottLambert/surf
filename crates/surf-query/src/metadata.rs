//! Footer-only parquet inspection: schema, row counts and embedded metadata.

use std::ops::Range;
use std::sync::Arc;

use bytes::Bytes;
use datafusion::arrow::datatypes::{DataType, Schema};
use datafusion::parquet::arrow::async_reader::MetadataFetch;
use datafusion::parquet::arrow::parquet_to_arrow_schema;
use datafusion::parquet::errors::ParquetError;
use datafusion::parquet::file::metadata::ParquetMetaDataReader;
use futures::FutureExt;
use futures::future::BoxFuture;
use surf_core::ColumnKind;
use surf_storage::Bucket;
use surf_storage::object_store::path::Path as ObjectPath;
use surf_storage::object_store::{ObjectStore, ObjectStoreExt};

use crate::Result;

/// The key-value metadata entry that holds each table's markdown description.
pub const MAML_KEY: &str = "maml";

/// One column of a table.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnInfo {
    pub name: String,
    /// Human-readable Arrow type, e.g. `Float64`.
    pub data_type: String,
    pub kind: ColumnKind,
    pub nullable: bool,
}

/// Everything shown on a table's overview page.
#[derive(Debug, Clone, PartialEq)]
pub struct TableInfo {
    pub key: String,
    pub size_bytes: u64,
    pub num_rows: u64,
    pub num_row_groups: usize,
    pub columns: Vec<ColumnInfo>,
    /// Markdown stored under the `maml` key, if any.
    pub maml: Option<String>,
    /// All other key-value metadata (excluding the large Arrow schema blob).
    pub other_metadata: Vec<(String, String)>,
    pub created_by: Option<String>,
}

impl TableInfo {
    pub fn column(&self, name: &str) -> Option<&ColumnInfo> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// Read only the parquet footer of `key` (a few range requests).
pub async fn read_table_info(bucket: &Bucket, key: &str) -> Result<TableInfo> {
    let size_bytes = bucket.size(key).await?;
    let fetch = RangeFetch {
        store: bucket.object_store(),
        path: ObjectPath::from(key),
    };
    let meta = ParquetMetaDataReader::new()
        .load_and_finish(fetch, size_bytes)
        .await?;
    let file = meta.file_metadata();
    let schema = parquet_to_arrow_schema(file.schema_descr(), file.key_value_metadata())?;

    let mut maml = None;
    let mut other_metadata = Vec::new();
    for kv in file.key_value_metadata().into_iter().flatten() {
        let value = kv.value.clone().unwrap_or_default();
        if kv.key.eq_ignore_ascii_case(MAML_KEY) {
            maml = Some(value);
        } else if kv.key != "ARROW:schema" {
            other_metadata.push((kv.key.clone(), value));
        }
    }
    // Some writers put it on the Arrow schema instead of the file footer.
    if maml.is_none() {
        maml = schema
            .metadata()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(MAML_KEY))
            .map(|(_, v)| v.clone());
    }

    Ok(TableInfo {
        key: key.to_owned(),
        size_bytes,
        num_rows: u64::try_from(file.num_rows()).unwrap_or_default(),
        num_row_groups: meta.num_row_groups(),
        columns: columns_of(&schema),
        maml,
        other_metadata,
        created_by: file.created_by().map(str::to_owned),
    })
}

/// Byte-range reads against the bucket, for loading just the parquet footer.
struct RangeFetch {
    store: Arc<dyn ObjectStore>,
    path: ObjectPath,
}

impl MetadataFetch for RangeFetch {
    fn fetch(&mut self, range: Range<u64>) -> BoxFuture<'_, Result<Bytes, ParquetError>> {
        async move {
            self.store
                .get_range(&self.path, range)
                .await
                .map_err(|e| ParquetError::External(Box::new(e)))
        }
        .boxed()
    }
}

pub(crate) fn columns_of(schema: &Schema) -> Vec<ColumnInfo> {
    schema
        .fields()
        .iter()
        .map(|f| ColumnInfo {
            name: f.name().clone(),
            data_type: f.data_type().to_string(),
            kind: kind_of(f.data_type()),
            nullable: f.is_nullable(),
        })
        .collect()
}

/// Map an Arrow type onto the coarse kinds the filter builder understands.
pub fn kind_of(dt: &DataType) -> ColumnKind {
    use DataType as D;
    match dt {
        D::Int8 | D::Int16 | D::Int32 | D::Int64 | D::UInt8 | D::UInt16 | D::UInt32 | D::UInt64 => {
            ColumnKind::Integer
        }
        D::Float16
        | D::Float32
        | D::Float64
        | D::Decimal32(..)
        | D::Decimal64(..)
        | D::Decimal128(..)
        | D::Decimal256(..) => ColumnKind::Float,
        D::Utf8 | D::LargeUtf8 | D::Utf8View => ColumnKind::String,
        D::Boolean => ColumnKind::Boolean,
        D::Date32 | D::Date64 | D::Timestamp(..) | D::Time32(_) | D::Time64(_) => {
            ColumnKind::Temporal
        }
        D::Dictionary(_, value) => kind_of(value),
        _ => ColumnKind::Other,
    }
}
