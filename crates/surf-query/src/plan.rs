//! Translating a [`Selection`] into a DataFusion plan.

use datafusion::arrow::datatypes::DataType;
use datafusion::logical_expr::{Expr, Like, cast};
use datafusion::prelude::{DataFrame, ident, lit};
use surf_core::{Combinator, Literal, Op, Selection, TypedFilter};

use crate::metadata::kind_of;
use crate::{QueryError, Result};

/// Apply filters, then the column projection, to `df`.
pub fn apply(df: DataFrame, selection: &Selection) -> Result<DataFrame> {
    let mut df = df;
    if let Some(predicate) = predicate(&df, selection)? {
        df = df.filter(predicate)?;
    }
    if !selection.columns.is_empty() {
        for c in &selection.columns {
            if df.schema().field_with_unqualified_name(c).is_err() {
                return Err(QueryError::UnknownColumn(c.clone()));
            }
        }
        let cols: Vec<&str> = selection.columns.iter().map(String::as_str).collect();
        df = df.select_columns(&cols)?;
    }
    Ok(df)
}

/// The combined row predicate, or `None` when every row is wanted.
pub fn predicate(df: &DataFrame, selection: &Selection) -> Result<Option<Expr>> {
    let schema = df.schema();
    let type_of = |name: &str| {
        schema
            .field_with_unqualified_name(name)
            .ok()
            .map(|f| f.data_type().clone())
    };
    let typed = selection
        .filters
        .validate(|name| type_of(name).map(|t| kind_of(&t)))?;
    let exprs = typed
        .iter()
        .map(|f| to_expr(f, &type_of(&f.column).expect("validated above")))
        .collect::<Vec<_>>();
    let builder = match selection.filters.combinator {
        Combinator::And => exprs.into_iter().reduce(Expr::and),
        Combinator::Or => exprs.into_iter().reduce(Expr::or),
    };

    let advanced = selection.advanced_where.trim();
    let advanced = if advanced.is_empty() {
        None
    } else {
        Some(parse_advanced(df, advanced)?)
    };

    Ok(match (builder, advanced) {
        (Some(a), Some(b)) => Some(a.and(b)),
        (a, b) => a.or(b),
    })
}

/// Parse and type-check a user-supplied SQL `WHERE` expression.
fn parse_advanced(df: &DataFrame, sql: &str) -> Result<Expr> {
    let expr = df
        .parse_sql_expr(sql)
        .map_err(|e| QueryError::AdvancedFilter(clean(&e)))?;
    // Planning a filter resolves columns and types, surfacing mistakes early.
    df.clone()
        .filter(expr.clone())
        .map_err(|e| QueryError::AdvancedFilter(clean(&e)))?;
    Ok(expr)
}

fn clean(e: &datafusion::error::DataFusionError) -> String {
    e.strip_backtrace()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn literal(value: &Literal, data_type: &DataType) -> Expr {
    match value {
        Literal::Int(i) => lit(*i),
        Literal::Float(f) => lit(*f),
        Literal::Bool(b) => lit(*b),
        Literal::Str(s) if kind_of(data_type) == surf_core::ColumnKind::Temporal => {
            cast(lit(s.clone()), data_type.clone())
        }
        Literal::Str(s) => lit(s.clone()),
    }
}

fn to_expr(f: &TypedFilter, data_type: &DataType) -> Expr {
    let column = ident(&f.column);
    let v = |i: usize| literal(&f.values[i], data_type);
    match f.op {
        Op::Eq => column.eq(v(0)),
        Op::NotEq => column.not_eq(v(0)),
        Op::Lt => column.lt(v(0)),
        Op::LtEq => column.lt_eq(v(0)),
        Op::Gt => column.gt(v(0)),
        Op::GtEq => column.gt_eq(v(0)),
        Op::Between => column.between(v(0), v(1)),
        Op::IsNull => column.is_null(),
        Op::IsNotNull => column.is_not_null(),
        Op::Contains | Op::StartsWith => {
            let Literal::Str(text) = &f.values[0] else {
                unreachable!("string ops only validate for string columns")
            };
            let escaped = escape_like(text);
            let pattern = if f.op == Op::Contains {
                format!("%{escaped}%")
            } else {
                format!("{escaped}%")
            };
            // Case-insensitive: friendlier for searching names.
            Expr::Like(Like::new(
                false,
                Box::new(column),
                Box::new(lit(pattern)),
                Some('\\'),
                true,
            ))
        }
    }
}

fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(escape_like("50%_a\\b"), "50\\%\\_a\\\\b");
    }
}
