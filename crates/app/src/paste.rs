//! Paste on a phone: the Paste button beside the fields a person pastes into,
//! and the restore field's cleaning of a pasted recovery phrase
//! (docs/DECISIONS.md D32 item 10 (g) and (h)).
//!
//! On a phone iced's fields have no edit menu, so a paste reaches them only
//! from a hardware keyboard (D32 item 9 (g)). In the compact layout a Paste
//! button stands beside each destination field (W4, W8), the signed-bytes
//! field (W9) and the recovery-phrase field (S6). It reads the platform's
//! clipboard through iced, only on that tap, and what it read replaces what
//! the field held; an empty or refused read changes nothing.
//!
//! iced's `text_input` drops every control character of a paste before any
//! hook sees it, so a phrase copied one word to a line would arrive as one
//! long word. The restore field alone is wrapped in `cleaned`, which hands
//! the field a clipboard whose read returns the cleaned text.

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, clipboard, overlay};
use iced::{Element, Event, Length, Rectangle, Size, Vector, mouse};

use crate::app::wipe;
use crate::theme;
use crate::ui;

/// `text` as a recovery phrase is sent and validated: every run of
/// whitespace, line breaks and tabs among it, made one space, and the ends
/// trimmed (docs/DECISIONS.md D32 item 10 (h)). The words themselves are not
/// changed. The result is built in one allocation, so no copy of the phrase
/// is left behind by a buffer that grew.
#[must_use]
pub fn phrase(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// What a Paste button's read gives a field other than the phrase: the text
/// as iced's own paste would insert it, with the control characters left
/// out, or `None` when nothing but whitespace was read, so that the field
/// does not change.
#[must_use]
pub fn plain(text: &str) -> Option<String> {
    let out: String = text.chars().filter(|c| !c.is_control()).collect();
    (!out.trim().is_empty()).then_some(out)
}

/// The Paste button beside a field in the compact layout: disabled when
/// `on_press` is `None`, as the field beside it is while the page waits.
pub fn button<'a, M: Clone + 'a>(on_press: Option<M>) -> Element<'a, M> {
    ui::button_with(
        "Paste",
        theme::Button::Secondary,
        ui::Size::Medium,
        None,
        on_press,
    )
    .into()
}

/// The clipboard the restore field is handed: its reads come back as
/// [`phrase`] cleans them, and the text read is wiped. Writes (a copy from
/// the field, which a secure field refuses anyway) pass through.
struct Cleaning<'a>(&'a mut dyn Clipboard);

impl Clipboard for Cleaning<'_> {
    fn read(&self, kind: clipboard::Kind) -> Option<String> {
        let mut read = self.0.read(kind)?;
        let cleaned = phrase(&read);
        wipe(&mut read);
        Some(cleaned)
    }

    fn write(&mut self, kind: clipboard::Kind, contents: String) {
        self.0.write(kind, contents);
    }
}

/// `field`, the restore field, with what keyboard paste reads cleaned as
/// [`phrase`] cleans it before iced inserts it at the caret
/// (docs/DECISIONS.md D32 item 10 (h)). It draws, lays out and handles
/// every event as the field does; only the clipboard it is handed differs.
pub(crate) fn cleaned<'a, M: 'a>(field: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(Cleaned {
        field: field.into(),
    })
}

struct Cleaned<'a, M> {
    field: Element<'a, M>,
}

impl<'a, M: 'a> Widget<M, iced::Theme, iced::Renderer> for Cleaned<'a, M> {
    fn size(&self) -> Size<Length> {
        self.field.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.field.as_widget().size_hint()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.field)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.field));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.field
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.field.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.field
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.field.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            &mut Cleaning(clipboard),
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.field.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, iced::Renderer>> {
        self.field.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Driver;
    use iced::keyboard::key::{NativeCode, Physical};
    use iced::keyboard::{self, Key, Location, Modifiers};
    use iced::widget::text_input;

    #[test]
    fn a_phrase_has_its_line_breaks_tabs_and_runs_of_spaces_made_single_spaces() {
        assert_eq!(
            phrase("  abandon\nability\r\n\table  about\n\nabove \n"),
            "abandon ability able about above"
        );
        assert_eq!(phrase("abandon ability"), "abandon ability", "unchanged");
        assert_eq!(phrase(" \n\t "), "", "nothing but whitespace");
    }

    #[test]
    fn a_plain_paste_keeps_what_iced_would_insert_and_changes_nothing_for_blanks() {
        assert_eq!(
            plain("4f2a\n").as_deref(),
            Some("4f2a"),
            "the line break a saved artifact ends with is left out"
        );
        assert_eq!(plain(" \n\t"), None);
        assert_eq!(plain(""), None);
    }

    /// A clipboard that holds `0`.
    struct Holding(&'static str);

    impl Clipboard for Holding {
        fn read(&self, _kind: clipboard::Kind) -> Option<String> {
            Some(self.0.to_owned())
        }

        fn write(&mut self, _kind: clipboard::Kind, _contents: String) {}
    }

    /// The key a paste is made with: V with the platform's command key.
    fn paste_key() -> Event {
        let v = Key::Character("v".into());
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: v.clone(),
            modified_key: v,
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::COMMAND,
            text: None,
            repeat: false,
        })
    }

    /// What a focused field, empty, publishes when `copied` is pasted into it
    /// from the keyboard, wrapped in [`cleaned`] or not.
    fn pasted(copied: &'static str, wrapped: bool) -> Vec<String> {
        let input = text_input("", "").on_input(|text| text);
        let field: Element<'_, String> = if wrapped {
            cleaned(input)
        } else {
            input.into()
        };
        let mut driver = Driver::new(field, (300, 60));
        driver.focus_next();
        let modifiers = Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND));
        driver.send_with(&[modifiers, paste_key()], &mut Holding(copied))
    }

    #[test]
    fn keyboard_paste_into_the_restore_field_keeps_the_words_apart() {
        let copied = "abandon\nability\nable\n";
        assert_eq!(
            pasted(copied, false),
            ["abandonabilityable"],
            "iced alone joins the words: the case the wrapper is for"
        );
        assert_eq!(pasted(copied, true), ["abandon ability able"]);
    }

    #[test]
    fn the_wrapper_draws_exactly_what_the_field_draws() {
        let field = || text_input("Recovery phrase", "").width(Length::Fixed(200.0));
        let bare = Driver::<()>::new(field(), (300, 60)).draw();
        let wrapped = Driver::<()>::new(cleaned(field()), (300, 60)).draw();
        assert!(bare == wrapped, "the wrapper changed the drawing");
    }
}
