//! The "Select & download" tab: column picker, filter builder, advanced SQL
//! and subset download.
//!
//! Nothing here touches the network until the user presses download, because
//! queries against the bucket cost money.

use std::sync::Arc;

use iced::widget::{button, checkbox, column, pick_list, row, scrollable, text, text_input};
use iced::{Element, Length, Task};
use surf_core::{ColumnKind, Combinator, ExportFormat, Filter, FilterSet, Op, Selection};
use surf_query::TableInfo;

use super::downloads::Request;
use crate::services::Services;
use crate::widgets::{self, human_count, selectable, tint};

#[derive(Debug, Clone)]
pub enum Message {
    ToggleColumn(usize, bool),
    SelectAll(bool),
    ColumnSearch(String),
    AddFilter,
    RemoveFilter(usize),
    FilterColumn(usize, String),
    FilterOp(usize, Op),
    FilterValue(usize, String),
    FilterValue2(usize, String),
    Combinator(Combinator),
    Advanced(String),
    Format(ExportFormat),
    DownloadSubset,
    /// Result of checking the selection against the table before downloading.
    Validated(Selection, Result<(), String>),
}

pub enum Action {
    None,
    Run(Task<Message>),
    Download(Request),
}

#[derive(Debug, Clone)]
struct FilterRow {
    column: String,
    op: Op,
    value: String,
    value2: String,
}

pub struct Query {
    info: Arc<TableInfo>,
    column_names: Vec<String>,
    selected: Vec<bool>,
    search: String,
    filters: Vec<FilterRow>,
    combinator: Combinator,
    advanced: String,
    format: ExportFormat,
    /// A selection being checked before download.
    checking: bool,
    /// Why the last download attempt was refused (e.g. invalid SQL).
    refused: Option<String>,
}

impl Query {
    pub fn new(info: Arc<TableInfo>) -> Self {
        let column_names: Vec<String> = info.columns.iter().map(|c| c.name.clone()).collect();
        Query {
            selected: vec![true; column_names.len()],
            column_names,
            info,
            search: String::new(),
            filters: Vec::new(),
            combinator: Combinator::And,
            advanced: String::new(),
            format: ExportFormat::Parquet,
            checking: false,
            refused: None,
        }
    }

    /// The current choices as a domain [`Selection`].
    pub fn selection(&self) -> Selection {
        let all = self.selected.iter().all(|s| *s);
        Selection {
            columns: if all {
                Vec::new()
            } else {
                self.column_names
                    .iter()
                    .zip(&self.selected)
                    .filter(|(_, s)| **s)
                    .map(|(c, _)| c.clone())
                    .collect()
            },
            filters: FilterSet {
                combinator: self.combinator,
                filters: self
                    .filters
                    .iter()
                    .map(|f| {
                        Filter::new(f.column.clone(), f.op)
                            .with_values(f.value.clone(), f.value2.clone())
                    })
                    .collect(),
            },
            advanced_where: self.advanced.clone(),
        }
    }

    /// A problem that must be fixed before running, checked instantly.
    fn problem(&self) -> Option<String> {
        if !self.selected.iter().any(|s| *s) {
            return Some("Select at least one column.".into());
        }
        self.selection()
            .filters
            .validate(|name| self.kind_of(name))
            .err()
            .map(|e| e.to_string())
    }

    fn kind_of(&self, column: &str) -> Option<ColumnKind> {
        self.info.column(column).map(|c| c.kind)
    }

