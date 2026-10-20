//! A parquet table: overview (its maml metadata) and the select/download tab.

use std::sync::Arc;

use iced::widget::{button, column, row, scrollable, text};
use iced::{Element, Length, Task};
use surf_core::Entry;
use surf_core::maml::{self, Maml};
use surf_query::TableInfo;

use super::downloads::Request;
use super::query::{self, Query};
use crate::services::Services;
use crate::widgets::{self, Loadable, human_bytes, human_count};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Select,
}

#[derive(Debug, Clone)]
pub enum Message {
    InfoLoaded(Result<Arc<TableInfo>, String>),
    Tab(Tab),
    FieldSearch(String),
    DownloadFile,
    Query(query::Message),
}

pub enum Action {
    None,
    Run(Task<Message>),
    Download(Request),
}

pub struct TableScreen {
    entry: Entry,
    info: Loadable<Arc<TableInfo>>,
    /// The parsed maml (`None` when the table has none).
    maml: Option<Result<Maml, String>>,
    field_search: String,
    tab: Tab,
    query: Option<Query>,
}

impl TableScreen {
    pub fn new(services: &Services, entry: Entry) -> (Self, Task<Message>) {
        let engine = services.engine.clone();
        let key = entry.key.clone();
        let task = Task::perform(
            async move {
                engine
                    .table_info(&key)
                    .await
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            },
            Message::InfoLoaded,
        );
        let screen = TableScreen {
            entry,
            info: Loadable::Loading,
            maml: None,
            field_search: String::new(),
            tab: Tab::Overview,
            query: None,
        };
        (screen, task)
    }

    pub fn key(&self) -> &str {
        &self.entry.key
    }

    pub fn update(&mut self, services: &Services, message: Message) -> Action {
        match message {
            Message::InfoLoaded(Ok(info)) => {
                self.maml = info.maml.as_deref().map(maml::parse);
                self.query = Some(Query::new(info.clone()));
                self.info = Loadable::Ready(info);
                Action::None
            }
            Message::InfoLoaded(Err(e)) => {
                self.info = Loadable::Failed(e);
                Action::None
            }
            Message::Tab(tab) => {
                self.tab = tab;
                // Show a first preview the first time the tab is opened.
                match (&mut self.query, tab) {
                    (Some(q), Tab::Select) if q.needs_first_preview() => {
                        Action::Run(q.preview(services).map(Message::Query))
                    }
                    _ => Action::None,
                }
            }
            Message::FieldSearch(s) => {
                self.field_search = s;
                Action::None
            }
            Message::DownloadFile => Action::Download(Request::Object {
                key: self.entry.key.clone(),
                size: self.entry.size,
            }),
            Message::Query(msg) => {
                let Some(q) = &mut self.query else {
                    return Action::None;
                };
                match q.update(services, msg) {
                    query::Action::None => Action::None,
                    query::Action::Run(task) => Action::Run(task.map(Message::Query)),
                    query::Action::Download(request) => Action::Download(request),
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let tab_button = |label, tab| {
            button(text(label))
                .style(if self.tab == tab {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press(Message::Tab(tab))
        };
        let header = row![
            column![
                widgets::heading(self.entry.name.as_str()),
                widgets::muted(self.entry.key.as_str()),
            ]
            .spacing(2)
            .width(Length::Fill),
            tab_button("Overview", Tab::Overview),
            tab_button("Select & download", Tab::Select),
            button(text("Download whole file"))
                .style(button::secondary)
                .on_press(Message::DownloadFile),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let body: Element<'_, Message> = match (&self.info, self.tab) {
            (Loadable::Loading, _) => widgets::loading("table details"),
            (Loadable::Failed(e), _) => widgets::failed("this table", e),
            (Loadable::Ready(info), Tab::Overview) => self.view_overview(info),
            (Loadable::Ready(_), Tab::Select) => match &self.query {
                Some(q) => q.view().map(Message::Query),
                None => widgets::loading("columns"),
            },
        };

        column![header, body]
            .spacing(16)
            .padding(20)
            .height(Length::Fill)
            .into()
    }

    fn view_overview<'a>(&'a self, info: &'a TableInfo) -> Element<'a, Message> {
        let stats = row![
            widgets::stat("Rows", human_count(info.num_rows)),
            widgets::stat("Columns", human_count(info.columns.len() as u64)),
            widgets::stat("File size", human_bytes(info.size_bytes)),
        ]
        .spacing(40);

        let mut content = column![widgets::card(stats)].spacing(16);
        match (&self.maml, &info.maml) {
            (Some(Ok(maml)), _) => {
                content = content.push(widgets::maml_summary(maml));
                if !maml.fields.is_empty() {
                    content = content.push(widgets::maml_fields(
                        &maml.fields,
                        &self.field_search,
                        Message::FieldSearch,
                    ));
                }
            }
            (Some(Err(e)), Some(raw)) => {
                content = content
                    .push(widgets::muted(format!(
                        "The metadata couldn't be read ({e}); showing it as text."
                    )))
                    .push(widgets::yaml_view(raw));
            }
            _ => content = content.push(widgets::muted("This table has no metadata.")),
        }

        scrollable(content.padding(iced::Padding::ZERO.right(14)))
            .height(Length::Fill)
            .into()
    }
}
