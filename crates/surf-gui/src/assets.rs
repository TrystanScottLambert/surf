//! Logos and icons, embedded in the binary.
//!
//! Generated from `logos/` by `packaging/make_icons.sh`. Image handles are
//! created once: a fresh handle per frame would re-upload the texture.

use std::sync::LazyLock;

use iced::widget::image::Handle;
use iced::window;

static WAVE: LazyLock<Handle> =
    LazyLock::new(|| Handle::from_bytes(&include_bytes!("../assets/wave.png")[..]));
static WORDMARK: LazyLock<Handle> =
    LazyLock::new(|| Handle::from_bytes(&include_bytes!("../assets/wordmark.png")[..]));

/// The WAVES wave, without text.
pub fn wave() -> Handle {
    WAVE.clone()
}

/// The full WAVES wordmark, with light text for the dark WAVES theme.
pub fn wordmark() -> Handle {
    WORDMARK.clone()
}

/// The app icon (the wave on a white rounded tile) as PNG.
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// The window icon (used on Linux; macOS uses the dock icon instead).
pub fn window_icon() -> Option<window::Icon> {
    window::icon::from_file_data(ICON_PNG, None)
        .inspect_err(|e| tracing::warn!(error = %e, "could not load window icon"))
        .ok()
}
