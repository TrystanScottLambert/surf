//! The downloads panel: running jobs with progress, cancel and "show in folder".

use std::path::{Path, PathBuf};

use futures::SinkExt;
use iced::task::Handle;
use iced::widget::{button, column, progress_bar, row, scrollable, text};
use iced::{Element, Length, Task};
use surf_core::{ExportFormat, Selection, suggested_file_name};

use crate::services::Services;
use crate::widgets::{self, BOLD, human_bytes, human_count};

/// Something the user asked to download.
#[derive(Debug, Clone)]
pub enum Request {
    /// The raw file, byte for byte.
    Object { key: String, size: Option<u64> },
    /// A filtered / projected subset, written by the query engine.
    Subset {
        key: String,
        selection: Selection,
        format: ExportFormat,
        expected_rows: Option<u64>,
    },
}

impl Request {
    pub fn suggested_name(&self) -> String {
        match self {
            Request::Object { key, .. } => suggested_file_name(key, false, ExportFormat::Parquet),
            Request::Subset {
                key,
                selection,
                format,
                ..
            } => suggested_file_name(key, !selection.is_everything(), *format),
        }
    }

    pub fn format(&self) -> ExportFormat {
        match self {
            Request::Object { .. } => ExportFormat::Parquet,
            Request::Subset { format, .. } => *format,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Progress { done: u64, total: Option<u64> },
    Finished(Result<u64, String>),
}

#[derive(Debug, Clone)]
pub enum Message {
    Event(u64, Event),
    Cancel(u64),
    Reveal(u64),
    ClearFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Bytes,
    Rows,
}

impl Unit {
    fn format(self, n: u64) -> String {
        match self {
            Unit::Bytes => human_bytes(n),
            Unit::Rows => format!("{} rows", human_count(n)),
        }
    }
}

#[derive(Debug)]
enum State {
    Running { done: u64, total: Option<u64> },
    Done(u64),
    Failed(String),
    Cancelled,
}

struct Job {
    id: u64,
    dest: PathBuf,
    unit: Unit,
    state: State,
    handle: Option<Handle>,
}

#[derive(Default)]
pub struct Downloads {
    jobs: Vec<Job>,
    next_id: u64,
}

impl Downloads {
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Start writing `request` to `dest`.
    pub fn start(&mut self, services: &Services, request: Request, dest: PathBuf) -> Task<Message> {
        let id = self.next_id;
        self.next_id += 1;
        let engine = services.engine.clone();
        let (unit, total) = match &request {
            Request::Object { size, .. } => (Unit::Bytes, *size),
            Request::Subset { expected_rows, .. } => (Unit::Rows, *expected_rows),
        };
        tracing::info!(id, ?request, dest = %dest.display(), "starting download");

        let target = dest.clone();
        let stream = iced::stream::channel(
            32,
            async move |mut tx: futures::channel::mpsc::Sender<Event>| {
                // Progress is best-effort (try_send drops updates if the UI is behind);
                // the final result is always delivered.
                let mut progress = tx.clone();
                let result = match request {
                    Request::Object { key, .. } => engine
                        .bucket()
                        .download(&key, &target, |done, total| {
                            let _ = progress.try_send(Event::Progress {
                                done,
                                total: Some(total),
                            });
                        })
                        .await
                        .map_err(|e| e.to_string()),
                    Request::Subset {
                        key,
                        selection,
                        format,
                        ..
                    } => engine
                        .export(&key, &selection, format, &target, |done| {
                            let _ = progress.try_send(Event::Progress { done, total });
                        })
                        .await
                        .map_err(|e| e.to_string()),
                };
                let _ = tx.send(Event::Finished(result)).await;
            },
        );

        let (task, handle) = Task::run(stream, move |event| Message::Event(id, event)).abortable();
        self.jobs.push(Job {
            id,
            dest,
            unit,
            state: State::Running { done: 0, total },
            handle: Some(handle),
        });
        task
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Event(id, event) => {
                let Some(job) = self.job(id) else { return };
                if !matches!(job.state, State::Running { .. }) {
                    return;
                }
                job.state = match event {
                    Event::Progress { done, total } => State::Running { done, total },
                    Event::Finished(Ok(n)) => State::Done(n),
                    Event::Finished(Err(e)) => {
                        tracing::warn!(id, error = %e, "download failed");
                        State::Failed(e)
                    }
                };
                if !matches!(job.state, State::Running { .. }) {
                    job.handle = None;
                }
            }
            Message::Cancel(id) => {
                if let Some(job) = self.job(id) {
                    if let Some(handle) = job.handle.take() {
                        // Dropping the future removes the partial `.part` file.
                        handle.abort();
                    }
                    job.state = State::Cancelled;
                }
            }
            Message::Reveal(id) => {
                if let Some(job) = self.job(id) {
                    reveal(&job.dest);
                }
            }
            Message::ClearFinished => self
                .jobs
                .retain(|j| matches!(j.state, State::Running { .. })),
        }
    }

    fn job(&mut self, id: u64) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let jobs = self.jobs.iter().rev().map(|job| {
            let name = job
                .dest
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (status, fraction): (Element<'_, Message>, Option<f32>) = match &job.state {
                State::Running {
                    done,
                    total: Some(total),
                } if *total > 0 => (
                    widgets::muted(format!(
                        "{} of {}",
                        job.unit.format(*done),
                        job.unit.format(*total)
                    )),
                    Some(*done as f32 / *total as f32),
                ),
                State::Running { done, .. } => (
                    widgets::muted(format!("{} so far…", job.unit.format(*done))),
                    None,
                ),
                State::Done(n) => (
                    text(format!("Done — {}", job.unit.format(*n)))
                        .size(13)
                        .style(text::success)
                        .into(),
                    Some(1.0),
                ),
                State::Failed(e) => (
                    text(format!("Failed: {e}"))
                        .size(13)
                        .style(text::danger)
                        .into(),
                    None,
                ),
                State::Cancelled => (widgets::muted("Cancelled"), None),
            };
            let mut line = row![
                column![text(name).font(BOLD).size(14), status]
                    .spacing(2)
                    .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            if let Some(f) = fraction {
                line = line.push(
                    progress_bar(0.0..=1.0, f.clamp(0.0, 1.0))
                        .length(180)
                        .girth(8),
                );
            }
            line = match job.state {
                State::Running { .. } => line.push(
                    button(text("Cancel").size(13))
                        .style(button::danger)
                        .on_press(Message::Cancel(job.id)),
                ),
                State::Done(_) => line.push(
                    button(text("Show in folder").size(13))
                        .style(button::secondary)
                        .on_press(Message::Reveal(job.id)),
                ),
                _ => line,
            };
            line.into()
        });
        column![
            row![
                widgets::subheading("Downloads"),
                widgets::spacer(),
                button(text("Clear finished").size(13))
                    .style(button::text)
                    .on_press(Message::ClearFinished)
            ]
            .align_y(iced::Alignment::Center),
            scrollable(
                column(jobs)
                    .spacing(8)
                    .padding(iced::Padding::ZERO.right(12))
            )
            .height(Length::Shrink),
        ]
        .spacing(8)
        .padding([10, 16])
        .into()
    }
}

/// Open the folder containing `path` in the system file manager.
fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(drop);
    #[cfg(not(target_os = "macos"))]
    let result = open::that_detached(path.parent().unwrap_or(path));
    if let Err(e) = result {
        tracing::warn!(error = %e, "could not open file manager");
    }
}
