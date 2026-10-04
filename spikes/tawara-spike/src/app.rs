//! The shared iced program. The self-test is the same program driving itself
//! with timed messages; every fact it learns goes through `report`.
//!
//! The layout above the touch area has fixed heights, so CI can tap the
//! password field and the touch area from the logged `layout` line on any
//! screen: the field's centre is `FIELD_Y` logical pixels from the top of the
//! window, and the touch area, a scrollable list of rows, fills everything
//! below `TOUCH_TOP`.

use std::time::{Duration, Instant};

use iced::widget::{
    button, column, container, operation, progress_bar, row, scrollable, space, text, text_input,
};
use iced::{Element, Event, Fill, Size, Subscription, Task, event, keyboard, touch, window};
use zeroize::Zeroizing;

use crate::worker::{self, Running};
use crate::{Config, report};

const PW: &str = "pw";
const NOTES: &str = "notes";
const PAD: f32 = 16.0;
const GAP: f32 = 12.0;
const HEADER_H: f32 = 40.0;
const FIELD_H: f32 = 64.0;
const FIELD_Y: f32 = PAD + HEADER_H + GAP + FIELD_H / 2.0;
const ROWS: [f32; 6] = [24.0, 56.0, 48.0, 12.0, 24.0, 24.0];
const TOUCH_TOP: f32 = PAD
    + HEADER_H
    + GAP
    + FIELD_H
    + GAP
    + ROWS[0]
    + GAP
    + ROWS[1]
    + GAP
    + ROWS[2]
    + GAP
    + ROWS[3]
    + GAP
    + ROWS[4]
    + GAP
    + ROWS[5]
    + GAP;
const CLIP: &str = "tawara-spike-clip";
const MOBILE: bool = cfg!(any(target_os = "android", target_os = "ios"));

pub struct Spike {
    config: Config,
    password: String,
    /// The last submitted secret, held as the worker will hold it.
    submitted: Option<Zeroizing<String>>,
    notes: String,
    show_field: bool,
    progress: (u32, u32),
    status: String,
    running: Option<Running>,
    finished: bool,
    cancelled_at: Option<Instant>,
    progress_after_cancel: u32,
    frames: u64,
    last_frame: Option<Instant>,
    max_gap_while_running: Duration,
    fingers: u32,
    taps: u32,
    /// The scroll offset last logged, and the largest seen.
    scroll_logged: f32,
    scroll_max: f32,
    size: Size,
    scale: f32,
    step: u32,
}

#[derive(Debug, Clone)]
pub enum Message {
    Password(String),
    Submit,
    Notes(String),
    Start,
    Cancel,
    Worker(worker::Event),
    Frame(Instant),
    Window(window::Id, window::Event),
    Finger,
    Row(u32),
    Scrolled(scrollable::Viewport),
    Back,
    Copy,
    Paste,
    Pasted(Option<String>),
    Shot(window::Screenshot),
    Script(u32),
    Noop,
}

impl Spike {
    pub fn new(config: Config) -> (Self, Task<Message>) {
        let first = if config.self_test {
            worker::after(1500, Message::Script(0))
        } else {
            Task::none()
        };
        let spike = Self {
            config,
            password: String::new(),
            submitted: None,
            notes: String::new(),
            show_field: true,
            progress: (0, 0),
            status: String::from("idle"),
            running: None,
            finished: false,
            cancelled_at: None,
            progress_after_cancel: 0,
            frames: 0,
            last_frame: None,
            max_gap_while_running: Duration::ZERO,
            fingers: 0,
            taps: 0,
            scroll_logged: 0.0,
            scroll_max: 0.0,
            size: Size::ZERO,
            scale: 1.0,
            step: 0,
        };
        (spike, first)
    }

