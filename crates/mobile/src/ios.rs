//! iOS: the store's directory, its protection and backup exclusion, the
//! pasteboard, the app-switcher cover, the lock and the window scene, through the
//! Objective-C runtime with no Swift or Objective-C source
//! (docs/DECISIONS.md D4, D32).
//!
//! Everything here runs on the main thread: [`main`] before winit's
//! `UIApplicationMain`, and the notification blocks on the main queue. What
//! this writes on stderr is one `TAWARA` line per fact, never a secret.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2_foundation::{
    MainThreadMarker, NSCopying, NSDictionary, NSFileManager, NSFileProtectionComplete,
    NSFileProtectionKey, NSHomeDirectory, NSNotification, NSNotificationCenter, NSNotificationName,
    NSNumber, NSOperationQueue, NSString, NSURL, NSURLIsExcludedFromBackupKey,
};
use objc2_ui_kit::{
    UIApplication, UIApplicationDidBecomeActiveNotification,
    UIApplicationDidEnterBackgroundNotification, UIApplicationWillResignActiveNotification,
    UIColor, UIPasteboard, UIScene, UISceneWillConnectNotification, UIView, UIViewAutoresizing,
    UIWindow, UIWindowDidBecomeVisibleNotification, UIWindowScene,
};
use tawara_app::Host;

thread_local! {
    /// The cover over the window while the application is not active.
    static COVER: RefCell<Option<Retained<UIView>>> = const { RefCell::new(None) };
    /// The application's one window scene, once UIKit has connected it.
    static SCENE: RefCell<Option<Retained<UIWindowScene>>> = const { RefCell::new(None) };
    /// Windows shown before the scene connected, to go into it when it does.
    static PENDING: RefCell<Vec<Retained<UIWindow>>> = const { RefCell::new(Vec::new()) };
}

/// Start the application. Never returns: winit's `UIApplicationMain` ends
/// the process.
pub fn main() {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("TAWARA stop: not started on the main thread");
        std::process::exit(1);
    };
    let Some(private_dir) = private_dir() else {
        eprintln!("TAWARA stop: the app-private directory could not be made");
        std::process::exit(1);
    };
    eprintln!("TAWARA start: private directory {}", private_dir.display());
    observe(mtm);
    let host = Host {
        private_dir,
        copy,
        back: || {},
    };
    if let Err(error) = tawara_app::run_in(host) {
        eprintln!("TAWARA stop: {error}");
        std::process::exit(1);
    }
}

/// `Library/Application Support/Tawara` in the app's container, made mode
/// `0700`, protected with `NSFileProtectionComplete` and excluded from
/// iCloud and device backups (D32 items 3 and 7). The store goes in a
/// folder of its own inside it; the protection is set again on everything
/// already there at each start, so a file made since inherits nothing it
/// should not keep.
fn private_dir() -> Option<PathBuf> {
    // SAFETY: NSHomeDirectory has no preconditions and returns a valid,
    // autoreleased NSString.
    let home = PathBuf::from(unsafe { NSHomeDirectory().as_ref() }.to_string());
    let dir = home.join("Library/Application Support/Tawara");
    {
        use std::os::unix::fs::DirBuilderExt;
        if !dir.is_dir() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&dir)
                .ok()?;
        }
    }
    let protected = protect(&dir);
    let excluded = exclude_from_backup(&dir);
    eprintln!("TAWARA store: protected={protected} excluded_from_backup={excluded}");
    (protected && excluded).then_some(dir)
}

/// `NSFileProtectionComplete` on `path` and on everything below it.
fn protect(path: &Path) -> bool {
    let mut ok = set_protection(path);
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let child = entry.path();
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            ok &= if is_dir {
                protect(&child)
            } else {
                set_protection(&child)
            };
        }
    }
    ok
}

fn set_protection(path: &Path) -> bool {
    // SAFETY: the keys and values are Foundation's own constants, and the
    // file manager is used as documented, with a valid path.
    unsafe {
        let value: Retained<AnyObject> =
            Retained::into_super(Retained::into_super(NSFileProtectionComplete.copy()));
        let attributes = NSDictionary::from_vec(&[NSFileProtectionKey], vec![value]);
        NSFileManager::defaultManager()
            .setAttributes_ofItemAtPath_error(&attributes, &ns_path(path))
            .map_err(|e| eprintln!("TAWARA protect {}: {e:?}", path.display()))
            .is_ok()
    }
}

/// `NSURLIsExcludedFromBackupKey` on the directory: neither iCloud nor a
/// device backup holds it (docs/PLAN.md section 4.5).
fn exclude_from_backup(dir: &Path) -> bool {
    // SAFETY: Foundation calls with valid objects and Foundation's own key.
    unsafe {
        let url = NSURL::fileURLWithPath_isDirectory(&ns_path(dir), true);
        let yes = NSNumber::new_bool(true);
        url.setResourceValue_forKey_error(Some(&yes), NSURLIsExcludedFromBackupKey)
            .map_err(|e| eprintln!("TAWARA exclude {}: {e:?}", dir.display()))
            .is_ok()
    }
}

fn ns_path(path: &Path) -> Retained<NSString> {
    NSString::from_str(&path.to_string_lossy())
}

/// The pasteboard: iced's clipboard is a stub on iOS
/// (docs/spikes/P1-REPORT.md).
fn copy(text: &str) {
    // SAFETY: plain UIKit calls with valid objects, on the main thread,
    // where iced runs the application's update.
    unsafe { UIPasteboard::generalPasteboard().setString(Some(&NSString::from_str(text))) };
}

