//! A query-engine-agnostic model of row filters built in the UI.
//!
//! The UI edits [`Filter`]s as raw text. Before running anything, each filter
//! is validated against the column's [`ColumnKind`] into a [`TypedFilter`],
//! which a query backend can translate without any further string parsing.

use std::fmt;

/// Coarse column type, enough to decide which operators and literals apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Integer,
    Float,
    String,
    Boolean,
    /// Dates and timestamps; values are passed as text and cast by the engine.
    Temporal,
    /// Lists, structs, binary, … — only null checks are offered.
    Other,
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Between,
    Contains,
    StartsWith,
    IsNull,
    IsNotNull,
}

impl Op {
    pub const ALL: [Op; 11] = [
        Op::Eq,
        Op::NotEq,
        Op::Lt,
        Op::LtEq,
        Op::Gt,
        Op::GtEq,
        Op::Between,
        Op::Contains,
        Op::StartsWith,
        Op::IsNull,
        Op::IsNotNull,
    ];

    /// How many values the operator takes.
    pub fn arity(self) -> usize {
        match self {
            Op::IsNull | Op::IsNotNull => 0,
            Op::Between => 2,
            _ => 1,
        }
    }

    /// Whether this operator makes sense for a column of `kind`.
    pub fn applies_to(self, kind: ColumnKind) -> bool {
        use ColumnKind as K;
        match self {
            Op::IsNull | Op::IsNotNull => true,
            Op::Eq | Op::NotEq => kind != K::Other,
            Op::Lt | Op::LtEq | Op::Gt | Op::GtEq | Op::Between => {
                matches!(kind, K::Integer | K::Float | K::Temporal | K::String)
            }
            Op::Contains | Op::StartsWith => kind == K::String,
        }
    }

    /// Operators offered for a column kind, in display order.
    pub fn for_kind(kind: ColumnKind) -> Vec<Op> {
        Op::ALL
            .into_iter()
            .filter(|op| op.applies_to(kind))
            .collect()
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Op::Eq => "=",
            Op::NotEq => "≠",
            Op::Lt => "<",
            Op::LtEq => "≤",
            Op::Gt => ">",
            Op::GtEq => "≥",
            Op::Between => "between",
            Op::Contains => "contains",
            Op::StartsWith => "starts with",
            Op::IsNull => "is empty",
            Op::IsNotNull => "is not empty",
        })
    }
}

/// A typed literal value.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
}

/// How multiple filters are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Combinator {
    #[default]
    And,
    Or,
}

impl fmt::Display for Combinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Combinator::And => "all of (AND)",
            Combinator::Or => "any of (OR)",
        })
    }
}

/// One filter row as edited in the UI (values are raw text).
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub column: String,
    pub op: Op,
    pub value: String,
    /// Upper bound for [`Op::Between`].
    pub value2: String,
}

impl Filter {
    pub fn new(column: impl Into<String>, op: Op) -> Self {
        Filter {
            column: column.into(),
            op,
            value: String::new(),
            value2: String::new(),
        }
    }

    pub fn with_values(mut self, value: impl Into<String>, value2: impl Into<String>) -> Self {
        self.value = value.into();
        self.value2 = value2.into();
        self
    }

    /// Validate this filter for a column of the given kind.
    pub fn validate(&self, kind: ColumnKind) -> Result<TypedFilter, FilterError> {
        if !self.op.applies_to(kind) {
            return Err(FilterError::OperatorNotApplicable {
                column: self.column.clone(),
                op: self.op,
            });
        }
        let raw: &[&str] = match self.op.arity() {
            0 => &[],
            1 => &[self.value.as_str()],
            _ => &[self.value.as_str(), self.value2.as_str()],
        };
        let values = raw
            .iter()
            .map(|v| parse_literal(&self.column, self.op, kind, v))
            .collect::<Result<_, _>>()?;
        Ok(TypedFilter {
            column: self.column.clone(),
            kind,
            op: self.op,
            values,
        })
    }
}

/// A validated filter ready for a query backend.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedFilter {
    pub column: String,
    pub kind: ColumnKind,
    pub op: Op,
    /// Exactly `op.arity()` values.
    pub values: Vec<Literal>,
}

/// A set of filters joined by one combinator.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FilterSet {
    pub combinator: Combinator,
    pub filters: Vec<Filter>,
}

