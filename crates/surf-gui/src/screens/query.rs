//! The "Select & download" tab: column picker, filter builder, advanced SQL,
//! live preview and subset download.

use std::sync::Arc;

use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_input,
};
use iced::{Element, Length, Task};
use surf_core::{ColumnKind, Combinator, ExportFormat, Filter, FilterSet, Op, Selection};
use surf_query::{Preview, TableInfo};

use super::downloads::Request;
use crate::services::Services;
use crate::widgets::{self, BOLD, human_count};

/// Rows shown in the preview grid.
const PREVIEW_ROWS: usize = 100;

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
    RunPreview,
    PreviewLoaded(u64, Result<Arc<Preview>, String>),
    CountLoaded(u64, Result<u64, String>),
    Format(ExportFormat),
    DownloadSubset,
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

#[derive(Debug)]
enum CountState {
    Idle,
    Counting,
    Done(u64),
    Failed(String),
}

#[derive(Debug, Default)]
enum PreviewState {
    #[default]
    NotRun,
    Loading,
    Ready(Arc<Preview>),
    Failed(String),
}

pub struct Query {
    info: Arc<TableInfo>,
    column_names: Vec<String>,
    selected: Vec<bool>,
    search: String,
    filters: Vec<FilterRow>,
    combinator: Combinator,
    advanced: String,
    preview: PreviewState,
    /// Matching-row count, computed separately because it can be slow on
    /// very large tables with filters.
    count: CountState,
    count_task: Option<iced::task::Handle>,
    /// Whether the selection changed since the last preview.
    stale: bool,
    /// Incremented for every preview request so late results are ignored.
    generation: u64,
    format: ExportFormat,
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
            preview: PreviewState::NotRun,
            count: CountState::Idle,
            count_task: None,
            stale: false,
            generation: 0,
            format: ExportFormat::Parquet,
        }
    }

    pub fn needs_first_preview(&self) -> bool {
        matches!(self.preview, PreviewState::NotRun)
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

    pub fn preview(&mut self, services: &Services) -> Task<Message> {
        if self.problem().is_some() {
            return Task::none();
        }
        self.generation += 1;
        self.preview = PreviewState::Loading;
        self.stale = false;
        let generation = self.generation;
        let engine = services.engine.clone();
        let key = self.info.key.clone();
        let selection = self.selection();
        let sample = Task::perform(
            {
                let (engine, key, selection) = (engine.clone(), key.clone(), selection.clone());
                async move {
                    engine
                        .sample(&key, &selection, PREVIEW_ROWS)
                        .await
                        .map(Arc::new)
                        .map_err(|e| e.to_string())
                }
            },
            move |result| Message::PreviewLoaded(generation, result),
        );
        // A previous, possibly long-running count is no longer wanted.
        if let Some(handle) = self.count_task.take() {
            handle.abort();
        }
        self.count = CountState::Counting;
        let (count, handle) = Task::perform(
            async move {
                engine
                    .count(&key, &selection)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |result| Message::CountLoaded(generation, result),
        )
        .abortable();
        self.count_task = Some(handle);
        Task::batch([sample, count])
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
            Message::RunPreview => return Action::Run(self.preview(services)),
            Message::PreviewLoaded(generation, result) => {
                if generation == self.generation {
                    self.preview = match result {
                        Ok(p) => PreviewState::Ready(p),
                        Err(e) => PreviewState::Failed(e),
                    };
                }
                changed = false;
            }
            Message::CountLoaded(generation, result) => {
                if generation == self.generation {
                    self.count_task = None;
                    self.count = match result {
                        Ok(n) => CountState::Done(n),
                        Err(e) => CountState::Failed(e),
                    };
                }
                changed = false;
            }
            Message::DownloadSubset => {
                let expected_rows = match (&self.count, self.stale) {
                    (CountState::Done(n), false) => Some(*n),
                    _ => None,
                };
                return Action::Download(Request::Subset {
                    key: self.info.key.clone(),
                    selection: self.selection(),
                    format: self.format,
                    expected_rows,
                });
            }
        }
        if changed {
            self.stale = true;
        }
        Action::None
    }

    fn default_op(&self, column: &str) -> Op {
        let kind = self.kind_of(column).unwrap_or(ColumnKind::Other);
        Op::for_kind(kind).first().copied().unwrap_or(Op::IsNotNull)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let left = column![self.view_columns()].width(300).height(Length::Fill);
        let right = column![
            self.view_filters(),
            self.view_actions(),
            self.view_preview()
        ]
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
                    .on_submit(Message::RunPreview)
                    .font(widgets::MONO),
            ]
            .spacing(4),
        );
        widgets::card(content)
    }

    fn view_actions(&self) -> Element<'_, Message> {
        let problem = self.problem();
        let busy = matches!(self.preview, PreviewState::Loading);
        let mut line = row![
            button(text(if busy {
                "Updating…"
            } else {
                "Update preview"
            }))
            .on_press_maybe((problem.is_none() && !busy).then_some(Message::RunPreview)),
            widgets::spacer(),
            text("Format"),
            pick_list(ExportFormat::ALL, Some(self.format), Message::Format),
            button(text("Download selection"))
                .style(button::success)
                .on_press_maybe(problem.is_none().then_some(Message::DownloadSubset)),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        if let Some(p) = problem {
            line = line.push(text(p).style(text::danger).size(13));
        }
        line.into()
    }

    fn view_preview(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match &self.preview {
            PreviewState::NotRun => widgets::muted("Press “Update preview” to see matching rows."),
            PreviewState::Loading => widgets::loading("preview"),
            PreviewState::Failed(e) => widgets::failed("the preview", e),
            PreviewState::Ready(p) => {
                let total = human_count(self.info.num_rows);
                let shown = p.rows.len() as u64;
                let mut summary = match &self.count {
                    CountState::Done(n) => format!("{} of {total} rows match", human_count(*n)),
                    CountState::Counting => format!("Counting matching rows (of {total})…"),
                    CountState::Failed(e) => format!("Couldn't count matching rows: {e}"),
                    CountState::Idle => String::new(),
                };
                let more = match self.count {
                    CountState::Done(n) => shown < n,
                    _ => shown == PREVIEW_ROWS as u64,
                };
                if more {
                    summary.push_str(&format!(" · showing the first {}", human_count(shown)));
                } else if shown == 0 {
                    summary.push_str(" · no rows to show");
                }
                let mut header = row![text(summary).font(BOLD)].spacing(10);
                if self.stale {
                    header = header
                        .push(text("(out of date — press Update preview)").style(text::warning));
                }
                column![
                    header,
                    container(widgets::data_grid(&p.columns, &p.rows)).height(Length::Fill)
                ]
                .spacing(8)
                .height(Length::Fill)
                .into()
            }
        };
        container(body)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
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