    fn note_layout(&self) {
        report::note(format!(
            "layout logical {:.0}x{:.0} scale {}: field centre ({:.0},{FIELD_Y:.0}); touch target y {TOUCH_TOP:.0}..{:.0}",
            self.size.width,
            self.size.height,
            self.scale,
            self.size.width / 2.0,
            self.size.height - PAD
        ));
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Password(value) => {
                self.password = value;
                report::note(format!("pw_len={}", self.password.chars().count()));
                Task::none()
            }
            Message::Submit => {
                // The secret leaves the widget's state at once.
                let secret = Zeroizing::new(std::mem::take(&mut self.password));
                let len = secret.chars().count();
                report::note(format!("submit pw_len={len}"));
                if len > 0 {
                    report::check(
                        "password.moved_to_zeroizing",
                        self.password.is_empty(),
                        format!(
                            "{len} characters moved; the field holds {}",
                            self.password.chars().count()
                        ),
                    );
                    self.submitted = Some(secret);
                }
                Task::none()
            }
            Message::Notes(value) => {
                self.notes = value;
                report::note(format!(
                    "notes_len={} non_ascii={}",
                    self.notes.chars().count(),
                    self.notes.chars().filter(|c| !c.is_ascii()).count()
                ));
                Task::none()
            }
            Message::Start => self.start(40, 100),
            Message::Cancel => {
                // The abort drops the stream, so no event comes back from it;
                // the worker thread notes when it stops.
                if let Some(running) = self.running.take() {
                    running.cancel();
                    self.cancelled_at = Some(Instant::now());
                    self.status =
                        format!("cancelled at {} of {}", self.progress.0, self.progress.1);
                }
                Task::none()
            }
            Message::Worker(event) => {
                match event {
                    worker::Event::Progress { step, of } => {
                        self.progress = (step, of);
                        if self.cancelled_at.is_some() {
                            self.progress_after_cancel += 1;
                        }
                    }
                    worker::Event::Finished { steps, elapsed_ms } => {
                        self.running = None;
                        self.finished = true;
                        self.status = format!("finished {steps} steps in {elapsed_ms} ms");
                        report::note(&self.status);
                    }
                    worker::Event::Cancelled { at_step } => {
                        self.running = None;
                        self.status = format!("cancelled before step {at_step}");
                    }
                }
                Task::none()
            }
            Message::Frame(now) => {
                if let Some(last) = self.last_frame {
                    let gap = now - last;
                    self.max_gap_while_running = self.max_gap_while_running.max(gap);
                    if gap > Duration::from_secs(1) {
                        report::note(format!("frame after gap of {} ms", gap.as_millis()));
                    }
                }
                self.frames += 1;
                self.last_frame = Some(now);
                Task::none()
            }
            Message::Window(_, event) => {
                match event {
                    window::Event::RedrawRequested(_) => return Task::none(),
                    window::Event::Opened { size, .. } | window::Event::Resized(size) => {
                        self.size = size;
                        report::note(format!("window event {event:?}"));
                        self.note_layout();
                    }
                    window::Event::Rescaled(scale) => {
                        self.scale = scale;
                        report::note(format!("window event {event:?}"));
                        self.note_layout();
                    }
                    other => report::note(format!("window event {other:?}")),
                }
                Task::none()
            }
            Message::Finger => {
                self.fingers += 1;
                report::note(format!("touch finger n={}", self.fingers));
                Task::none()
            }
            Message::Row(n) => {
                self.taps += 1;
                report::note(format!("touch target n={} row={n}", self.taps));
                Task::none()
            }
            Message::Scrolled(viewport) => {
                // One line per 40 logical pixels of travel, not per event.
                let y = viewport.absolute_offset().y;
                self.scroll_max = self.scroll_max.max(y);
                if (y - self.scroll_logged).abs() >= 40.0 {
                    self.scroll_logged = y;
                    report::note(format!("scroll offset y={y:.0} max={:.0}", self.scroll_max));
                }
                Task::none()
            }
            Message::Back => back(),
            Message::Copy => {
                let ok = crate::platform_clipboard_write(CLIP);
                report::note(format!("platform clipboard write ok={ok}"));
                iced::clipboard::write(CLIP.into())
            }
            Message::Paste => {
                let platform = crate::platform_clipboard_read();
                report::note(format!(
                    "platform paste_len={:?}",
                    platform.as_ref().map(|s| s.chars().count())
                ));
                iced::clipboard::read().map(Message::Pasted)
            }
            Message::Pasted(value) => {
                report::note(format!(
                    "iced paste_len={:?}",
                    value.as_ref().map(|s| s.chars().count())
                ));
                Task::none()
            }
            Message::Shot(shot) => {
                self.scale = shot.scale_factor;
                // A blank or failed frame is one colour; text, inputs and
                // buttons draw dozens.
                let mut colours = std::collections::HashSet::new();
                for px in shot.rgba.chunks_exact(4) {
                    colours.insert([px[0], px[1], px[2]]);
                    if colours.len() > 64 {
                        break;
                    }
                }
                report::check(
                    "render.screenshot_not_blank",
                    colours.len() >= 16,
                    format!(
                        "{}x{} physical at scale {}, {}{} distinct colours",
                        shot.size.width,
                        shot.size.height,
                        shot.scale_factor,
                        if colours.len() > 64 { "at least " } else { "" },
                        colours.len()
                    ),
                );
                self.note_layout();
                if let Some(path) = &self.config.screenshot {
                    match write_png(path, &shot) {
                        Ok(()) => report::note(format!("screenshot written to {}", path.display())),
                        Err(e) => {
                            report::check("render.screenshot_write", false, e);
                        }
                    }
                }
                Task::none()
            }
            Message::Script(step) => self.script(step),
            Message::Noop => Task::none(),
        }
    }

    fn start(&mut self, steps: u32, step_ms: u64) -> Task<Message> {
        if let Some(previous) = self.running.take() {
            previous.cancel();
        }
        self.finished = false;
        self.cancelled_at = None;
        self.progress_after_cancel = 0;
        self.max_gap_while_running = Duration::ZERO;
        self.last_frame = None;
        self.progress = (0, steps);
        let (task, running) = worker::start(steps, step_ms, Message::Worker);
        self.running = Some(running);
        self.status = String::from("running");
        task
    }

    /// The self-test timeline. Each step checks what the previous one set
    /// up, then schedules the next.
    fn script(&mut self, step: u32) -> Task<Message> {
        self.step = step;
        let next = |ms, n| worker::after(ms, Message::Script(n));
        match step {
            0 => {
                // The screenshot (Message::Shot) is the first-frame check.
                report::note("focus requested on the password field");
                let shot = window::latest()
                    .and_then(window::screenshot)
                    .map(Message::Shot);
                Task::batch([operation::focus(PW), shot, ios_typing(), next(10_000, 1)])
            }
            1 => {
                // CI may have typed and pressed Return during the 10 s window.
                let typed = self.submitted.as_ref().map(|s| s.chars().count());
                report::note(format!(
                    "typed submitted_len={typed:?} field_len={}",
                    self.password.chars().count()
                ));
                let synthetic = if typed.is_none() {
                    // Where nothing could type, the move into Zeroizing is
                    // still checked, with a value set from here.
                    Task::done(Message::Password("correct horse battery".into()))
                        .chain(Task::done(Message::Submit))
                } else {
                    Task::none()
                };
                Task::batch([synthetic, self.start(40, 100), next(450, 2)])
            }
            2 => {
                report::check(
                    "task.progress",
                    self.progress.0 >= 2,
                    format!("{:?}", self.progress),
                );
                Task::batch([Task::done(Message::Cancel), next(1500, 3)])
            }
            3 => {
                // At most one update already in flight may land after the abort.
                report::check(
                    "task.cancel",
                    self.running.is_none()
                        && self.progress_after_cancel <= 1
                        && self.progress.0 < 40,
                    format!(
                        "progress {:?}, {} updates after cancel, status {}",
                        self.progress, self.progress_after_cancel, self.status
                    ),
                );
                Task::batch([self.start(20, 100), next(4000, 4)])
            }
            4 => {
                report::check(
                    "task.complete",
                    self.finished && self.running.is_none(),
                    &self.status,
                );
                report::check(
                    "ui.responsive",
                    self.max_gap_while_running < Duration::from_millis(500),
                    format!(
                        "largest frame gap while running {} ms",
                        self.max_gap_while_running.as_millis()
                    ),
                );
                if MOBILE {
                    let wrote = crate::platform_clipboard_write(CLIP);
                    let read = crate::platform_clipboard_read();
                    report::check(
                        "clipboard.platform_roundtrip",
                        wrote && read.as_deref() == Some(CLIP),
                        format!(
                            "write={wrote} read_len={:?}",
                            read.as_ref().map(String::len)
                        ),
                    );
                }
                // iced's own clipboard: expected to work on the desktop, and
                // to be window_clipboard's stub on Android and iOS.
                iced::clipboard::write::<Message>("tawara-iced-clip".into()).chain(
                    iced::clipboard::read().map(|v: Option<String>| {
                        let ok = v.as_deref() == Some("tawara-iced-clip");
                        if MOBILE {
                            report::note(format!(
                                "iced clipboard roundtrip={ok} (a stub is expected)"
                            ));
                            // Leave the platform clipboard holding the spike's
                            // value for the outside check (simctl pbpaste).
                            crate::platform_clipboard_write(CLIP);
                        } else {
                            report::check(
                                "clipboard.iced_roundtrip",
                                ok,
                                format!("read_len={:?}", v.map(|s| s.len())),
                            );
                        }
                        Message::Script(5)
                    }),
                )
            }
            5 => {
                self.show_field = false;
                report::note("field hidden");
                next(3000, 6)
            }
            6 => {
                self.show_field = true;
                report::note("field shown");
                Task::batch([operation::focus(PW), next(3000, 7)])
            }
            7 => {
                let config = self.config.clone();
                worker::on_thread(move || crate::library_checks(&config), Message::Script(8))
            }
            8 if cfg!(target_os = "ios") => Task::batch([ios_orientation(true), next(3000, 9)]),
            9 if cfg!(target_os = "ios") => Task::batch([ios_orientation(false), next(3000, 10)]),
            8 | 9 => Task::done(Message::Script(10)),
            10 => {
                report::done();
                if MOBILE {
                    // A long task keeps the program busy through the
                    // lifecycle checks CI runs next (background and return).
                    report::note("READY lifecycle");
                    self.start(600, 100)
                } else {
                    iced::exit()
                }
            }
            _ => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let busy = self.running.is_some();
        let field: Element<'_, Message> = if self.show_field {
            text_input("Password", &self.password)
                .id(PW)
                .secure(true)
                .on_input(Message::Password)
                .on_submit(Message::Submit)
                .padding(10)
                .size(18)
                .into()
        } else {
            space().into()
        };
        let submitted = match &self.submitted {
            Some(s) => format!(
                "submitted {} characters, held in a Zeroizing buffer",
                s.chars().count()
            ),
            None => format!("{} characters", self.password.chars().count()),
        };
        column![
            container(text("Tawara spike").size(24)).center_y(HEADER_H),
            container(field).center_y(FIELD_H),
            container(text(submitted).size(14)).center_y(ROWS[0]),
            container(
                text_input("Notes: type, compose, paste", &self.notes)
                    .id(NOTES)
                    .on_input(Message::Notes)
                    .padding(10)
                    .size(16)
            )
            .center_y(ROWS[1]),
            container(
                row![
                    button("Start").on_press_maybe((!busy).then_some(Message::Start)),
                    button("Cancel").on_press_maybe(busy.then_some(Message::Cancel)),
                    button("Copy").on_press(Message::Copy),
                    button("Paste").on_press(Message::Paste),
                ]
                .spacing(8)
            )
            .center_y(ROWS[2]),
            progress_bar(0.0..=self.progress.1.max(1) as f32, self.progress.0 as f32)
                .girth(ROWS[3]),
            container(text(&self.status).size(14)).center_y(ROWS[4]),
            container(
                text(format!(
                    "frames {}  fingers {}  taps {}  scroll {:.0}  step {}",
                    self.frames, self.fingers, self.taps, self.scroll_max, self.step
                ))
                .size(14)
            )
            .center_y(ROWS[5]),
            scrollable((1..=40u32).fold(column![].spacing(8), |rows, n| {
                rows.push(
                    button(text(format!("Row {n}: tap, or swipe to scroll")))
                        .width(Fill)
                        .padding(14)
                        .on_press(Message::Row(n)),
                )
            }))
            .on_scroll(Message::Scrolled)
            .width(Fill)
            .height(Fill),
        ]
        .spacing(GAP)
        .padding(PAD)
        .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Listening to frames makes iced redraw continuously (unthrottled on
        // tiny-skia), so only while a task runs: the largest gap between
        // frames then is how long the interface stalled.
        let frames = if self.running.is_some() {
            window::frames().map(Message::Frame)
        } else {
            Subscription::none()
        };
        Subscription::batch([
            frames,
            window::events().map(|(id, e)| Message::Window(id, e)),
            event::listen_with(keys),
        ])
    }
}

