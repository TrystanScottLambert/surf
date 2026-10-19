//! What subset of a table the user wants.

use std::fmt;

use crate::filter::FilterSet;
use crate::path::{Version, file_stem, last_segment, parent_name};

/// A user's choice of columns and rows from one table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Selection {
    /// Columns to keep, in order. Empty means all columns.
    pub columns: Vec<String>,
    /// Filters built with the visual builder.
    pub filters: FilterSet,
    /// Optional advanced SQL `WHERE` expression, ANDed with `filters`.
    pub advanced_where: String,
}

impl Selection {
    /// True when the selection is the whole table (all columns, no filters).
    pub fn is_everything(&self) -> bool {
        self.columns.is_empty() && self.filters.is_empty() && self.advanced_where.trim().is_empty()
    }
}

/// Output file format for downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Parquet,
    Csv,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 2] = [ExportFormat::Parquet, ExportFormat::Csv];

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Parquet => "parquet",
            ExportFormat::Csv => "csv",
        }
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ExportFormat::Parquet => "Parquet",
            ExportFormat::Csv => "CSV",
        })
    }
}

/// A friendly default file name for a download of the table at `key`:
/// `groups/v1/galaxies.parquet` -> `groups_v1_galaxies[_subset].csv`.
pub fn suggested_file_name(key: &str, subset: bool, format: ExportFormat) -> String {
    let stem = file_stem(key);
    let folder = key.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    let mut parts = Vec::new();
    if last_segment(folder).parse::<Version>().is_ok() {
        if let Some(dataset) = parent_name(folder) {
            parts.push(dataset);
        }
        parts.push(last_segment(folder));
    }
    parts.push(stem);
    let mut name = parts.join("_");
    if subset {
        name.push_str("_subset");
    }
    format!("{name}.{}", format.extension())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_names() {
        assert_eq!(
            suggested_file_name("groups/v1/galaxies.parquet", false, ExportFormat::Parquet),
            "groups_v1_galaxies.parquet"
        );
        assert_eq!(
            suggested_file_name("groups/v1/galaxies.parquet", true, ExportFormat::Csv),
            "groups_v1_galaxies_subset.csv"
        );
        assert_eq!(
            suggested_file_name("loose.parquet", false, ExportFormat::Csv),
            "loose.csv"
        );
    }

    #[test]
    fn everything() {
        let mut s = Selection::default();
        assert!(s.is_everything());
        s.advanced_where = "  ".into();
        assert!(s.is_everything());
        s.columns.push("ra".into());
        assert!(!s.is_everything());
    }
}
