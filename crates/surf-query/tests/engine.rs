//! End-to-end tests of the query engine against an in-memory bucket.

use std::sync::Arc;

use datafusion::arrow::array::{BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::file::metadata::KeyValue;
use datafusion::parquet::file::properties::WriterProperties;
use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};
use surf_core::{ColumnKind, Combinator, ExportFormat, Filter, FilterSet, Op, Selection};
use surf_query::{Engine, QueryError};
use surf_storage::Bucket;
use surf_storage::object_store::memory::InMemory;
use surf_storage::object_store::path::Path as ObjectPath;
use surf_storage::object_store::{ObjectStoreExt, PutPayload};

const KEY: &str = "groups/v1/galaxies.parquet";
const N: i64 = 1000;
const MAML: &str = "# Galaxies\n\n| column | unit |\n|---|---|\n| ra | deg |";

/// `N` rows: id 0..N, ra = id * 0.36, dec = id - 500 (so 499 rows have dec > 0),
/// name "Gal-<id>" (null when id % 100 == 0), flag = id even.
fn fixture_bytes() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("ra", DataType::Float64, false),
        Field::new("dec", DataType::Float64, false),
        Field::new("name", DataType::Utf8, true),
        Field::new("flag", DataType::Boolean, false),
        Field::new("mag.r", DataType::Float64, false),
    ]));
    let ids: Vec<i64> = (0..N).collect();
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids.clone())),
            Arc::new(Float64Array::from_iter_values(
                ids.iter().map(|i| *i as f64 * 0.36),
            )),
            Arc::new(Float64Array::from_iter_values(
                ids.iter().map(|i| (*i - 500) as f64),
            )),
            Arc::new(StringArray::from_iter(
                ids.iter()
                    .map(|i| (i % 100 != 0).then(|| format!("Gal-{i}"))),
            )),
            Arc::new(BooleanArray::from_iter(
                ids.iter().map(|i| Some(i % 2 == 0)),
            )),
            Arc::new(Float64Array::from_iter_values(
                ids.iter().map(|i| 15.0 + (*i % 10) as f64),
            )),
        ],
    )
    .unwrap();
    let props = WriterProperties::builder()
        .set_max_row_group_row_count(Some(100))
        .set_key_value_metadata(Some(vec![KeyValue::new("maml".into(), MAML.to_owned())]))
        .build();
    let mut buf = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buf, schema, Some(props)).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    buf
}

async fn engine() -> Engine {
    let store = Arc::new(InMemory::new());
    store
        .put(&ObjectPath::from(KEY), PutPayload::from(fixture_bytes()))
        .await
        .unwrap();
    Engine::new(Bucket::from_store(store, "test", "memory://test")).unwrap()
}

