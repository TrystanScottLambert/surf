//! The macOS dock icon when running outside an app bundle (e.g. `cargo run`).
//!
//! A bundled `Surf.app` gets its icon from `Contents/Resources/Surf.icns`;
//! a bare binary shows the generic "exec" icon unless it sets one itself.
#![allow(unsafe_code)]

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::NSData;

/// Set the dock icon from PNG bytes. Must be called on the main thread.
pub fn set_icon(png: &[u8]) {
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("dock icon not set: not on the main thread");
        return;
    };
    let data = NSData::with_bytes(png);
    let Some(image): Option<Retained<NSImage>> = NSImage::initWithData(mtm.alloc(), &data) else {
        tracing::warn!("dock icon not set: image could not be decoded");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: we are on the main thread (checked above) and pass a valid,
    // retained NSImage; AppKit keeps its own reference to it.
    unsafe { app.setApplicationIconImage(Some(&image)) };
}
