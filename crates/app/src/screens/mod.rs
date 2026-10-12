//! The screens (docs/SCREENS.md), each a function of the [`Model`].

mod first_run;
mod wallet;

use iced::widget::{column, container, progress_bar, scrollable};
use iced::{Element, Length};
use tawara_wallet_core::Activity;

use crate::app::{Busy, Message, Model, Screen};
use crate::keyboard;
use crate::theme::{self, color, space};
use crate::ui::{self, t, ty};

/// The page's scrollable in the phone layout: the one the screenshot example
/// scrolls to its end (docs/DECISIONS.md D32 item 10 (b)).
pub fn page_id() -> iced::widget::Id {
    iced::widget::Id::from("page")
}

/// The whole window for `model`. The safe area pads the root in every layout:
/// it is zero on the desktop, and a phone's system bars and keyboard are kept
/// clear of the content (docs/DECISIONS.md D32 item 10 (b)).
pub fn view(model: &Model) -> Element<'_, Message> {
    let page = if let Some(panicked) = model.stopped {
        stopped(model, panicked)
    } else {
        match &model.screen {
            Screen::Wallet(page) => wallet::view(model, page),
            _ => first_run::view(model),
        }
    };
    // On a phone the page is wrapped once, inside the safe area's padding
    // applied below. The wrapper draws nothing of its own, and it asks for the
    // keyboard again on a tap of a focused field while the keyboard is hidden
    // (docs/DECISIONS.md D32 item 10 (e)). The desktop has no soft keyboard,
    // and a touch screen there would have the wrapper withhold a frame's
    // request and end an input method's composition, so its page is not
    // wrapped.
    let page = if model.phone() {
        keyboard::reshow(page, model.safe_area.keyboard > 0.0)
    } else {
        page
    };
    // A compact wallet's bars are the chrome colour, and the strips the system
    // covers above and below them take the same colour, so they read as part
    // of the bars.
    let chrome =
        model.compact() && model.stopped.is_none() && matches!(model.screen, Screen::Wallet(_));
    let style: fn(&iced::Theme) -> iced::widget::container::Style =
        if chrome { theme::sidebar } else { theme::page };
    container(page)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(model.safe_area.padding(0.0))
        .style(style)
        .into()
}

/// What the worker is doing for the command the screen waits on, as a title
/// and a sentence: S8, and the wallet page while it refreshes.
fn activity(busy: &Busy) -> (&'static str, &'static str) {
    match busy.activity {
        None => (
            "Waiting",
            "The wallet does one thing at a time, and it is finishing what came before, \
             usually a question to the node. This starts when that is done; Cancel stops \
             both.",
        ),
        Some(Activity::DerivingKey) => (
            "Opening the store",
            "Deriving the store's key from the password. It takes a few seconds and 64 MiB \
             of memory on purpose: that is what makes guessing a password expensive.",
        ),
        Some(Activity::AskingNode) => (
            "Reconciling",
            "Asking the node about each account and comparing the key index this store holds \
             with the one the chain shows.",
        ),
        Some(Activity::ReadingIndex) => (
            "Reading the node's index",
            "Asking the node's transaction index about each account, one request each. Nothing \
             is written.",
        ),
    }
}

/// How far the command has got, as the library counts it, once it has
/// said.
fn progress<'a>(busy: &Busy) -> Option<Element<'a, Message>> {
    let p = busy.progress?;
    let mut detail = column![t(
        format!("Account {} of {}", p.account + 1, p.accounts),
        ty::ROW_TITLE,
        color::TEXT_PRIMARY,
    )]
    .spacing(space::S8);
    if p.ceiling > 0 {
        // Positions are counted in `u32`; a bar needs only their ratio.
        #[allow(clippy::cast_precision_loss)]
        let (position, ceiling) = (p.position as f32, p.ceiling as f32);
        detail = detail
            .push(
                progress_bar(0.0..=ceiling, position)
                    .girth(4)
                    .style(theme::progress),
            )
            .push(ui::helper(format!(
                "{} of at most {} key positions searched",
                ui::group(&p.position.to_string()),
                ui::group(&p.ceiling.to_string())
            )));
    }
    Some(detail.into())
}

