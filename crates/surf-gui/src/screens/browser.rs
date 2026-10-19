//! The bucket tree in the sidebar, loaded lazily folder by folder.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length, Task};
use surf_core::{Entry, EntryKind, Listing};

use crate::services::Services;
use crate::widgets::{self, BOLD, human_bytes};

#[derive(Debug, Clone)]
pub enum Message {
    Clicked(Entry),
    Listed(String, Result<Arc<Listing>, String>),
    Refresh,
}

/// What the browser asks the app to do.
pub enum Action {
    None,
    Run(Task<Message>),
    /// Show a version's page; `load` lists its folder in the tree if needed.
    OpenVersion {
        prefix: String,
        load: Option<Task<Message>>,
    },
    OpenTable(Entry),
    OpenDocument(Entry),
}

#[derive(Debug)]
enum Folder {
    Loading,
    Loaded(Arc<Listing>),
    Failed(String),
}

#[derive(Debug, Default)]
pub struct Browser {
    folders: HashMap<String, Folder>,
    expanded: HashSet<String>,
    selected: Option<String>,
}

impl Browser {
    pub fn new(services: &Services) -> (Self, Task<Message>) {
        let mut browser = Browser::default();
        let task = browser.load(services, String::new());
        (browser, task)
    }

    pub fn update(&mut self, services: &Services, message: Message) -> Action {
        match message {
            Message::Listed(prefix, result) => {
                let folder = match result {
                    Ok(listing) => Folder::Loaded(listing),
                    Err(e) => Folder::Failed(e),
                };
                self.folders.insert(prefix, folder);
                Action::None
            }
            Message::Refresh => {
                self.folders.clear();
                let mut tasks = vec![self.load(services, String::new())];
                let open: Vec<String> = self.expanded.iter().cloned().collect();
                tasks.extend(open.into_iter().map(|p| self.load(services, p)));
                Action::Run(Task::batch(tasks))
            }
            Message::Clicked(entry) => {
                self.selected = Some(entry.key.clone());
                match entry.kind {
                    EntryKind::Folder | EntryKind::VersionFolder(_) => {
                        let is_version = matches!(entry.kind, EntryKind::VersionFolder(_));
                        let task = self.toggle(services, entry.key.clone(), is_version);
                        if is_version {
                            Action::OpenVersion {
                                prefix: entry.key,
                                load: task,
                            }
                        } else {
                            task.map_or(Action::None, Action::Run)
                        }
                    }
                    EntryKind::Parquet => Action::OpenTable(entry),
                    EntryKind::Markdown => Action::OpenDocument(entry),
                    EntryKind::Other => Action::None,
                }
            }
        }
    }

    /// Mark `key` as selected (e.g. when opened from elsewhere).
    pub fn select(&mut self, key: &str) {
        self.selected = Some(key.to_owned());
    }

    /// Expand/collapse a folder; returns a load task if it is not loaded yet.
    /// Version folders only ever expand when clicked.
    fn toggle(
        &mut self,
        services: &Services,
        prefix: String,
        keep_open: bool,
    ) -> Option<Task<Message>> {
        if self.expanded.contains(&prefix) && !keep_open {
            self.expanded.remove(&prefix);
            return None;
        }
        self.expanded.insert(prefix.clone());
        match self.folders.get(&prefix) {
            Some(Folder::Loaded(_) | Folder::Loading) => None,
            _ => Some(self.load(services, prefix)),
        }
    }

    fn load(&mut self, services: &Services, prefix: String) -> Task<Message> {
        self.folders.insert(prefix.clone(), Folder::Loading);
        let bucket = services.bucket().clone();
        Task::perform(
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
            move |result| Message::Listed(prefix.clone(), result),
        )
    }

    pub fn view(&self, bucket_name: &str) -> Element<'_, Message> {
        let header = row![
            text(bucket_name.to_owned())
                .font(BOLD)
                .size(16)
                .width(Length::Fill),
            button(text("Refresh").size(13))
                .style(button::secondary)
                .on_press(Message::Refresh),
        ]
        .align_y(iced::Alignment::Center);

        let mut rows = Vec::new();
        self.view_folder("", 0, &mut rows);
        column![
            header,
            widgets::muted(
                "Click a version (v1, v2, …) to read about it, or a table to explore it."
            ),
            scrollable(
                column(rows)
                    .spacing(1)
                    .padding(iced::Padding::ZERO.right(10))
            )
            .height(Length::Fill),
        ]
        .spacing(10)
        .padding(12)
        .into()
    }

    fn view_folder<'a>(&'a self, prefix: &str, depth: usize, out: &mut Vec<Element<'a, Message>>) {
        let indent = depth as f32 * 16.0;
        match self.folders.get(prefix) {
            None => {}
            Some(Folder::Loading) => {
                out.push(
                    container(widgets::muted("Loading…"))
                        .padding(iced::Padding::ZERO.left(indent + 22.0))
                        .into(),
                );
            }
            Some(Folder::Failed(e)) => {
                out.push(
                    container(
                        text(format!("Couldn't load: {e}"))
                            .size(12)
                            .style(text::danger),
                    )
                    .padding(iced::Padding::ZERO.left(indent + 22.0))
                    .into(),
                );
            }
            Some(Folder::Loaded(listing)) => {
                if listing.entries.is_empty() {
                    out.push(
                        container(widgets::muted("(empty)"))
                            .padding(iced::Padding::ZERO.left(indent + 22.0))
                            .into(),
                    );
                }
                for entry in &listing.entries {
                    out.push(self.view_entry(entry, indent));
                    if entry.kind.is_folder() && self.expanded.contains(&entry.key) {
                        self.view_folder(&entry.key, depth + 1, out);
                    }
                }
            }
        }
    }

    fn view_entry<'a>(&'a self, entry: &'a Entry, indent: f32) -> Element<'a, Message> {
        let expanded = self.expanded.contains(&entry.key);
        let (marker, label) = match entry.kind {
            EntryKind::Folder | EntryKind::VersionFolder(_) => {
                (if expanded { "▾" } else { "▸" }, entry.name.as_str())
            }
            EntryKind::Parquet => ("▦", entry.name.as_str()),
            EntryKind::Markdown => ("¶", entry.name.as_str()),
            EntryKind::Other => ("·", entry.name.as_str()),
        };
        let mut line = row![
            text(marker).width(16),
            text(label).size(14).width(Length::Fill)
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center);
        if let EntryKind::VersionFolder(_) = entry.kind {
            line = line.push(widgets::muted("version"));
        }
        if let (EntryKind::Parquet, Some(size)) = (entry.kind, entry.size) {
            line = line.push(widgets::muted(human_bytes(size)));
        }
        let selected = self.selected.as_deref() == Some(entry.key.as_str());
        let clickable = !matches!(entry.kind, EntryKind::Other);
        button(line)
            .padding(iced::Padding::from([3, 6]).left(indent + 6.0))
            .width(Length::Fill)
            .style(if selected {
                button::primary
            } else {
                button::text
            })
            .on_press_maybe(clickable.then(|| Message::Clicked(entry.clone())))
            .into()
    }
}
