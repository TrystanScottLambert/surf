//! A dataset version's page: its tables, description (`.md`), dataset info
//! (`.daml`) and checksums, in tabs.

use std::sync::Arc;

use iced::widget::{button, column, markdown, row, scrollable, table, text};
use iced::{Element, Length, Padding, Task, Theme};
use surf_core::checksums::{self, Checksums};
use surf_core::{Entry, EntryKind, Listing, parent_name};

use crate::services::Services;
use crate::widgets::{self, BOLD, Loadable, MONO, human_bytes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tables,
    Description,
    DatasetInfo,
    Checksums,
}

/// Which supporting document a load result belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doc {
    Description,
    DatasetInfo,
    Checksums,
}

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Arc<Listing>, String>),
    DocLoaded(Doc, Result<Arc<String>, String>),
    Tab(Tab),
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

/// A supporting file: loading, absent from the folder, or loaded.
#[derive(Debug, Default)]
enum File<T> {
    #[default]
    Waiting,
    Missing,
    Loaded(Loadable<T>),
}

pub struct VersionScreen {
    prefix: String,
    title: String,
    tab: Tab,
    listing: Loadable<Arc<Listing>>,
    description: File<Vec<markdown::Item>>,
    dataset_info: File<Arc<String>>,
    checksums: File<Checksums>,
}

impl VersionScreen {
    pub fn new(services: &Services, prefix: String) -> (Self, Task<Message>) {
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
        let screen = VersionScreen {
            prefix,
            title,
            tab: Tab::Tables,
            listing: Loadable::Loading,
            description: File::Waiting,
            dataset_info: File::Waiting,
            checksums: File::Waiting,
        };
        (screen, task)
    }

    pub fn key(&self) -> &str {
        &self.prefix
    }

    pub fn update(&mut self, services: &Services, message: Message) -> Action {
        match message {
            Message::Listed(Ok(listing)) => {
                let mut tasks = Vec::new();
                let mut start = |entry: Option<&Entry>, doc: Doc| match entry {
                    Some(e) => {
                        tasks.push(load_text(services, e.key.clone(), doc));
                        true
                    }
                    None => false,
                };
                let has_md = start(listing.readme(), Doc::Description);
                let has_daml = start(listing.find(EntryKind::DatasetMeta), Doc::DatasetInfo);
                let has_sums = start(listing.find(EntryKind::Checksums), Doc::Checksums);
                self.description = waiting_or_missing(has_md);
                self.dataset_info = waiting_or_missing(has_daml);
                self.checksums = waiting_or_missing(has_sums);
                self.listing = Loadable::Ready(listing);
                Action::Run(Task::batch(tasks))
            }
            Message::Listed(Err(e)) => {
                self.listing = Loadable::Failed(e);
                Action::None
            }
            Message::DocLoaded(doc, result) => {
                match doc {
                    Doc::Description => {
                        self.description = File::Loaded(map(result, |md| {
                            markdown::parse(&surf_core::text::linkify(&md)).collect()
                        }));
                    }
                    Doc::DatasetInfo => self.dataset_info = File::Loaded(map(result, |t| t)),
                    Doc::Checksums => {
                        self.checksums = File::Loaded(map(result, |t| checksums::parse(&t)));
                    }
                }
                Action::None
            }
            Message::Tab(tab) => {
                self.tab = tab;
                Action::None
            }
            Message::Link(url) => Action::OpenUrl(url),
            Message::OpenTable(entry) => Action::OpenTable(entry),
            Message::Download(entry) => Action::Download(entry),
        }
    }

