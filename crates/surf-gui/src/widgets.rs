//! Small reusable view helpers.

use iced::widget::{button, column, container, row, scrollable, space, table, text};
use iced::{Element, Font, Length, font};

/// Something fetched asynchronously.
#[derive(Debug, Clone, Default)]
pub enum Loadable<T> {
    #[default]
    Loading,
    Ready(T),
    Failed(String),
}

pub const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};
pub const MONO: Font = Font::MONOSPACE;

pub fn heading<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(22).font(BOLD).into()
}

pub fn subheading<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(16).font(BOLD).into()
}

pub fn muted<'a, M: 'a>(label: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(label).size(13).style(text::secondary).into()
}

pub fn loading<'a, M: 'a>(what: &str) -> Element<'a, M> {
    container(text(format!("Loading {what}…")).style(text::secondary))
        .padding(12)
        .into()
}

pub fn failed<'a, M: 'a>(what: &str, error: &str) -> Element<'a, M> {
    container(
        column![
            text(format!("Could not load {what}")).font(BOLD),
            text(error.to_owned()).size(13)
        ]
        .spacing(4),
    )
    .padding(12)
    .style(container::danger)
    .width(Length::Fill)
    .into()
}

/// A dismissible error banner.
pub fn error_banner<'a, M: Clone + 'a>(message: &'a str, on_dismiss: M) -> Element<'a, M> {
    container(
        row![
            text(message).width(Length::Fill),
            button(text("Dismiss"))
                .style(button::secondary)
                .on_press(on_dismiss)
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
    )
    .padding(10)
    .style(container::danger)
    .width(Length::Fill)
    .into()
}

/// A card-like box around some content.
pub fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(14)
        .style(container::bordered_box)
        .width(Length::Fill)
        .into()
}

/// A grid of string cells, scrollable in both directions.
pub fn data_grid<'a, M: 'a>(columns: &'a [String], rows: &'a [Vec<String>]) -> Element<'a, M> {
    let cols = columns.iter().enumerate().map(|(i, name)| {
        table::column(text(name.as_str()).font(BOLD).size(13), move |r: usize| {
            text(rows[r].get(i).map(String::as_str).unwrap_or(""))
                .size(13)
                .font(MONO)
        })
    });
    let grid = table(cols, 0..rows.len()).padding_x(10).padding_y(4);
    scrollable(grid)
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// A label/value pair for stats rows.
pub fn stat<'a, M: 'a>(label: &'a str, value: String) -> Element<'a, M> {
    column![muted(label), text(value).size(18).font(BOLD)]
        .spacing(2)
        .into()
}

pub fn spacer<'a, M: 'a>() -> Element<'a, M> {
    space::horizontal().into()
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn human_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers() {
        assert_eq!(human_count(0), "0");
        assert_eq!(human_count(1234567), "1,234,567");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
