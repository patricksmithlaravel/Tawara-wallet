//! Android glue: the AndroidApp, the self-test switch, the app-private
//! directory, and the JNI clipboard and Back helpers. No Java code.

use std::ffi::CStr;
use std::sync::OnceLock;

use winit::platform::android::activity::AndroidApp;

use crate::{Config, android_jni, report};

static APP: OnceLock<AndroidApp> = OnceLock::new();

/// A system property. The shell user may set `debug.*` ones, so CI turns
/// the self-test on with `adb shell setprop debug.tawara.spike 1`.
fn system_property(name: &CStr) -> Option<String> {
    let mut value = [0 as libc::c_char; 92]; // PROP_VALUE_MAX
    // SAFETY: `name` is NUL-terminated and `value` has PROP_VALUE_MAX bytes.
    let len = unsafe { libc::__system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    // SAFETY: __system_property_get NUL-terminates what it writes.
    (len > 0).then(|| {
        unsafe { CStr::from_ptr(value.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

pub fn init(app: &AndroidApp) -> Config {
    let _ = APP.set(app.clone());
    let files = app.internal_data_path();
    report::note(format!(
        "internal_data_path={files:?} content_rect={:?}",
        app.content_rect()
    ));
    if let Some(backend) = system_property(c"debug.tawara.spike.backend") {
        report::note(format!(
            "ICED_BACKEND={backend} (from debug.tawara.spike.backend)"
        ));
        // SAFETY: no other thread reads or writes the environment yet; iced
        // has not started.
        unsafe { std::env::set_var("ICED_BACKEND", backend) };
    }
    // `files/` is 0771 on Android (group-writable), which the library refuses
    // as a store directory; the sibling `no_backup/` holds the run
    // directories, and is outside Auto Backup.
    let no_backup = files
        .as_deref()
        .and_then(|f| f.parent())
        .map(|d| d.join("no_backup"));
    Config {
        self_test: system_property(c"debug.tawara.spike").as_deref() == Some("1"),
        probe_dirs: files.iter().chain(no_backup.iter()).cloned().collect(),
        data_dir: no_backup,
        screenshot: None,
        offline: false,
    }
}

pub fn clipboard_write(text: &str) -> bool {
    android_jni::write(text)
        .map_err(|e| report::note(format!("jni clipboard write: {e:?}")))
        .is_ok()
}

pub fn clipboard_read() -> Option<String> {
    android_jni::read()
        .map_err(|e| report::note(format!("jni clipboard read: {e:?}")))
        .ok()
        .flatten()
}

/// Back on the root activity: move the task to the background, as Android
/// 12 and later do themselves, keeping the activity and winit's event loop.
pub fn move_task_to_back() {
    if let Some(app) = APP.get() {
        if let Err(e) = android_jni::move_task_to_back(app) {
            report::note(format!("moveTaskToBack: {e:?}"));
        }
    }
}
