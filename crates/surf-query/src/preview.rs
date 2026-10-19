//! Rendering record batches as plain strings for display.

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::util::display::{ArrayFormatter, FormatOptions};

use crate::Result;

/// A small sample of rows plus the total number that match.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Number of rows matching the selection's filters (whole table).
    pub matching_rows: u64,
}

pub(crate) fn to_rows(batches: &[RecordBatch]) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let Some(first) = batches.first() else {
        return Ok((Vec::new(), Vec::new()));
    };
    let columns = first
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    let options = FormatOptions::default().with_null("—");
    let mut rows = Vec::new();
    for batch in batches {
        let formatters = batch
            .columns()
            .iter()
            .map(|c| ArrayFormatter::try_new(c.as_ref(), &options))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for r in 0..batch.num_rows() {
            rows.push(formatters.iter().map(|f| f.value(r).to_string()).collect());
        }
    }
    Ok((columns, rows))
}
