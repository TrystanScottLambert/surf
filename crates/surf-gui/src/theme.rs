//! The app's single colour scheme, taken from the WAVES logo.

use iced::theme::Palette;
use iced::{Color, Theme, color};

/// Colours sampled from the WAVES logo.
pub mod logo {
    use super::*;

    pub const NAVY: Color = color!(0x0E338C);
    pub const OCEAN: Color = color!(0x106AB4);
    pub const SKY: Color = color!(0xA6D0EF);
    pub const FOAM: Color = color!(0xDEECEF);
}

/// Deep-ocean background: the logo's navy, darkened for long reading sessions.
const DEPTHS: Color = color!(0x0A1A33);

/// The WAVES theme: navy depths, foam text, ocean-blue accents. Status
/// colours stay distinct from the blues so they still read as status.
pub fn waves() -> Theme {
    Theme::custom(
        "WAVES",
        Palette {
            background: DEPTHS,
            text: logo::FOAM,
            primary: logo::OCEAN,
            success: color!(0x2FA58F),
            warning: color!(0xE8B04B),
            danger: color!(0xE5604D),
        },
    )
}
