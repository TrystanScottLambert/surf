use surf_core::FilterError;

/// Errors from the query layer, phrased for end users where possible.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error(transparent)]
    Filter(#[from] FilterError),
    #[error("the advanced filter is not valid: {0}")]
    AdvancedFilter(String),
    #[error("unknown column `{0}`")]
    UnknownColumn(String),
    #[error("query failed: {0}")]
    Engine(#[from] datafusion::error::DataFusionError),
    #[error("could not read parquet file: {0}")]
    Parquet(#[from] datafusion::parquet::errors::ParquetError),
    #[error("could not write output: {0}")]
    Arrow(#[from] datafusion::arrow::error::ArrowError),
    #[error(transparent)]
    Storage(#[from] surf_storage::StorageError),
    #[error("could not reach the bucket: {0}")]
    Store(#[from] surf_storage::object_store::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = QueryError> = std::result::Result<T, E>;
