//! Writing a selection to a local Parquet or CSV file.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use datafusion::arrow::csv::WriterBuilder as CsvWriterBuilder;
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::basic::{Compression, ZstdLevel};
use datafusion::parquet::file::metadata::KeyValue;
use datafusion::parquet::file::properties::WriterProperties;
use datafusion::prelude::DataFrame;
use futures::StreamExt;
use surf_core::ExportFormat;

use crate::Result;
use crate::metadata::MAML_KEY;

/// Stream `df` into `dest`, reporting rows written. Returns the row count.
///
/// Output goes to `<dest>.part` and is renamed on success; a failed or
/// dropped export leaves nothing behind.
pub async fn write(
    df: DataFrame,
    format: ExportFormat,
    dest: &Path,
    maml: Option<String>,
    mut on_progress: impl FnMut(u64) + Send,
) -> Result<u64> {
    let schema = df.schema().inner().clone();
    let mut stream = df.execute_stream().await?;
    let part = PartFile::new(dest);
    let file = BufWriter::new(File::create(&part.path)?);
    let mut rows = 0u64;
    on_progress(0);

    match format {
        ExportFormat::Parquet => {
            // Keep the table's description with the subset.
            let kv = maml.map(|m| vec![KeyValue::new(MAML_KEY.to_owned(), m)]);
            let props = WriterProperties::builder()
                .set_compression(Compression::ZSTD(ZstdLevel::default()))
                .set_key_value_metadata(kv)
                .build();
            let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
            while let Some(batch) = stream.next().await {
                let batch = batch?;
                writer.write(&batch)?;
                rows += batch.num_rows() as u64;
                on_progress(rows);
            }
            writer.close()?;
        }
        ExportFormat::Csv => {
            let mut writer = CsvWriterBuilder::new().with_header(true).build(file);
            while let Some(batch) = stream.next().await {
                let batch = batch?;
                writer.write(&batch)?;
                rows += batch.num_rows() as u64;
                on_progress(rows);
            }
            writer
                .into_inner()
                .into_inner()
                .map_err(|e| e.into_error())?;
        }
    }
    part.commit(dest)?;
    tracing::info!(dest = %dest.display(), rows, ?format, "export complete");
    Ok(rows)
}

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