impl FilterSet {
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// Validate every filter. `kind_of` returns `None` for unknown columns.
    pub fn validate(
        &self,
        kind_of: impl Fn(&str) -> Option<ColumnKind>,
    ) -> Result<Vec<TypedFilter>, FilterError> {
        self.filters
            .iter()
            .map(|f| {
                let kind = kind_of(&f.column)
                    .ok_or_else(|| FilterError::UnknownColumn(f.column.clone()))?;
                f.validate(kind)
            })
            .collect()
    }
}

/// Why a filter could not be used.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FilterError {
    #[error("unknown column `{0}`")]
    UnknownColumn(String),
    #[error("`{op}` can't be used on column `{column}`")]
    OperatorNotApplicable { column: String, op: Op },
    #[error("`{column}` {op}: please enter a value")]
    MissingValue { column: String, op: Op },
    #[error("`{column}` {op}: `{value}` is not a valid {expected}")]
    InvalidValue {
        column: String,
        op: Op,
        value: String,
        expected: &'static str,
    },
}

fn parse_literal(
    column: &str,
    op: Op,
    kind: ColumnKind,
    raw: &str,
) -> Result<Literal, FilterError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() && !(kind == ColumnKind::String && matches!(op, Op::Eq | Op::NotEq)) {
        return Err(FilterError::MissingValue {
            column: column.to_owned(),
            op,
        });
    }
    let invalid = |expected| FilterError::InvalidValue {
        column: column.to_owned(),
        op,
        value: raw.to_owned(),
        expected,
    };
    match kind {
        ColumnKind::Integer => trimmed
            .parse::<i64>()
            .map(Literal::Int)
            // Allow `5.5` against an integer column: compare as float.
            .or_else(|_| trimmed.parse::<f64>().map(Literal::Float))
            .map_err(|_| invalid("number")),
        ColumnKind::Float => trimmed
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(Literal::Float)
            .ok_or_else(|| invalid("number")),
        ColumnKind::Boolean => match trimmed.to_ascii_lowercase().as_str() {
            "true" | "t" | "yes" | "1" => Ok(Literal::Bool(true)),
            "false" | "f" | "no" | "0" => Ok(Literal::Bool(false)),
            _ => Err(invalid("true/false value")),
        },
        // Strings keep their exact text (no trimming); temporal values are cast by the engine.
        ColumnKind::String => Ok(Literal::Str(raw.to_owned())),
        ColumnKind::Temporal => Ok(Literal::Str(trimmed.to_owned())),
        ColumnKind::Other => Err(invalid("value for this column type")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_applicability() {
        assert!(Op::Gt.applies_to(ColumnKind::Float));
        assert!(!Op::Contains.applies_to(ColumnKind::Float));
        assert!(!Op::Gt.applies_to(ColumnKind::Boolean));
        assert_eq!(
            Op::for_kind(ColumnKind::Other),
            vec![Op::IsNull, Op::IsNotNull]
        );
    }

    #[test]
    fn validates_numeric_filter() {
        let f = Filter::new("dec", Op::Gt).with_values("0", "");
        let t = f.validate(ColumnKind::Float).unwrap();
        assert_eq!(t.values, vec![Literal::Float(0.0)]);

        let f = Filter::new("n", Op::Between).with_values("1", " 5 ");
        let t = f.validate(ColumnKind::Integer).unwrap();
        assert_eq!(t.values, vec![Literal::Int(1), Literal::Int(5)]);
    }

    #[test]
    fn rejects_bad_values() {
        let f = Filter::new("dec", Op::Gt).with_values("abc", "");
        assert!(matches!(
            f.validate(ColumnKind::Float),
            Err(FilterError::InvalidValue { .. })
        ));
        let f = Filter::new("dec", Op::Gt);
        assert!(matches!(
            f.validate(ColumnKind::Float),
            Err(FilterError::MissingValue { .. })
        ));
        let f = Filter::new("dec", Op::Contains).with_values("x", "");
        assert!(matches!(
            f.validate(ColumnKind::Float),
            Err(FilterError::OperatorNotApplicable { .. })
        ));
        let f = Filter::new("dec", Op::Gt).with_values("inf", "");
        assert!(f.validate(ColumnKind::Float).is_err());
    }

    #[test]
    fn null_checks_take_no_value() {
        let t = Filter::new("x", Op::IsNull)
            .validate(ColumnKind::Other)
            .unwrap();
        assert!(t.values.is_empty());
    }

    #[test]
    fn filter_set_unknown_column() {
        let set = FilterSet {
            combinator: Combinator::And,
            filters: vec![Filter::new("nope", Op::IsNull)],
        };
        assert_eq!(
            set.validate(|_| None),
            Err(FilterError::UnknownColumn("nope".into()))
        );
    }
}
