//! The screens (docs/SCREENS.md), each a function of the [`Model`].

mod first_run;
mod wallet;

use iced::widget::{column, container};
use iced::{Element, Length};

use crate::app::{Message, Model, Screen};
use crate::theme::{self, color, space};
use crate::ui::{self, ty};

/// The whole window for `model`.
pub fn view(model: &Model) -> Element<'_, Message> {
    let page = if let Some(panicked) = model.stopped {
        stopped(panicked)
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

/// The worker has stopped: nothing more can be done in this run.
fn stopped<'a>(panicked: bool) -> Element<'a, Message> {
    let body = if panicked {
        "The wallet's worker stopped because of a fault. Every secret it held was erased on \
         the way out: the store's key, its seed and any phrase being shown. Nothing was \
         written that had not been. Close Tawara and start it again."
    } else {
        "The wallet's worker could not start, or has stopped. Close Tawara and start it \
         again."
    };
    container(
        ui::card(
            column![
                ui::t(
                    "THE WALLET STOPPED",
                    ty::ONBOARDING_TITLE,
                    color::TEXT_PRIMARY
                ),
                ui::t(body, ty::INTRO, color::TEXT_SECONDARY),
            ]
            .spacing(space::S12),
        )
        .max_width(520.0),
    )
    .center(Length::Fill)
    .into()
}
