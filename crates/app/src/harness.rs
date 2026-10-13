//! The test harness: an interface built from an element at a size and driven
//! by hand, with events, operations and drawing, as the runtime drives one.
//! The tests of the widgets and the screens use it, and so does the layout
//! check the phone screens are held to: every node of a screen at 360 logical
//! pixels must lie within the width (docs/DECISIONS.md D32 item 10 (b)).
//!
//! The pointer and the fingers are given in window coordinates, as the shell
//! gives them. Each event carries its position where it has one, and the
//! cursor the interface sees is that position, which is what a scrollable
//! reads a finger's drag from.

use std::borrow::Cow;

use iced::advanced::clipboard;
use iced::advanced::graphics;
use iced::advanced::layout;
use iced::advanced::renderer::{self, Headless};
use iced::advanced::widget::operation::{Outcome, focusable};
use iced::advanced::widget::{Operation, Tree};
use iced::event::Status;
use iced::theme::Base;
use iced::{Element, Event, Point, Rectangle, Size, mouse, touch, window};
use iced_runtime::user_interface::{Cache, State, UserInterface};

use crate::{fonts, theme};

/// The renderer the tests draw with: the software renderer, with the bundled
/// faces loaded as the screenshot example loads them.
pub(crate) fn renderer() -> iced::Renderer {
    static FONTS: std::sync::Once = std::sync::Once::new();
    FONTS.call_once(|| {
        if let Ok(mut font_system) = graphics::text::font_system().write() {
            for bytes in fonts::FILES {
                font_system.load_font(Cow::Borrowed(bytes));
            }
        }
    });
    iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        fonts::BODY,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("the software renderer starts")
}

/// An interface built from `root` at `size`, and the messages it produces
/// when it is driven.
pub(crate) struct Driver<'a, M> {
    ui: UserInterface<'a, M, iced::Theme, iced::Renderer>,
    renderer: iced::Renderer,
    size: (u32, u32),
    cursor: Option<Point>,
}