    pub fn view<'a>(&'a self, theme: &'a Theme) -> Element<'a, Message> {
        let tab_button = |label, tab| {
            button(text(label))
                .style(if self.tab == tab {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press(Message::Tab(tab))
        };
        let tabs = row![
            tab_button("Tables", Tab::Tables),
            tab_button("Description", Tab::Description),
            tab_button("Dataset info", Tab::DatasetInfo),
            tab_button("Checksums", Tab::Checksums),
        ]
        .spacing(8);

        let body: Element<'_, Message> = match &self.listing {
            Loadable::Loading => widgets::loading("this version"),
            Loadable::Failed(e) => widgets::failed("this version", e),
            Loadable::Ready(listing) => match self.tab {
                Tab::Tables => self.view_tables(listing),
                Tab::Description => view_file(&self.description, "description", |items| {
                    widgets::markdown_view(items, theme).map(Message::Link)
                }),
                Tab::DatasetInfo => view_file(&self.dataset_info, "dataset info", |yaml| {
                    widgets::yaml_view(yaml)
                }),
                Tab::Checksums => view_file(&self.checksums, "checksums", |sums| {
                    view_checksums(sums, listing)
                }),
            },
        };

        column![
            widgets::heading(self.title.as_str()),
            tabs,
            scrollable(column![body].padding(Padding::ZERO.right(14))).height(Length::Fill),
        ]
        .spacing(16)
        .padding(20)
        .into()
    }

    fn view_tables<'a>(&'a self, listing: &'a Listing) -> Element<'a, Message> {
        let mut content = column![].spacing(16);

        if let File::Loaded(Loadable::Ready(yaml)) = &self.dataset_info {
            let mut summary = column![].spacing(4);
            if let Some(desc) = yaml_value(yaml, "description") {
                summary = summary.push(text(desc).size(15));
            }
            let byline: Vec<String> = [("author", "By"), ("date", "Date"), ("license", "License")]
                .iter()
                .filter_map(|(k, label)| yaml_value(yaml, k).map(|v| format!("{label}: {v}")))
                .collect();
            if !byline.is_empty() {
                summary = summary.push(widgets::muted(byline.join("   ·   ")));
            }
            content = content.push(summary);
        }

        let tables: Vec<&Entry> = listing.of_kind(EntryKind::Parquet).collect();
        if tables.is_empty() {
            return content
                .push(widgets::muted("No parquet tables in this version."))
                .into();
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
        content.push(widgets::card(column(rows).spacing(8))).into()
    }
}

fn view_checksums<'a>(sums: &'a Checksums, listing: &'a Listing) -> Element<'a, Message> {
    let size_of = |file: &str| {
        let key = format!("{}{file}", listing.prefix);
        let entry = listing.entries.iter().find(|e| e.key == key);
        match (entry.and_then(|e| e.size), EntryKind::of_file(file)) {
            (Some(size), _) => human_bytes(size),
            (None, EntryKind::TableMeta) => "inside the table".to_owned(),
            (None, _) => "not in folder".to_owned(),
        }
    };
    let grid = table(
        [
            table::column(text("File").font(BOLD), |e: &checksums::ChecksumEntry| {
                text(e.file.as_str()).size(14)
            }),
            table::column(text("Type").font(BOLD), |e: &checksums::ChecksumEntry| {
                text(EntryKind::of_file(&e.file).describe()).size(13)
            }),
            table::column(
                text("Size").font(BOLD),
                move |e: &checksums::ChecksumEntry| text(size_of(&e.file)).size(13),
            ),
            table::column(text("SHA-1").font(BOLD), |e: &checksums::ChecksumEntry| {
                text(e.sha1.as_str()).size(13).font(MONO)
            }),
        ],
        sums.entries.iter(),
    )
    .padding_x(12)
    .padding_y(5);

    let notes = column(sums.notes.iter().map(|n| widgets::muted(n.as_str()))).spacing(2);
    widgets::card(column![notes, grid].spacing(12))
}

fn view_file<'a, T>(
    file: &'a File<T>,
    what: &str,
    view: impl FnOnce(&'a T) -> Element<'a, Message>,
) -> Element<'a, Message> {
    match file {
        File::Waiting | File::Loaded(Loadable::Loading) => widgets::loading(what),
        File::Missing => widgets::muted(format!("This version has no {what} file.")),
        File::Loaded(Loadable::Failed(e)) => widgets::failed(&format!("the {what}"), e),
        File::Loaded(Loadable::Ready(value)) => view(value),
    }
}

fn waiting_or_missing<T>(present: bool) -> File<T> {
    if present {
        File::Waiting
    } else {
        File::Missing
    }
}

fn map<T>(result: Result<Arc<String>, String>, f: impl FnOnce(Arc<String>) -> T) -> Loadable<T> {
    match result {
        Ok(text) => Loadable::Ready(f(text)),
        Err(e) => Loadable::Failed(e),
    }
}

/// The value of a top-level `key: value` line in simple YAML, joining folded
/// continuation lines and removing surrounding quotes. Not a YAML parser.
fn yaml_value(yaml: &str, key: &str) -> Option<String> {
    let mut lines = yaml.lines();
    let first = lines
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?
        .trim();
    let mut value = first.to_owned();
    for cont in lines.take_while(|l| l.starts_with("  ") && !l.trim_start().starts_with('-')) {
        value.push(' ');
        value.push_str(cont.trim());
    }
    let value = value.trim_matches(['\'', '"']).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn load_text(services: &Services, key: String, doc: Doc) -> Task<Message> {
    let bucket = services.bucket().clone();
    Task::perform(
        async move {
            bucket
                .read_text(&key)
                .await
                .map(Arc::new)
                .map_err(|e| e.to_string())
        },
        move |result| Message::DocLoaded(doc, result),
    )
}

#[cfg(test)]
mod tests {
    use super::yaml_value;

    const DAML: &str = "survey: WAVES\ndate: '2026-07-10'\ncoauthors:\n- A\ndescription: FUV-W2 photometry in the\n  WAVES-N region\nlicense: Copyright WAVES [Private]\n";

    #[test]
    fn reads_simple_yaml_values() {
        assert_eq!(yaml_value(DAML, "survey").as_deref(), Some("WAVES"));
        assert_eq!(yaml_value(DAML, "date").as_deref(), Some("2026-07-10"));
        assert_eq!(
            yaml_value(DAML, "description").as_deref(),
            Some("FUV-W2 photometry in the WAVES-N region")
        );
        assert_eq!(yaml_value(DAML, "coauthors"), None);
        assert_eq!(yaml_value(DAML, "missing"), None);
    }
}