/// `UIApplication`'s notices, on the main queue. winit hears them too, but
/// iced passes none of them to the application (docs/spikes/P1-REPORT.md,
/// I6). The observers live as long as the process.
fn observe(mtm: MainThreadMarker) {
    type Notice = (
        &'static NSNotificationName,
        fn(MainThreadMarker, &NSNotification),
    );
    // SAFETY: the notification names are UIKit's own constants.
    let notices: [Notice; 5] = unsafe {
        [
            (UISceneWillConnectNotification, scene_connected),
            (UIWindowDidBecomeVisibleNotification, window_visible),
            (UIApplicationWillResignActiveNotification, cover),
            (
                UIApplicationDidEnterBackgroundNotification,
                entered_background,
            ),
            (UIApplicationDidBecomeActiveNotification, uncover),
        ]
    };
    // SAFETY: the main queue and the default center are always valid.
    let (center, queue) = unsafe {
        (
            NSNotificationCenter::defaultCenter(),
            NSOperationQueue::mainQueue(),
        )
    };
    for (name, act) in notices {
        // SAFETY: the center passes a valid notification for the call.
        let block =
            RcBlock::new(move |note: NonNull<NSNotification>| act(mtm, unsafe { note.as_ref() }));
        // SAFETY: the block is called on the main queue, as `act` needs.
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                Some(&queue),
                &block,
            )
        };
        std::mem::forget(observer);
    }
}

/// UIKit has connected the application's scene (Info.plist's
/// `UIApplicationSceneManifest`, D32 item 8). Apps linked with the iOS 27
/// SDK must use the scene life cycle or UIKit stops them at launch, and in
/// that life cycle a window is shown only in a scene. winit 0.30 makes its
/// window with `initWithFrame:` and no scene, so the shell puts it in the
/// scene: here, if it was shown first, or in [`window_visible`] if the
/// scene came first. Both orders happen.
fn scene_connected(_: MainThreadMarker, note: &NSNotification) {
    // SAFETY: this notification's object is the UIScene that connects.
    let Some(scene) = (unsafe { note.object() }).map(|o| unsafe { Retained::cast::<UIScene>(o) })
    else {
        return;
    };
    if !scene.is_kind_of::<UIWindowScene>() {
        return;
    }
    // SAFETY: checked just above.
    let scene = unsafe { Retained::cast::<UIWindowScene>(scene) };
    eprintln!("TAWARA scene: connected");
    for window in PENDING.take() {
        place(&window, &scene);
    }
    SCENE.set(Some(scene));
}

/// A window became visible: into the scene if it has none, or held until
/// the scene connects.
fn window_visible(_: MainThreadMarker, note: &NSNotification) {
    // SAFETY: this notification's object is the UIWindow.
    let Some(window) = (unsafe { note.object() }).map(|o| unsafe { Retained::cast::<UIWindow>(o) })
    else {
        return;
    };
    // SAFETY: a UIKit call on the main thread with a valid window.
    if unsafe { window.windowScene() }.is_some() {
        return;
    }
    let scene = SCENE.with_borrow(Clone::clone);
    match scene {
        Some(scene) => place(&window, &scene),
        None => PENDING.with_borrow_mut(|pending| pending.push(window)),
    }
}

fn place(window: &UIWindow, scene: &UIWindowScene) {
    // SAFETY: UIKit calls on the main thread with valid objects.
    let placed = unsafe {
        window.setWindowScene(Some(scene));
        window.makeKeyAndVisible();
        window.windowScene().is_some()
    };
    // Read back from UIKit, for platform/ios/tawara.sh's check.
    eprintln!("TAWARA scene: window in scene={placed}");
}

/// The application is about to stop being active (the app switcher, a
/// call, Control Center): cover the window, so the snapshot iOS takes for
/// the app switcher shows nothing of the wallet (docs/PLAN.md section 4.3).
fn cover(mtm: MainThreadMarker, _: &NSNotification) {
    COVER.with_borrow_mut(|cover| {
        if cover.is_some() {
            return;
        }
        // SAFETY: UIKit calls on the main thread with valid objects.
        unsafe {
            #[allow(deprecated)]
            let Some(window) = UIApplication::sharedApplication(mtm).keyWindow() else {
                return;
            };
            let view = UIView::initWithFrame(mtm.alloc(), window.bounds());
            let [r, g, b] = background();
            let colour = UIColor::colorWithRed_green_blue_alpha(r, g, b, 1.0);
            view.setBackgroundColor(Some(&colour));
            view.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
            );
            window.addSubview(&view);
            *cover = Some(view);
        }
    });
    eprintln!("TAWARA lifecycle: inactive; covered");
}

/// Active again: the cover goes. The wallet is locked if the application
/// was in the background meanwhile.
fn uncover(_: MainThreadMarker, _: &NSNotification) {
    if let Some(view) = COVER.with_borrow_mut(Option::take) {
        // SAFETY: UIKit call on the main thread with a valid view.
        unsafe { view.removeFromSuperview() };
    }
    eprintln!("TAWARA lifecycle: active; uncovered");
}

/// In the background: the wallet locks there and then (D32 item 6).
fn entered_background(_: MainThreadMarker, _: &NSNotification) {
    eprintln!("TAWARA lifecycle: background; locking");
    tawara_app::left_foreground();
}

/// The application's background colour, as `UIColor` takes it.
fn background() -> [f64; 3] {
    let c = tawara_app::theme::color::BG_APP;
    [f64::from(c.r), f64::from(c.g), f64::from(c.b)]
}