fn filters(combinator: Combinator, filters: Vec<Filter>) -> Selection {
    Selection {
        filters: FilterSet {
            combinator,
            filters,
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn reads_table_info_from_footer() {
    let info = engine().await.table_info(KEY).await.unwrap();
    assert_eq!(info.num_rows, N as u64);
    assert_eq!(info.num_row_groups, 10);
    assert_eq!(info.maml.as_deref(), Some(MAML));
    let names: Vec<_> = info.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "ra", "dec", "name", "flag", "mag.r"]);
    assert_eq!(info.column("dec").unwrap().kind, ColumnKind::Float);
    assert_eq!(info.column("id").unwrap().kind, ColumnKind::Integer);
    assert_eq!(info.column("name").unwrap().kind, ColumnKind::String);
    assert!(info.column("name").unwrap().nullable);
    assert!(
        info.other_metadata
            .iter()
            .all(|(k, _)| k != "maml" && k != "ARROW:schema")
    );
}

#[tokio::test]
async fn preview_whole_table() {
    let p = engine()
        .await
        .preview(KEY, &Selection::default(), 25)
        .await
        .unwrap();
    assert_eq!(p.matching_rows, N as u64);
    assert_eq!(p.rows.len(), 25);
    assert_eq!(p.columns.len(), 6);
}

#[tokio::test]
async fn filter_dec_positive_and_project_columns() {
    let mut sel = filters(
        Combinator::And,
        vec![Filter::new("dec", Op::Gt).with_values("0", "")],
    );
    sel.columns = vec!["dec".into(), "id".into(), "mag.r".into()];
    let p = engine().await.preview(KEY, &sel, 5).await.unwrap();
    assert_eq!(p.matching_rows, 499);
    assert_eq!(p.columns, ["dec", "id", "mag.r"]);
    assert!(p.rows.iter().all(|r| r[0].parse::<f64>().unwrap() > 0.0));
}

#[tokio::test]
async fn combinators_and_operators() {
    let e = engine().await;
    let count = |sel: Selection| {
        let e = e.clone();
        async move { e.preview(KEY, &sel, 1).await.unwrap().matching_rows }
    };
    let between = Filter::new("id", Op::Between).with_values("10", "19");
    assert_eq!(
        count(filters(Combinator::And, vec![between.clone()])).await,
        10
    );

    let even = Filter::new("flag", Op::Eq).with_values("yes", "");
    assert_eq!(
        count(filters(
            Combinator::And,
            vec![between.clone(), even.clone()]
        ))
        .await,
        5
    );

    let high = Filter::new("id", Op::GtEq).with_values("990", "");
    assert_eq!(
        count(filters(Combinator::Or, vec![between, high])).await,
        20
    );

    let contains = Filter::new("name", Op::Contains).with_values("gal-99", "");
    // Gal-99, Gal-990..Gal-999 except Gal-900 (null) → 11
    assert_eq!(count(filters(Combinator::And, vec![contains])).await, 11);

    assert_eq!(
        count(filters(
            Combinator::And,
            vec![Filter::new("name", Op::IsNull)]
        ))
        .await,
        10
    );
    let int_vs_float = Filter::new("id", Op::Lt).with_values("2.5", "");
    assert_eq!(count(filters(Combinator::And, vec![int_vs_float])).await, 3);
}

#[tokio::test]
async fn advanced_sql_is_anded_with_builder() {
    let mut sel = filters(
        Combinator::And,
        vec![Filter::new("dec", Op::Gt).with_values("0", "")],
    );
    sel.advanced_where = "id < 600 AND \"mag.r\" >= 15".into();
    let p = engine().await.preview(KEY, &sel, 1).await.unwrap();
    assert_eq!(p.matching_rows, 99); // ids 501..=599
}

#[tokio::test]
async fn rejects_bad_input_without_running() {
    let e = engine().await;
    let sel = Selection {
        advanced_where: "dec >".into(),
        ..Default::default()
    };
    assert!(matches!(
        e.validate(KEY, &sel).await,
        Err(QueryError::AdvancedFilter(_))
    ));

    let sel = Selection {
        advanced_where: "nope > 1".into(),
        ..Default::default()
    };
    assert!(matches!(
        e.validate(KEY, &sel).await,
        Err(QueryError::AdvancedFilter(_))
    ));

    let sel = Selection {
        columns: vec!["nope".into()],
        ..Default::default()
    };
    assert!(matches!(
        e.validate(KEY, &sel).await,
        Err(QueryError::UnknownColumn(_))
    ));

    let sel = filters(
        Combinator::And,
        vec![Filter::new("dec", Op::Gt).with_values("north", "")],
    );
    assert!(matches!(
        e.validate(KEY, &sel).await,
        Err(QueryError::Filter(_))
    ));
}

#[tokio::test]
async fn empty_result_still_has_columns() {
    let mut sel = filters(
        Combinator::And,
        vec![Filter::new("id", Op::Gt).with_values("5000", "")],
    );
    sel.columns = vec!["ra".into()];
    let p = engine().await.preview(KEY, &sel, 10).await.unwrap();
    assert_eq!(p.matching_rows, 0);
    assert!(p.rows.is_empty());
    assert_eq!(p.columns, ["ra"]);
}

#[tokio::test]
async fn exports_parquet_with_maml_and_csv() {
    let e = engine().await;
    let dir = tempfile::tempdir().unwrap();
    let mut sel = filters(
        Combinator::And,
        vec![Filter::new("dec", Op::Gt).with_values("0", "")],
    );
    sel.columns = vec!["id".into(), "dec".into()];

    let pq = dir.path().join("out.parquet");
    let mut progress = 0;
    let rows = e
        .export(KEY, &sel, ExportFormat::Parquet, &pq, |r| progress = r)
        .await
        .unwrap();
    assert_eq!((rows, progress), (499, 499));
    let reader = SerializedFileReader::new(std::fs::File::open(&pq).unwrap()).unwrap();
    let meta = reader.metadata().file_metadata();
    assert_eq!(meta.num_rows(), 499);
    assert_eq!(meta.schema_descr().num_columns(), 2);
    let kv = meta.key_value_metadata().unwrap();
    assert!(
        kv.iter()
            .any(|k| k.key == "maml" && k.value.as_deref() == Some(MAML))
    );

    let csv = dir.path().join("out.csv");
    e.export(KEY, &sel, ExportFormat::Csv, &csv, |_| {})
        .await
        .unwrap();
    let text = std::fs::read_to_string(&csv).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("id,dec"));
    assert_eq!(lines.count(), 499);
    assert!(!dir.path().join("out.csv.part").exists());
}

#[tokio::test]
async fn works_against_a_local_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(KEY);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, fixture_bytes()).unwrap();
    let e = Engine::new(Bucket::local(dir.path()).unwrap()).unwrap();
    let listing = e.bucket().list_dir("groups/").await.unwrap();
    assert_eq!(listing.entries[0].name, "v1");
    let sel = filters(
        Combinator::And,
        vec![Filter::new("dec", Op::Gt).with_values("0", "")],
    );
    assert_eq!(e.preview(KEY, &sel, 1).await.unwrap().matching_rows, 499);
}
