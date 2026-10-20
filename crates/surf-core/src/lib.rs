//! Pure domain model for surf.
//!
//! This crate performs no I/O. It describes how keys in the bucket map onto
//! datasets, versions and tables, and how a user's column/row selection is
//! represented and validated before any query engine sees it.

pub mod filter;
pub mod maml;
pub mod path;
pub mod selection;
pub mod text;
pub mod tree;

pub use filter::{
    ColumnKind, Combinator, Filter, FilterError, FilterSet, Literal, Op, TypedFilter,
};
pub use path::{Version, file_stem, parent_name};
pub use selection::{ExportFormat, Selection, suggested_file_name};
pub use tree::{Entry, EntryKind, Listing, readme_key_for};