    pub fn update(&mut self, services: &Services, message: Message) -> Action {
        let mut changed = true;
        match message {
            Message::ToggleColumn(i, on) => self.selected[i] = on,
            Message::SelectAll(on) => {
                // Respect the search box: only (de)select what's visible.
                let search = self.search.to_lowercase();
                for (name, sel) in self.column_names.iter().zip(self.selected.iter_mut()) {
                    if name.to_lowercase().contains(&search) {
                        *sel = on;
                    }
                }
            }
            Message::ColumnSearch(s) => {
                self.search = s;
                changed = false;
            }
            Message::AddFilter => {
                let column = self.column_names.first().cloned().unwrap_or_default();
                let op = self.default_op(&column);
                self.filters.push(FilterRow {
                    column,
                    op,
                    value: String::new(),
                    value2: String::new(),
                });
            }
            Message::RemoveFilter(i) => {
                self.filters.remove(i);
            }
            Message::FilterColumn(i, column) => {
                let kind = self.kind_of(&column).unwrap_or(ColumnKind::Other);
                if !self.filters[i].op.applies_to(kind) {
                    self.filters[i].op = self.default_op(&column);
                }
                self.filters[i].column = column;
            }
            Message::FilterOp(i, op) => self.filters[i].op = op,
            Message::FilterValue(i, v) => self.filters[i].value = v,
            Message::FilterValue2(i, v) => self.filters[i].value2 = v,
            Message::Combinator(c) => self.combinator = c,
            Message::Advanced(s) => self.advanced = s,
            Message::Format(f) => {
                self.format = f;
                changed = false;
            }
            Message::DownloadSubset => {
                // Plan the query (no data is read) so mistakes in the advanced
                // condition show up here rather than after choosing a file.
                self.checking = true;
                self.refused = None;
                let engine = services.engine.clone();
                let key = self.info.key.clone();
                let selection = self.selection();
                return Action::Run(Task::perform(
                    {
                        let selection = selection.clone();
                        async move {
                            engine
                                .validate(&key, &selection)
                                .await
                                .map_err(|e| e.to_string())
                        }
                    },
                    move |result| Message::Validated(selection.clone(), result),
                ));
            }
            Message::Validated(selection, result) => {
                self.checking = false;
                // Ignore a result for a selection the user has since changed.
                if selection != self.selection() {
                    return Action::None;
                }
                return match result {
                    Ok(()) => Action::Download(Request::Subset {
                        key: self.info.key.clone(),
                        selection,
                        format: self.format,
                        expected_rows: None,
                    }),
                    Err(e) => {
                        self.refused = Some(e);
                        Action::None
                    }
                };
            }
        }
        if changed {
            self.refused = None;
        }
        Action::None
    }

    fn default_op(&self, column: &str) -> Op {
        let kind = self.kind_of(column).unwrap_or(ColumnKind::Other);
        Op::for_kind(kind).first().copied().unwrap_or(Op::IsNotNull)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let left = column![self.view_columns()].width(300).height(Length::Fill);
        let right = column![self.view_filters(), self.view_summary()]
            .spacing(14)
            .width(Length::Fill)
            .height(Length::Fill);
        row![left, right].spacing(16).height(Length::Fill).into()
    }

