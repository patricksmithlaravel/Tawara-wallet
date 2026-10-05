//! The screens (docs/SCREENS.md), each a function of the [`Model`].

mod first_run;
mod wallet;

use iced::widget::{column, container, progress_bar, scrollable};
use iced::{Element, Length};
use tawara_wallet_core::Activity;

use crate::app::{Busy, Message, Model, Screen};
use crate::theme::{self, color, space};
use crate::ui::{self, t, ty};

/// The whole window for `model`.
pub fn view(model: &Model) -> Element<'_, Message> {
    let page = if let Some(panicked) = model.stopped {
        stopped(model, panicked)
    } else {
        match &model.screen {
            Screen::Wallet(page) => wallet::view(model, page),
            _ => first_run::view(model),
        }
    };
    container(page)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::page)
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
    let mut parts = column![ui::card(
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
    container(scrollable(
        container(parts.max_width(640.0)).center_x(Length::Fill),
    ))
    .center(Length::Fill)
    .into()
}
