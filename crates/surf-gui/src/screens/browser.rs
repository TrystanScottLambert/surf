//! The bucket tree in the sidebar, loaded lazily folder by folder.
//!
//! Only folders and tables are shown; a version's description, metadata and
//! checksums appear on its version page instead.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length, Padding, Task};
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
    /// A dataset folder the user clicked before its listing arrived; its
    /// latest version opens once it does.
    pending_latest: Option<String>,
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
                self.folders.insert(prefix.clone(), folder);
                if self.pending_latest.as_deref() == Some(prefix.as_str()) {
                    self.pending_latest = None;
                    return self.open_latest(services, &prefix).unwrap_or(Action::None);
                }
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
                    EntryKind::VersionFolder(_) => {
                        let load = self.expand(services, entry.key.clone());
                        Action::OpenVersion {
                            prefix: entry.key,
                            load,
                        }
                    }
                    EntryKind::Folder => {
                        if self.expanded.remove(&entry.key) {
                            return Action::None;
                        }
                        match self.expand(services, entry.key.clone()) {
                            Some(load) => {
                                self.pending_latest = Some(entry.key);
                                Action::Run(load)
                            }
                            None => self
                                .open_latest(services, &entry.key)
                                .unwrap_or(Action::None),
                        }
                    }
                    EntryKind::Parquet => Action::OpenTable(entry),
                    _ => Action::None,
                }
            }
        }
    }

    /// Mark `key` as selected (e.g. when opened from elsewhere).
    pub fn select(&mut self, key: &str) {
        self.selected = Some(key.to_owned());
    }

    /// If `prefix` (already loaded) contains versions, open the newest one.
    fn open_latest(&mut self, services: &Services, prefix: &str) -> Option<Action> {
        let Some(Folder::Loaded(listing)) = self.folders.get(prefix) else {
            return None;
        };
        let latest = listing.latest_version()?.key.clone();
        self.selected = Some(latest.clone());
        let load = self.expand(services, latest.clone());
        Some(Action::OpenVersion {
            prefix: latest,
            load,
        })
    }

    /// Expand a folder; returns a load task if it isn't loaded yet.
    fn expand(&mut self, services: &Services, prefix: String) -> Option<Task<Message>> {
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
            widgets::muted("Click a dataset to see its latest version, or a table to explore it."),
            scrollable(column(rows).spacing(1).padding(Padding::ZERO.right(10)))
                .height(Length::Fill),
        ]
        .spacing(10)
        .padding(12)
        .into()
    }

    fn view_folder<'a>(&'a self, prefix: &str, depth: usize, out: &mut Vec<Element<'a, Message>>) {
        let indent = Padding::ZERO.left(depth as f32 * 16.0 + 22.0);
        match self.folders.get(prefix) {
            None => {}
            Some(Folder::Loading) => {
                out.push(container(widgets::muted("Loading…")).padding(indent).into());
            }
            Some(Folder::Failed(e)) => {
                out.push(
                    container(
                        text(format!("Couldn't load: {e}"))
                            .size(12)
                            .style(text::danger),
                    )
                    .padding(indent)
                    .into(),
                );
            }
            Some(Folder::Loaded(listing)) => {
                let visible: Vec<&Entry> = listing
                    .entries
                    .iter()
                    .filter(|e| e.kind.is_tree_visible())
                    .collect();
                if visible.is_empty() {
                    out.push(
                        container(widgets::muted("(no tables)"))
                            .padding(indent)
                            .into(),
                    );
                }
                for entry in visible {
                    out.push(self.view_entry(entry, depth as f32 * 16.0));
                    if entry.kind.is_folder() && self.expanded.contains(&entry.key) {
                        self.view_folder(&entry.key, depth + 1, out);
                    }
                }
            }
        }
    }

    fn view_entry<'a>(&'a self, entry: &'a Entry, indent: f32) -> Element<'a, Message> {
        let marker = match entry.kind {
            _ if entry.kind.is_folder() => {
                if self.expanded.contains(&entry.key) {
                    "▾"
                } else {
                    "▸"
                }
            }
            _ => "▦",
        };
        let mut line = row![
            text(marker).width(16),
            text(entry.name.as_str()).size(14).width(Length::Fill)
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
        button(line)
            .padding(Padding::from([3, 6]).left(indent + 6.0))
            .width(Length::Fill)
            .style(if selected {
                button::primary
            } else {
                button::text
            })
            .on_press(Message::Clicked(entry.clone()))
            .into()
    }
}
