//! Top-level application state: routes messages between the sidebar,
//! the current screen and the downloads panel.

use std::path::PathBuf;

use iced::widget::{column, container, image, row, rule, text};
use iced::{Element, Length, Task, Theme};
use surf_core::{Entry, ExportFormat};

use crate::assets;
use crate::screens::browser::{self, Browser};
use crate::screens::downloads::{self, Downloads, Request};
use crate::screens::table::{self, TableScreen};
use crate::screens::version::{self, VersionScreen};
use crate::services::Services;
use crate::theme::logo;
use crate::widgets::{self, BOLD};

#[derive(Debug, Clone)]
pub enum Message {
    Browser(browser::Message),
    /// Screen messages carry the key of the screen they belong to, so results
    /// for a screen the user already left are dropped.
    Version(String, version::Message),
    Table(String, table::Message),
    Downloads(downloads::Message),
    SaveTo(Request, Option<PathBuf>),
    DismissError,
}

enum Screen {
    Welcome,
    Version(VersionScreen),
    Table(Box<TableScreen>),
}

pub struct App {
    services: Result<Services, String>,
    browser: Browser,
    screen: Screen,
    downloads: Downloads,
    error: Option<String>,
    theme: Theme,
    last_dir: Option<PathBuf>,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let services = Services::connect();
        let (browser, task) = match &services {
            Ok(s) => {
                let (b, t) = Browser::new(s);
                (b, t.map(Message::Browser))
            }
            Err(_) => (Browser::default(), Task::none()),
        };
        let error = services
            .as_ref()
            .err()
            .map(|e| format!("Could not set up the connection: {e}"));
        let app = App {
            services,
            browser,
            screen: Screen::Welcome,
            downloads: Downloads::default(),
            error,
            theme: crate::theme::waves(),
            last_dir: directories::UserDirs::new()
                .and_then(|d| d.download_dir().map(PathBuf::from)),
        };
        (app, task)
    }

    pub fn title(&self) -> String {
        let place = match &self.screen {
            Screen::Welcome => None,
            Screen::Version(v) => Some(v.key().trim_end_matches('/')),
            Screen::Table(t) => Some(t.key()),
        };
        match place {
            Some(p) => format!("Surf — {p}"),
            None => "Surf".to_owned(),
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let Ok(services) = &self.services else {
            if let Message::DismissError = message {
                self.error = None;
            }
            return Task::none();
        };
        let services = services.clone();
        match message {
            Message::Browser(msg) => match self.browser.update(&services, msg) {
                browser::Action::None => Task::none(),
                browser::Action::Run(t) => t.map(Message::Browser),
                browser::Action::OpenVersion { prefix, load } => {
                    let load = load.map_or_else(Task::none, |t| t.map(Message::Browser));
                    Task::batch([load, self.open_version(&services, prefix)])
                }
                browser::Action::OpenTable(entry) => self.open_table(&services, entry),
            },
            Message::Version(key, msg) => {
                let Screen::Version(screen) = &mut self.screen else {
                    return Task::none();
                };
                if screen.key() != key {
                    return Task::none();
                }
                match screen.update(&services, msg) {
                    version::Action::None => Task::none(),
                    version::Action::Run(t) => t.map(move |m| Message::Version(key.clone(), m)),
                    version::Action::OpenTable(entry) => {
                        self.browser.select(&entry.key);
                        self.open_table(&services, entry)
                    }
                    version::Action::Download(entry) => self.ask_where(Request::Object {
                        key: entry.key,
                        size: entry.size,
                    }),
                    version::Action::OpenUrl(url) => {
                        open_url(&url);
                        Task::none()
                    }
                }
            }
            Message::Table(key, msg) => {
                let Screen::Table(screen) = &mut self.screen else {
                    return Task::none();
                };
                if screen.key() != key {
                    return Task::none();
                }
                match screen.update(&services, msg) {
                    table::Action::None => Task::none(),
                    table::Action::Run(t) => t.map(move |m| Message::Table(key.clone(), m)),
                    table::Action::Download(request) => self.ask_where(request),
                }
            }
            Message::Downloads(msg) => {
                self.downloads.update(msg);
                Task::none()
            }
            Message::SaveTo(_, None) => Task::none(),
            Message::SaveTo(request, Some(dest)) => {
                self.last_dir = dest.parent().map(PathBuf::from);
                self.downloads
                    .start(&services, request, dest)
                    .map(Message::Downloads)
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
        }
    }

    fn open_version(&mut self, services: &Services, prefix: String) -> Task<Message> {
        let (screen, task) = VersionScreen::new(services, prefix.clone());
        self.screen = Screen::Version(screen);
        task.map(move |m| Message::Version(prefix.clone(), m))
    }

    fn open_table(&mut self, services: &Services, entry: Entry) -> Task<Message> {
        let key = entry.key.clone();
        let (screen, task) = TableScreen::new(services, entry);
        self.screen = Screen::Table(Box::new(screen));
        task.map(move |m| Message::Table(key.clone(), m))
    }

    /// Show a native "Save as" dialog, then start the download.
    fn ask_where(&self, request: Request) -> Task<Message> {
        let format = request.format();
        let mut dialog = rfd::AsyncFileDialog::new().set_file_name(request.suggested_name());
        dialog = match format {
            ExportFormat::Parquet => dialog.add_filter("Parquet", &["parquet"]),
            ExportFormat::Csv => dialog.add_filter("CSV", &["csv"]),
        };
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }
        Task::perform(
            async move { dialog.save_file().await.map(|h| h.path().to_path_buf()) },
            move |path| Message::SaveTo(request.clone(), path),
        )
    }

    pub fn view(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let bucket = self
            .services
            .as_ref()
            .map(|s| s.bucket().name())
            .unwrap_or("not connected");

        let header = container(
            row![
                image(assets::wave()).height(30),
                text("Surf").size(20).font(BOLD),
                text("WAVES data browser").size(13).color(logo::SKY),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        )
        .padding([10, 16])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(logo::NAVY.into()),
            text_color: Some(logo::FOAM),
            ..container::Style::default()
        });

        let sidebar = container(self.browser.view(bucket).map(Message::Browser))
            .width(320)
            .height(Length::Fill);

        let main: Element<'_, Message> = match &self.screen {
            Screen::Welcome => welcome(),
            Screen::Version(a) => {
                let key = a.key().to_owned();
                a.view(theme).map(move |m| Message::Version(key.clone(), m))
            }
            Screen::Table(t) => {
                let key = t.key().to_owned();
                t.view().map(move |m| Message::Table(key.clone(), m))
            }
        };

        let mut main_col = column![].height(Length::Fill).width(Length::Fill);
        if let Some(e) = &self.error {
            main_col = main_col
                .push(container(widgets::error_banner(e, Message::DismissError)).padding(12));
        }
        main_col = main_col.push(main);

        let mut layout = column![
            header,
            rule::horizontal(1),
            row![sidebar, rule::vertical(1), main_col].height(Length::Fill)
        ];
        if !self.downloads.is_empty() {
            layout = layout
                .push(rule::horizontal(1))
                .push(container(self.downloads.view().map(Message::Downloads)).max_height(220));
        }
        layout.into()
    }
}

fn welcome<'a>() -> Element<'a, Message> {
    container(
        column![
            image(assets::wordmark()).width(460),
            widgets::heading("Welcome to Surf"),
            text("Browse the WAVES data products on the left."),
            text("• Click a dataset to open its latest version (v1, v2, …)."),
            text("• A version page shows its tables, description and dataset info."),
            text("• Click a table to see its columns and metadata, pick the rows and columns you want, and download them."),
        ]
        .spacing(10)
        .max_width(640),
    )
    .padding(32)
    .into()
}

fn open_url(url: &str) {
    let safe = ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| url.starts_with(scheme));
    if safe && let Err(e) = open::that_detached(url) {
        tracing::warn!(url, error = %e, "could not open link");
    }
}