/// The worker has stopped: nothing more can be done in this run, apart from
/// saving the bytes of the spends signed in it.
fn stopped(model: &Model, panicked: bool) -> Element<'_, Message> {
    let body = if panicked {
        "The wallet's worker stopped because of a fault. Every secret it held was erased on \
         the way out: the store's key, its seed and any phrase being shown. Nothing was \
         written that had not been. Close Tawara and start it again."
    } else {
        "The wallet's worker could not start, or has stopped. Close Tawara and start it \
         again."
    };
    let mut parts = column![ui::card_for(
        model.compact(),
        column![
            ui::t(
                "THE WALLET STOPPED",
                ty::ONBOARDING_TITLE,
                color::TEXT_PRIMARY
            ),
            ui::t(body, ty::INTRO, color::TEXT_SECONDARY),
        ]
        .spacing(space::S12),
    )]
    .spacing(space::S16);
    if let Some(kept) = wallet::kept_after_stop(model) {
        parts = parts.push(kept);
    }
    if model.compact() {
        // The phone: the cards fill the width between 16-pixel margins.
        return scrollable(container(parts).padding(space::S16).width(Length::Fill))
            .id(page_id())
            .style(theme::scroll)
            .height(Length::Fill)
            .into();
    }
    container(scrollable(
        container(parts.max_width(640.0)).center_x(Length::Fill),
    ))
    .center(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iced::advanced::layout;
    use iced::advanced::widget::Tree;
    use iced::mobile::SafeArea;
    use iced::{Padding, Point, Rectangle, Size};
    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::sample;

    use super::*;
    use crate::app::{Page, UnlockForm, WalletPage};
    use crate::harness::{Driver, finger_lift, finger_press, redraw};
    use iced::time::Instant;
    use iced_runtime::user_interface::State;

    /// The phone's safe area: the status bar above, the home indicator below.
    fn phone_area(keyboard: f32) -> SafeArea {
        SafeArea::new(Padding {
            top: 47.0,
            right: 0.0,
            bottom: 34.0,
            left: 0.0,
        })
        .with_keyboard(keyboard)
    }

    /// The sample store on the dashboard, in a phone of 390 by 844 with
    /// `keyboard` showing.
    fn phone_wallet(keyboard: f32) -> Model {
        let mut model = Model::new(Preferences::default(), None);
        model.width = 390.0;
        model.safe_area = phone_area(keyboard);
        model.wallet = Some(sample::wallet_view());
        model.screen = Screen::Wallet(WalletPage {
            page: Page::Dashboard,
            ..WalletPage::default()
        });
        model
    }

    /// The absolute bounds of every node of the window for `model` at 390 by
    /// 844, the root first.
    fn boxes(model: &Model) -> Vec<Rectangle> {
        let mut element = view(model);
        let renderer = crate::harness::renderer();
        let mut tree = Tree::new(element.as_widget());
        let limits = layout::Limits::new(Size::ZERO, Size::new(390.0, 844.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let mut out = vec![node.bounds()];
        collect(&node, Point::ORIGIN, &mut out);
        out
    }

    fn collect(node: &layout::Node, origin: Point, out: &mut Vec<Rectangle>) {
        for child in node.children() {
            let bounds = child.bounds();
            let at = Point::new(origin.x + bounds.x, origin.y + bounds.y);
            out.push(Rectangle::new(at, bounds.size()));
            collect(child, at, out);
        }
    }

    #[test]
    fn a_phone_wallet_is_drawn_between_its_safe_area_insets() {
        let model = phone_wallet(0.0);
        let mut element = view(&model);
        let renderer = crate::harness::renderer();
        let mut tree = Tree::new(element.as_widget());
        let limits = layout::Limits::new(Size::ZERO, Size::new(390.0, 844.0));
        let root = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let page = &root.children()[0].bounds();
        assert_eq!(page.y, 47.0, "the page starts under the status bar");
        assert_eq!(
            page.y + page.height,
            810.0,
            "and ends above the home indicator"
        );
    }

    #[test]
    fn with_the_keyboard_up_nothing_is_drawn_under_it_and_the_tab_bar_goes() {
        // The page is a scrollable, so what is inside it runs past the window
        // and is clipped: what is checked is the shell, the children of the
        // root's first child, which is the frame of the wallet.
        let shell_bottom = |model: &Model| {
            let mut element = view(model);
            let renderer = crate::harness::renderer();
            let mut tree = Tree::new(element.as_widget());
            let limits = layout::Limits::new(Size::ZERO, Size::new(390.0, 844.0));
            let root = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits);
            let frame = root.children()[0].bounds();
            let mut lowest = frame.y + frame.height;
            for child in root.children()[0].children() {
                let bounds = child.bounds();
                lowest = lowest.max(frame.y + bounds.y + bounds.height);
            }
            lowest
        };
        let lowest = shell_bottom(&phone_wallet(336.0));
        assert!(
            lowest <= 508.0 + 0.5,
            "the frame ends at {lowest}, under the keyboard"
        );
        assert!(
            boxes(&phone_wallet(336.0)).len() < boxes(&phone_wallet(0.0)).len(),
            "the tab bar's nodes are not laid out while the keyboard shows"
        );
    }

    #[test]
    fn the_unlock_form_has_the_folder_field_on_the_desktop_only() {
        let form = || {
            Screen::Unlock(UnlockForm {
                dir: "/a/folder".to_owned(),
                ..UnlockForm::default()
            })
        };
        let mut desktop = Model::new(Preferences::default(), None);
        desktop.screen = form();
        let mut driver = Driver::new(view(&desktop), (1440, 900));
        assert_eq!(driver.focusable().0, 2, "the folder and the password");

        let mut phone = Model::new(Preferences::default(), None);
        phone.confine(PathBuf::from("/private"));
        phone.width = 390.0;
        phone.screen = form();
        let mut driver = Driver::new(view(&phone), (390, 844));
        assert_eq!(driver.focusable().0, 1, "the password alone");
    }

    #[test]
    fn only_a_phone_withholds_a_frames_request_after_a_tap_on_the_focused_field() {
        // The keyboard wrapper is a phone's alone (docs/DECISIONS.md D32 item 10
        // (e)): on the desktop a tap on the focused field, from a touch screen,
        // leaves the field's request for the input method as it was.
        for phone in [true, false] {
            let mut model = Model::new(Preferences::default(), None);
            let size = if phone {
                model.confine(PathBuf::from("/private"));
                model.width = 390.0;
                (390, 844)
            } else {
                (1440, 900)
            };
            model.screen = Screen::Unlock(UnlockForm::default());
            let mut driver = Driver::new(view(&model), size);
            driver.focus_next();
            let at = driver.focused_bounds().expect("a field has the focus");
            let (x, y) = (at.center_x(), at.center_y());
            driver.send(&[finger_press(1, x, y), finger_lift(1, x, y)]);
            let (_, _, state) = driver.send(&[redraw(Instant::now())]);
            let State::Updated { input_method, .. } = state else {
                panic!("the interface was outdated, so it asked for nothing");
            };
            assert_eq!(
                input_method.is_enabled(),
                !phone,
                "phone {phone}: the frame after the tap"
            );
        }
    }

    #[test]
    fn tab_gives_a_phones_unlock_form_its_password_first() {
        // On a phone the password is the only field, so Tab's first focus is on
        // it (docs/DECISIONS.md D32 item 10 (a)).
        let mut phone = Model::new(Preferences::default(), None);
        phone.confine(PathBuf::from("/private"));
        phone.width = 390.0;
        phone.screen = Screen::Unlock(UnlockForm::default());
        let mut driver = Driver::new(view(&phone), (390, 844));
        driver.focus_next();
        assert_eq!(driver.focusable(), (1, Some(0)));
    }
}