    fn view_columns(&self) -> Element<'_, Message> {
        let search = self.search.to_lowercase();
        let n_selected = self.selected.iter().filter(|s| **s).count();
        let boxes = self
            .info
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.name.to_lowercase().contains(&search))
            .map(|(i, c)| {
                row![
                    checkbox(self.selected[i])
                        .label(c.name.as_str())
                        .on_toggle(move |on| Message::ToggleColumn(i, on))
                        .width(Length::Fill),
                    widgets::muted(c.data_type.as_str()),
                ]
                .spacing(6)
                .into()
            });
        widgets::card(
            column![
                widgets::subheading("Columns"),
                widgets::muted(format!(
                    "{n_selected} of {} selected",
                    self.column_names.len()
                )),
                text_input("Search columns…", &self.search).on_input(Message::ColumnSearch),
                row![
                    button(text("Select all").size(13))
                        .style(button::secondary)
                        .on_press(Message::SelectAll(true)),
                    button(text("Select none").size(13))
                        .style(button::secondary)
                        .on_press(Message::SelectAll(false)),
                ]
                .spacing(8),
                scrollable(
                    column(boxes)
                        .spacing(4)
                        .padding(iced::Padding::ZERO.right(12))
                )
                .height(Length::Fill),
            ]
            .spacing(10)
            .height(Length::Fill),
        )
    }

    fn view_filters(&self) -> Element<'_, Message> {
        let rows = self.filters.iter().enumerate().map(|(i, f)| {
            let kind = self.kind_of(&f.column).unwrap_or(ColumnKind::Other);
            let mut line = row![
                pick_list(self.column_names.as_slice(), Some(&f.column), move |c| {
                    Message::FilterColumn(i, c)
                })
                .width(200),
                pick_list(Op::for_kind(kind), Some(f.op), move |op| Message::FilterOp(
                    i, op
                ))
                .width(130),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            if f.op.arity() >= 1 {
                line = line.push(
                    text_input(placeholder(kind), &f.value)
                        .on_input(move |v| Message::FilterValue(i, v))
                        .width(160),
                );
            }
            if f.op.arity() == 2 {
                line = line.push(text("and")).push(
                    text_input(placeholder(kind), &f.value2)
                        .on_input(move |v| Message::FilterValue2(i, v))
                        .width(160),
                );
            }
            line.push(widgets::spacer())
                .push(
                    button(text("Remove").size(13))
                        .style(button::danger)
                        .on_press(Message::RemoveFilter(i)),
                )
                .into()
        });

        let mut content = column![
            row![
                widgets::subheading("Rows"),
                widgets::spacer(),
                button(text("+ Add filter")).on_press(Message::AddFilter),
            ]
            .align_y(iced::Alignment::Center),
        ]
        .spacing(10);

        if self.filters.is_empty() {
            content = content.push(widgets::muted(
                "All rows are included. Add a filter to keep only some rows, e.g. dec > 0.",
            ));
        } else {
            if self.filters.len() > 1 {
                content = content.push(
                    row![
                        text("Keep rows matching"),
                        pick_list(
                            [Combinator::And, Combinator::Or],
                            Some(self.combinator),
                            Message::Combinator
                        ),
                        text("these filters:"),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                );
            }
            content = content.push(column(rows).spacing(8));
        }
        content = content.push(
            column![
                widgets::muted("Advanced (optional): an SQL condition, combined with the filters above using AND."),
                text_input("e.g.  dec > 0 AND (mag_r < 19.5 OR z IS NULL)", &self.advanced)
                    .on_input(Message::Advanced)
                    .font(widgets::MONO),
            ]
            .spacing(4),
        );
        widgets::card(content)
    }

    /// A plain-language description of what will be downloaded.
    fn view_summary(&self) -> Element<'_, Message> {
        let n_selected = self.selected.iter().filter(|s| **s).count();
        let n_columns = self.column_names.len();
        let columns = if n_selected == n_columns {
            format!("All {n_columns} columns")
        } else {
            format!("{n_selected} of {n_columns} columns")
        };
        let rows = match self.rows_description() {
            Some(condition) => format!("Rows where {condition}"),
            None => "All rows".to_owned(),
        };

        let problem = self.problem().or_else(|| self.refused.clone());
        let ready = self.problem().is_none() && !self.checking;
        let mut content = column![
            widgets::subheading("Your selection"),
            selectable(columns),
            selectable(rows),
            widgets::muted(format!(
                "The full table has {} rows ({}). Filtering happens while downloading, so only the data you asked for is saved.",
                human_count(self.info.num_rows),
                widgets::human_bytes(self.info.size_bytes),
            )),
            row![
                text("Format"),
                pick_list(ExportFormat::ALL, Some(self.format), Message::Format),
                button(text(if self.checking {
                    "Checking…"
                } else {
                    "Download selection"
                }))
                .style(button::success)
                .on_press_maybe(ready.then_some(Message::DownloadSubset)),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(8);
        if let Some(p) = problem {
            content = content.push(selectable(p).style(tint(text::danger)).size(13));
        }
        widgets::card(content)
    }

    /// The row condition in words, e.g. `dec > 0 AND mag_r < 19`.
    fn rows_description(&self) -> Option<String> {
        let joiner = match self.combinator {
            Combinator::And => " AND ",
            Combinator::Or => " OR ",
        };
        let filters: Vec<String> = self
            .filters
            .iter()
            .map(|f| match f.op.arity() {
                0 => format!("{} {}", f.column, f.op),
                1 => format!("{} {} {}", f.column, f.op, f.value.trim()),
                _ => format!(
                    "{} between {} and {}",
                    f.column,
                    f.value.trim(),
                    f.value2.trim()
                ),
            })
            .collect();
        let advanced = self.advanced.trim();
        match (filters.len(), advanced.is_empty()) {
            (0, true) => None,
            (0, false) => Some(advanced.to_owned()),
            (_, true) => Some(filters.join(joiner)),
            (1, false) => Some(format!("{} AND ({advanced})", filters[0])),
            (_, false) => Some(format!("({}) AND ({advanced})", filters.join(joiner))),
        }
    }
}

fn placeholder(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::Integer => "whole number",
        ColumnKind::Float => "number, e.g. 0.5",
        ColumnKind::String => "text",
        ColumnKind::Boolean => "true / false",
        ColumnKind::Temporal => "e.g. 2024-01-31",
        ColumnKind::Other => "",
    }
}
