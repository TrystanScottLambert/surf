//! A generic model of MAML (and DAML) metadata: YAML documents describing a
//! table (or a dataset), with an optional `fields` list describing columns.
//!
//! The model follows the document rather than hard-coding the WAVES-MAML
//! spec, so keys added in future still display.

use yaml_rust2::{Yaml, YamlLoader};

/// A metadata value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
}

impl Value {
    /// A one-line rendering: scalars as-is, lists joined by `, `, maps as
    /// `key: value, key: value` (nested maps in braces).
    pub fn inline(&self) -> String {
        match self {
            Value::Text(t) => t.clone(),
            Value::List(items) => items
                .iter()
                .map(Value::inline)
                .collect::<Vec<_>>()
                .join(", "),
            Value::Map(entries) => entries
                .iter()
                .map(|(k, v)| match v {
                    Value::Map(_) => format!("{k}: {{{}}}", v.inline()),
                    _ => format!("{k}: {}", v.inline()),
                })
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

/// Column descriptions from `fields`, as a rectangular table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldTable {
    /// Union of the fields' keys, in first-seen order.
    pub columns: Vec<String>,
    /// One row per field, one cell per column (`""` when a field lacks a key).
    pub rows: Vec<Vec<String>>,
}

impl FieldTable {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// A parsed metadata document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Maml {
    /// Every top-level key except `fields`, in document order.
    pub entries: Vec<(String, Value)>,
    pub fields: FieldTable,
}

impl Maml {
    /// A top-level value as one line of text.
    pub fn get(&self, key: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.inline())
            .filter(|s| !s.is_empty())
    }
}

/// Parse a MAML/DAML document.
pub fn parse(yaml: &str) -> Result<Maml, String> {
    let docs = YamlLoader::load_from_str(yaml).map_err(|e| e.to_string())?;
    let Some(Yaml::Hash(root)) = docs.into_iter().next() else {
        return Err("the metadata is not a list of `key: value` entries".to_owned());
    };
    let mut maml = Maml::default();
    for (k, v) in root {
        let key = scalar(&k);
        if key == "fields" {
            maml.fields = field_table(&v);
        } else {
            maml.entries.push((key, convert(&v)));
        }
    }
    Ok(maml)
}

/// A human label for a key: `MAML_version` -> `MAML version`, `author` -> `Author`.
pub fn label(key: &str) -> String {
    let spaced = key.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => spaced,
    }
}

fn field_table(v: &Yaml) -> FieldTable {
    let Yaml::Array(items) = v else {
        return FieldTable::default();
    };
    let fields: Vec<Vec<(String, Value)>> = items
        .iter()
        .map(|item| match convert(item) {
            Value::Map(entries) => entries,
            other => vec![("name".to_owned(), other)],
        })
        .collect();
    let mut columns: Vec<String> = Vec::new();
    for f in &fields {
        for (k, _) in f {
            if !columns.contains(k) {
                columns.push(k.clone());
            }
        }
    }
    let rows = fields
        .iter()
        .map(|f| {
            columns
                .iter()
                .map(|c| {
                    f.iter()
                        .find(|(k, _)| k == c)
                        .map(|(_, v)| v.inline())
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    FieldTable { columns, rows }
}

fn convert(v: &Yaml) -> Value {
    match v {
        Yaml::Array(items) => Value::List(items.iter().map(convert).collect()),
        Yaml::Hash(h) => Value::Map(h.iter().map(|(k, v)| (scalar(k), convert(v))).collect()),
        other => Value::Text(scalar(other)),
    }
}

fn scalar(v: &Yaml) -> String {
    match v {
        Yaml::String(s) | Yaml::Real(s) => s.clone(),
        Yaml::Integer(i) => i.to_string(),
        Yaml::Boolean(b) => b.to_string(),
        Yaml::Null | Yaml::BadValue | Yaml::Alias(_) => String::new(),
        Yaml::Array(_) | Yaml::Hash(_) => convert(v).inline(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = "\
survey: WAVES
dataset: input_catalogue
table: input_cat_north_03
version: 2
date: '2025-09-01'
author: Dave Smith <dave@example.com>
coauthors:
- Joe Bloggs <joe@example.com>
DOIs:
- DOI: 10.1093/mnras/sty440
  type: paper
- DOI: 10.5281/zenodo.10059903
  type: software
depends:
- survey: WAVES
  dataset: stars
  table: phot_south
  version: 2
description: Just an example.
comments:
- This is an example.
license: Copyright WAVES [Private]
MAML_version: 1.1
keywords:
- catalogue
- example
fields:
- name: id
  unit: unitless
  info: ID field.
  ucd: meta.id;meta.main
  data_type: int32
- name: ra
  unit: deg
  info: Right ascension.
  ucd: pos.eq.ra
  data_type: float64
  qc:
    min: 0
    max: 360
";

    #[test]
    fn parses_top_level_entries_in_order() {
        let m = parse(EXAMPLE).unwrap();
        let keys: Vec<&str> = m.entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "survey",
                "dataset",
                "table",
                "version",
                "date",
                "author",
                "coauthors",
                "DOIs",
                "depends",
                "description",
                "comments",
                "license",
                "MAML_version",
                "keywords"
            ]
        );
        assert_eq!(m.get("version").as_deref(), Some("2"));
        assert_eq!(m.get("date").as_deref(), Some("2025-09-01"));
        assert_eq!(m.get("MAML_version").as_deref(), Some("1.1"));
        assert_eq!(m.get("keywords").as_deref(), Some("catalogue, example"));
    }

    #[test]
    fn lists_of_maps_render_inline() {
        let m = parse(EXAMPLE).unwrap();
        let (_, dois) = m.entries.iter().find(|(k, _)| k == "DOIs").unwrap();
        let Value::List(items) = dois else {
            panic!("DOIs should be a list")
        };
        assert_eq!(items[0].inline(), "DOI: 10.1093/mnras/sty440, type: paper");
        let (_, depends) = m.entries.iter().find(|(k, _)| k == "depends").unwrap();
        let Value::List(items) = depends else {
            panic!()
        };
        assert_eq!(
            items[0].inline(),
            "survey: WAVES, dataset: stars, table: phot_south, version: 2"
        );
    }

    #[test]
    fn fields_become_a_table_with_union_of_keys() {
        let m = parse(EXAMPLE).unwrap();
        assert_eq!(
            m.fields.columns,
            ["name", "unit", "info", "ucd", "data_type", "qc"]
        );
        assert_eq!(m.fields.rows.len(), 2);
        assert_eq!(m.fields.rows[0][5], "");
        assert_eq!(m.fields.rows[1][5], "min: 0, max: 360");
        assert_eq!(m.fields.rows[1][0], "ra");
    }

    #[test]
    fn handles_documents_without_fields_and_bad_input() {
        let daml = parse("survey: WAVES\ntables:\n- name: a\n  version: 1\n").unwrap();
        assert!(daml.fields.is_empty());
        assert_eq!(daml.get("tables").as_deref(), Some("name: a, version: 1"));
        assert!(parse("key: [unclosed").is_err());
        assert!(parse("- just\n- a list\n").is_err());
    }

    #[test]
    fn labels() {
        assert_eq!(label("MAML_version"), "MAML version");
        assert_eq!(label("author"), "Author");
        assert_eq!(label("DOIs"), "DOIs");
    }
}