impl<'a, M: 'a> Driver<'a, M> {
    /// The interface for `root` in a window of `size`, with nothing under
    /// the pointer yet.
    pub(crate) fn new(root: impl Into<Element<'a, M>>, size: (u32, u32)) -> Self {
        let mut renderer = renderer();
        let bounds = Size::new(size.0 as f32, size.1 as f32);
        let ui = UserInterface::build(root, bounds, Cache::default(), &mut renderer);
        Driver {
            ui,
            renderer,
            size,
            cursor: None,
        }
    }

    /// Hands `events` to the interface one at a time, each with the cursor
    /// where the events put it. Returns the messages they produced, the
    /// status each event got, and the interface's state after the last.
    pub(crate) fn send(&mut self, events: &[Event]) -> (Vec<M>, Vec<Status>, State) {
        let mut messages = Vec::new();
        let mut statuses = Vec::new();
        let mut state = State::Outdated;
        for event in events {
            if let Some(at) = position_of(event) {
                self.cursor = Some(at);
            }
            let cursor = self
                .cursor
                .map_or(mouse::Cursor::Unavailable, mouse::Cursor::Available);
            let (next, status) = self.ui.update(
                std::slice::from_ref(event),
                cursor,
                &mut self.renderer,
                &mut clipboard::Null,
                &mut messages,
            );
            statuses.extend(status);
            state = next;
        }
        (messages, statuses, state)
    }

    /// Hands `events` to the interface one at a time with `clipboard` as the
    /// platform's, and returns the messages they produced.
    pub(crate) fn send_with(
        &mut self,
        events: &[Event],
        clipboard: &mut dyn iced::advanced::Clipboard,
    ) -> Vec<M> {
        let mut messages = Vec::new();
        for event in events {
            let cursor = self
                .cursor
                .map_or(mouse::Cursor::Unavailable, mouse::Cursor::Available);
            let _ = self.ui.update(
                std::slice::from_ref(event),
                cursor,
                &mut self.renderer,
                clipboard,
                &mut messages,
            );
        }
        messages
    }

    /// Puts the pointer at `(x, y)` without an event, as when an interface is
    /// drawn new under a pointer that does not move: the widgets see the
    /// cursor there, and no move of it.
    pub(crate) fn place_pointer(&mut self, x: f32, y: f32) {
        self.cursor = Some(Point::new(x, y));
    }

    /// Runs `operation` over the interface to its end, as the runtime does.
    pub(crate) fn operate(&mut self, operation: Box<dyn Operation>) {
        let mut next = Some(operation);
        while let Some(mut operation) = next.take() {
            self.ui.operate(&self.renderer, operation.as_mut());
            if let Outcome::Chain(chained) = operation.finish() {
                next = Some(chained);
            }
        }
    }

    /// Gives Tab's first focus: the first focusable widget the keys reach.
    pub(crate) fn focus_next(&mut self) {
        self.operate(Box::new(focusable::focus_next()));
    }

    /// How many focusable widgets the interface holds, and which of them is
    /// focused by its index in Tab's order.
    pub(crate) fn focusable(&mut self) -> (usize, Option<usize>) {
        let mut counted = Focusables::default();
        self.ui.operate(&self.renderer, &mut counted);
        (counted.total, counted.focused)
    }

    /// Where the focused widget is laid out, in the window as it would be
    /// with nothing scrolled, if one is focused.
    pub(crate) fn focused_bounds(&mut self) -> Option<Rectangle> {
        let mut counted = Focusables::default();
        self.ui.operate(&self.renderer, &mut counted);
        counted.bounds
    }

    /// The window as RGBA pixels, drawn the way the screenshot example draws
    /// a screen.
    pub(crate) fn draw(&mut self) -> Vec<u8> {
        let theme = theme::theme();
        let style = theme.base();
        let _ = self.ui.update(
            &[redraw(iced::time::Instant::now())],
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut clipboard::Null,
            &mut Vec::new(),
        );
        self.ui.draw(
            &mut self.renderer,
            &theme,
            &renderer::Style {
                text_color: style.text_color,
            },
            mouse::Cursor::Unavailable,
        );
        self.renderer.screenshot(
            Size::new(self.size.0, self.size.1),
            1.0,
            style.background_color,
        )
    }
}

/// The focusable widgets an interface holds, counted in the order Tab goes
/// through them, and the index and the bounds of the focused one.
#[derive(Default)]
struct Focusables {
    total: usize,
    focused: Option<usize>,
    bounds: Option<Rectangle>,
}

impl iced::advanced::widget::Operation for Focusables {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        operate(self);
    }

    fn focusable(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn iced::advanced::widget::operation::Focusable,
    ) {
        if state.is_focused() {
            self.focused = Some(self.total);
            self.bounds = Some(bounds);
        }
        self.total += 1;
    }
}

/// What a window draws as text, and where each text lies: the layout's own
/// answer, so that a test finds a row or a button by what it says.
#[derive(Default)]
struct Texts(Vec<(String, Rectangle)>);

impl Operation for Texts {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn text(&mut self, _: Option<&iced::advanced::widget::Id>, bounds: Rectangle, text: &str) {
        self.0.push((text.to_owned(), bounds));
    }
}

/// Every text `root` draws in a window of `size`, unscrolled.
pub(crate) fn texts<'a, M: 'a>(
    root: impl Into<Element<'a, M>>,
    size: (f32, f32),
) -> Vec<(String, Rectangle)> {
    let mut renderer = renderer();
    let bounds = Size::new(size.0, size.1);
    let mut ui = UserInterface::build(root, bounds, Cache::default(), &mut renderer);
    let mut found = Texts::default();
    ui.operate(&renderer, &mut found);
    found.0
}

