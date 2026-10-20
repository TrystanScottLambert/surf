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

/// The window icon (used on Linux; macOS takes the icon from the app bundle).
pub fn window_icon() -> Option<window::Icon> {
    window::icon::from_file_data(include_bytes!("../assets/icon.png"), None)
        .inspect_err(|e| tracing::warn!(error = %e, "could not load window icon"))
        .ok()
}
