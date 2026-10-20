//! Small reusable view helpers.

use iced::widget::{
    button, column, container, markdown, row, scrollable, space, table, text, text_input,
};
use iced::{Element, Font, Length, Padding, Theme, font};
use surf_core::maml::{self, FieldTable, Maml};

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

/// Rendered markdown with consistent, readable styling: comfortable text
/// size, a clear heading scale, and a capped line length.
pub fn markdown_view<'a>(
    items: &'a [markdown::Item],
    theme: &'a Theme,
) -> Element<'a, markdown::Uri> {
    let base = 15.0;
    let settings = markdown::Settings {
        h1_size: (base * 1.75).into(),
        h2_size: (base * 1.45).into(),
        h3_size: (base * 1.25).into(),
        h4_size: (base * 1.1).into(),
        code_size: (base * 0.9).into(),
        spacing: (base * 1.1).into(),
        ..markdown::Settings::with_text_size(base, theme)
    };
    container(container(markdown::view(items, settings)).max_width(880))
        .padding([20, 28])
        .style(container::bordered_box)
        .width(Length::Fill)
        .into()
}

/// Plain YAML (or any preformatted text) in a monospace card.
pub fn yaml_view<'a, M: 'a>(yaml: &'a str) -> Element<'a, M> {
    card(text(yaml).font(MONO).size(13))
}

/// Top-level MAML/DAML entries as bold labels with their values.
pub fn maml_summary<'a, M: 'a>(maml: &'a Maml) -> Element<'a, M> {
    let rows = maml.entries.iter().map(|(key, value)| {
        row![
            text(maml::label(key)).font(BOLD).width(150),
            maml_value(value)
        ]
        .spacing(12)
        .into()
    });
    card(column(rows).spacing(8))
}

/// One metadata value: lists of records as a small table, scalars on one
/// line, short lists joined with commas, longer lists one item per line.
fn maml_value<'a, M: 'a>(value: &'a maml::Value) -> Element<'a, M> {
    use maml::Value;
    let line = |s: String| -> Element<'a, M> { text(s).width(Length::Fill).into() };
    if let Some(t) = value.as_table() {
        return small_table(t);
    }
    match value {
        Value::Text(t) => line(t.clone()),
        Value::Map(_) => line(value.inline()),
        Value::List(items) => {
            let short = items
                .iter()
                .all(|i| matches!(i, Value::Text(t) if t.chars().count() <= 32));
            if short {
                line(value.inline())
            } else {
                column(items.iter().map(|i| line(i.inline())))
                    .spacing(4)
                    .width(Length::Fill)
                    .into()
            }
        }
    }
}

/// A compact table for a list of records (e.g. a dataset's tables, DOIs).
fn small_table<'a, M: 'a>(t: FieldTable) -> Element<'a, M> {
    let columns = t.columns.iter().enumerate().map(|(i, name)| {
        table::column(
            text(name.replace('_', " ")).font(BOLD).size(13),
            move |r: Vec<String>| text(r[i].clone()).size(14),
        )
    });
    table(columns, t.rows).padding_x(12).padding_y(4).into()
}

/// The MAML `fields` as a searchable, scrollable table.
pub fn maml_fields<'a, M: Clone + 'a>(
    fields: &'a FieldTable,
    search: &'a str,
    on_search: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    let needle = search.to_lowercase();
    let matching: Vec<&Vec<String>> = fields
        .rows
        .iter()
        .filter(|r| needle.is_empty() || r.iter().any(|c| c.to_lowercase().contains(&needle)))
        .collect();
    let shown = matching.len();

    let columns = fields.columns.iter().enumerate().map(|(i, name)| {
        let col = table::column(
            text(name.replace('_', " ")).font(BOLD).size(13),
            move |r: &Vec<String>| {
                let cell = text(r[i].as_str()).size(13);
                if i == 0 { cell.font(MONO) } else { cell }
            },
        );
        if name == "info" {
            col.width(Length::Fill)
        } else {
            col
        }
    });
    let grid = table(columns, matching)
        .padding_x(10)
        .padding_y(4)
        .width(Length::Fill);

    let count = if needle.is_empty() {
        format!("{} columns", fields.rows.len())
    } else {
        format!("{shown} of {} columns", fields.rows.len())
    };
    card(
        column![
            row![
                subheading("Columns"),
                muted(count),
                spacer(),
                text_input("Search columns…", search)
                    .on_input(on_search)
                    .width(260),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            scrollable(container(grid).padding(Padding::ZERO.right(12))).height(480),
        ]
        .spacing(10),
    )
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
