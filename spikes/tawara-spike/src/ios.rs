//! iOS glue: the app-private directory, the pasteboard (window_clipboard
//! 0.5.1's iOS backend is a stub), and test hooks for what simctl cannot do.
//! Everything that touches UIKit runs on the main thread, inside
//! `iced::window::run`.

use std::path::PathBuf;
use std::ptr::NonNull;

use block2::RcBlock;
use iced::window::raw_window_handle::RawWindowHandle;
use objc2::msg_send;
use objc2::runtime::AnyObject;
use objc2_foundation::{MainThreadMarker, NSError, NSHomeDirectory, NSString};
use objc2_ui_kit::{
    UIApplication, UIInterfaceOrientationMask, UIPasteboard, UIView,
    UIWindowSceneGeometryPreferencesIOS,
};

use crate::report;

pub fn data_dir() -> Option<PathBuf> {
    let env_home = std::env::var_os("HOME").map(PathBuf::from);
    // SAFETY: NSHomeDirectory has no preconditions and returns a valid,
    // autoreleased NSString.
    let ns_home = PathBuf::from(unsafe { NSHomeDirectory().as_ref() }.to_string());
    report::note(format!("HOME={env_home:?} NSHomeDirectory={ns_home:?}"));
    let dir = ns_home.join("Library/Application Support");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

pub fn pasteboard_write(text: &str) -> bool {
    // SAFETY: plain UIKit calls with valid objects.
    unsafe { UIPasteboard::generalPasteboard().setString(Some(&NSString::from_str(text))) };
    true
}

pub fn pasteboard_read() -> Option<String> {
    // SAFETY: plain UIKit calls with valid objects.
    unsafe { UIPasteboard::generalPasteboard().string() }.map(|s| s.to_string())
}

fn ui_view(window: &dyn iced::window::Window) -> Option<NonNull<std::ffi::c_void>> {
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::UiKit(h) => Some(h.ui_view),
        _ => None,
    }
}

/// Sends text through the `UIKeyInput insertText:` path the soft keyboard
/// uses on winit's view, since simctl cannot type.
pub fn insert_text(window: &dyn iced::window::Window, text: &str) -> bool {
    let Some(view) = ui_view(window) else {
        return false;
    };
    // SAFETY: the handle's view lives as long as the window, which outlives
    // this call on the main thread.
    let view: &AnyObject = unsafe { view.cast::<AnyObject>().as_ref() };
    for c in text.chars() {
        let s = NSString::from_str(&c.to_string());
        // SAFETY: winit's view implements UIKeyInput's insertText:.
        let _: () = unsafe { msg_send![view, insertText: &*s] };
    }
    true
}

/// The view's safe-area insets (top, left, bottom, right) in points.
pub fn safe_area_insets(window: &dyn iced::window::Window) -> Option<[f64; 4]> {
    MainThreadMarker::new()?;
    let view = ui_view(window)?;
    // SAFETY: as in `insert_text`; the pointer is winit's UIView.
    let view: &UIView = unsafe { view.cast::<UIView>().as_ref() };
    let i = view.safeAreaInsets();
    Some([i.top, i.left, i.bottom, i.right])
}

/// Asks UIKit (iOS 16 and later) to rotate the app's window scene, since
/// simctl cannot rotate.
pub fn request_orientation(landscape: bool) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    // SAFETY: UIKit calls on the main thread with valid objects.
    #[allow(deprecated)]
    let Some(window) = (unsafe { UIApplication::sharedApplication(mtm).keyWindow() }) else {
        return false;
    };
    // SAFETY: as above.
    let Some(scene) = (unsafe { window.windowScene() }) else {
        return false;
    };
    let mask = if landscape {
        UIInterfaceOrientationMask::LandscapeRight
    } else {
        UIInterfaceOrientationMask::Portrait
    };
    // SAFETY: as above.
    let prefs = unsafe {
        UIWindowSceneGeometryPreferencesIOS::initWithInterfaceOrientations(mtm.alloc(), mask)
    };
    let on_error = RcBlock::new(|e: NonNull<NSError>| {
        // SAFETY: UIKit passes a valid NSError.
        report::note(format!("ios rotation refused: {:?}", unsafe { e.as_ref() }));
    });
    // SAFETY: as above.
    unsafe { scene.requestGeometryUpdateWithPreferences_errorHandler(&prefs, Some(&on_error)) };
    true
}