/// Mobile soft keyboards deliver Return as the character "\n" (winit's
/// Android key map, and iOS `insertText:`), which text_input ignores; Back
/// arrives as `BrowserBack` on Android. Raw finger presses are counted too.
fn keys(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    use keyboard::key::Named;
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key.as_ref() {
            keyboard::Key::Character("\n" | "\r") => Some(Message::Submit),
            keyboard::Key::Named(Named::BrowserBack) => Some(Message::Back),
            _ => None,
        },
        Event::Touch(touch::Event::FingerPressed { .. }) => Some(Message::Finger),
        _ => None,
    }
}

#[cfg(target_os = "android")]
fn back<T>() -> Task<T> {
    report::note("back pressed: moveTaskToBack");
    crate::android::move_task_to_back();
    Task::none()
}

#[cfg(not(target_os = "android"))]
fn back<T>() -> Task<T> {
    report::note("back pressed");
    Task::none()
}

/// iOS: log the safe-area insets, then type into the focused field through
/// winit's own `insertText:`, since simctl cannot type.
#[cfg(target_os = "ios")]
fn ios_typing() -> Task<Message> {
    worker::after(1500, ()).then(|()| {
        window::latest().then(|id| match id {
            Some(id) => window::run(id, |w| {
                report::note(format!(
                    "ios safeAreaInsets (top, left, bottom, right) = {:?}",
                    crate::ios::safe_area_insets(w)
                ));
                crate::ios::insert_text(w, "hunter2\n")
            })
            .map(|ok| {
                report::note(format!("ios insertText hook ran={ok}"));
                Message::Noop
            }),
            None => Task::none(),
        })
    })
}

#[cfg(not(target_os = "ios"))]
fn ios_typing() -> Task<Message> {
    Task::none()
}

#[cfg(target_os = "ios")]
fn ios_orientation(landscape: bool) -> Task<Message> {
    window::latest().then(move |id| match id {
        Some(id) => {
            window::run(id, move |_| crate::ios::request_orientation(landscape)).map(move |ok| {
                report::note(format!(
                    "ios orientation request landscape={landscape} sent={ok}"
                ));
                Message::Noop
            })
        }
        None => Task::none(),
    })
}

#[cfg(not(target_os = "ios"))]
fn ios_orientation(_landscape: bool) -> Task<Message> {
    Task::none()
}

fn write_png(path: &std::path::Path, shot: &window::Screenshot) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        shot.size.width,
        shot.size.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&shot.rgba)
        .map_err(|e| e.to_string())
}
