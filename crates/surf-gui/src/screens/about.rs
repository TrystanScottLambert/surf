//! A dataset version's landing page (its `<dataset>.md` plus the tables in it),
//! or a standalone markdown document.

use std::sync::Arc;

use iced::widget::{button, column, markdown, row, scrollable, text};
use iced::{Element, Length, Task, Theme};
use surf_core::{Entry, EntryKind, Listing, parent_name};

use crate::services::Services;
use crate::widgets::{self, Loadable, human_bytes};

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Arc<Listing>, String>),
    DocLoaded(Result<Arc<String>, String>),
    Link(markdown::Uri),
    OpenTable(Entry),
    Download(Entry),
}

pub enum Action {
    None,
    Run(Task<Message>),
    OpenTable(Entry),
    Download(Entry),
    OpenUrl(String),
}

/// Markdown that couldn't be found is not an error worth a red box.
#[derive(Debug)]
enum Doc {
    Missing,
    Loaded(Vec<markdown::Item>),
}

pub struct About {
    /// Version prefix (`groups/v1/`) or document key.
    key: String,
    title: String,
    is_version: bool,
    doc: Loadable<Doc>,
    files: Loadable<Arc<Listing>>,
}

impl About {
    /// Landing page for a version folder such as `groups/v1/`.
    pub fn version(services: &Services, prefix: String) -> (Self, Task<Message>) {
        let version = surf_core::path::last_segment(&prefix).to_owned();
        let title = match parent_name(&prefix) {
            Some(dataset) => format!("{dataset} — {version}"),
            None => version,
        };
        let bucket = services.bucket().clone();
        let task = Task::perform(
            {
                let prefix = prefix.clone();
                async move {
                    bucket
                        .list_dir(&prefix)
                        .await
                        .map(Arc::new)
                        .map_err(|e| e.to_string())
                }
            },
            Message::Listed,
        );
        let about = About {
            key: prefix,
            title,
            is_version: true,
            doc: Loadable::Loading,
            files: Loadable::Loading,
        };
        (about, task)
    }

    /// A standalone markdown document.
    pub fn document(services: &Services, key: String) -> (Self, Task<Message>) {
        let title = surf_core::path::last_segment(&key).to_owned();
        let task = load_doc(services, key.clone());
        let about = About {
            key,
            title,
            is_version: false,
            doc: Loadable::Loading,
            files: Loadable::Loading,
        };
        (about, task)
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn update(&mut self, services: &Services, message: Message) -> Action {
        match message {
            Message::Listed(Ok(listing)) => {
                let task = match listing.readme() {
                    Some(readme) => load_doc(services, readme.key.clone()),
                    None => {
                        self.doc = Loadable::Ready(Doc::Missing);
                        Task::none()
                    }
                };
                self.files = Loadable::Ready(listing);
                Action::Run(task)
            }
            Message::Listed(Err(e)) => {
                self.doc = Loadable::Failed(e.clone());
                self.files = Loadable::Failed(e);
                Action::None
            }
            Message::DocLoaded(result) => {
                self.doc = match result {
                    Ok(md) => Loadable::Ready(Doc::Loaded(markdown::parse(&md).collect())),
                    Err(e) => Loadable::Failed(e),
                };
                Action::None
            }
            Message::Link(url) => Action::OpenUrl(url),
            Message::OpenTable(entry) => Action::OpenTable(entry),
            Message::Download(entry) => Action::Download(entry),
        }
    }

    pub fn view<'a>(&'a self, theme: &'a Theme) -> Element<'a, Message> {
        let mut content = column![widgets::heading(self.title.as_str())].spacing(16);

        if self.is_version {
            content = content.push(self.view_tables());
        }

        let doc: Element<'_, Message> = match &self.doc {
            Loadable::Loading => widgets::loading("description"),
            Loadable::Failed(e) => widgets::failed("the description", e),
            Loadable::Ready(Doc::Missing) => {
                widgets::muted("This version has no description file.")
            }
            Loadable::Ready(Doc::Loaded(items)) => {
                widgets::card(markdown::view(items, theme).map(Message::Link))
            }
        };
        content = content.push(doc);

        scrollable(content.padding(iced::Padding::from(20).right(28)))
            .height(Length::Fill)
            .into()
    }

    fn view_tables(&self) -> Element<'_, Message> {
        match &self.files {
            Loadable::Loading => widgets::loading("tables"),
            Loadable::Failed(e) => widgets::failed("the tables", e),
            Loadable::Ready(listing) => {
                let tables: Vec<&Entry> = listing
                    .entries
                    .iter()
                    .filter(|e| e.kind == EntryKind::Parquet)
                    .collect();
                if tables.is_empty() {
                    return widgets::muted("No parquet tables in this version.");
                }
                let rows = tables.into_iter().map(|entry| {
                    row![
                        text(entry.name.as_str()).size(15).width(Length::Fill),
                        widgets::muted(entry.size.map(human_bytes).unwrap_or_default()),
                        button(text("Explore")).on_press(Message::OpenTable(entry.clone())),
                        button(text("Download file"))
                            .style(button::secondary)
                            .on_press(Message::Download(entry.clone())),
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center)
                    .into()
                });
                widgets::card(
                    column![widgets::subheading("Tables")]
                        .push(column(rows).spacing(8))
                        .spacing(10),
                )
            }
        }
    }
}

fn load_doc(services: &Services, key: String) -> Task<Message> {
    let bucket = services.bucket().clone();
    Task::perform(
        async move {
            bucket
                .read_text(&key)
                .await
                .map(Arc::new)
                .map_err(|e| e.to_string())
        },
        Message::DocLoaded,
    )
}