/// The window position an event carries, if it carries one.
fn position_of(event: &Event) -> Option<Point> {
    match event {
        Event::Mouse(mouse::Event::CursorMoved { position })
        | Event::Touch(
            touch::Event::FingerPressed { position, .. }
            | touch::Event::FingerMoved { position, .. }
            | touch::Event::FingerLifted { position, .. }
            | touch::Event::FingerLost { position, .. },
        ) => Some(*position),
        _ => None,
    }
}

/// The pointer moves to `(x, y)`.
pub(crate) fn mouse_move(x: f32, y: f32) -> Event {
    Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(x, y),
    })
}

/// The left button goes down where the pointer is.
pub(crate) fn mouse_press() -> Event {
    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}

/// The left button comes up where the pointer is.
pub(crate) fn mouse_release() -> Event {
    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}

/// Finger `id` goes down at `(x, y)`.
pub(crate) fn finger_press(id: u64, x: f32, y: f32) -> Event {
    Event::Touch(touch::Event::FingerPressed {
        id: touch::Finger(id),
        position: Point::new(x, y),
    })
}

/// Finger `id` moves to `(x, y)`.
pub(crate) fn finger_move(id: u64, x: f32, y: f32) -> Event {
    Event::Touch(touch::Event::FingerMoved {
        id: touch::Finger(id),
        position: Point::new(x, y),
    })
}

/// Finger `id` comes up at `(x, y)`.
pub(crate) fn finger_lift(id: u64, x: f32, y: f32) -> Event {
    Event::Touch(touch::Event::FingerLifted {
        id: touch::Finger(id),
        position: Point::new(x, y),
    })
}

/// The window is drawn at `at`, as the shell asks it each frame.
pub(crate) fn redraw(at: iced::time::Instant) -> Event {
    Event::Window(window::Event::RedrawRequested(at))
}

/// Every node of `root` whose right edge passes `width` plus half a pixel
/// when it is laid out at `width` by 844, as a window of that width would
/// lay it out, with `Ok` when none does. Each node's position is its parent's
/// plus its own, so the check is of what is drawn.
pub(crate) fn fits<'a, M: 'a>(
    root: impl Into<Element<'a, M>>,
    width: f32,
) -> Result<(), Vec<Rectangle>> {
    let mut element: Element<'a, M> = root.into();
    let renderer = renderer();
    let mut tree = Tree::new(element.as_widget());
    let limits = layout::Limits::new(Size::ZERO, Size::new(width, 844.0));
    let node = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &limits);
    let mut past = Vec::new();
    overflow(&node, Point::ORIGIN, width, &mut past);
    if past.is_empty() { Ok(()) } else { Err(past) }
}

/// Collects, under `node` at `origin`, every node whose right edge passes
/// `width` plus half a pixel.
fn overflow(node: &layout::Node, origin: Point, width: f32, past: &mut Vec<Rectangle>) {
    for child in node.children() {
        let bounds = child.bounds();
        let at = Point::new(origin.x + bounds.x, origin.y + bounds.y);
        let absolute = Rectangle::new(at, bounds.size());
        if absolute.x + absolute.width > width + 0.5 {
            past.push(absolute);
        }
        overflow(child, at, width, past);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Length;
    use iced::widget::{scrollable, space};

    #[test]
    fn fits_reports_a_node_laid_out_past_the_width_and_passes_one_within_it() {
        // iced clamps a node to the room its parent gives it, so a node past the
        // edge needs a layout that does not clamp: the content of a horizontal
        // scrollable is laid out at its full width.
        let wide: Element<'_, ()> = scrollable(
            space()
                .width(Length::Fixed(500.0))
                .height(Length::Fixed(10.0)),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .into();
        let past = fits(wide, 360.0).expect_err("a node past the width");
        assert!(past.iter().any(|r| r.x + r.width > 360.5), "{past:?}");

        let within: Element<'_, ()> = space()
            .width(Length::Fixed(300.0))
            .height(Length::Fixed(10.0))
            .into();
        assert_eq!(fits(within, 360.0), Ok(()));
    }
}
