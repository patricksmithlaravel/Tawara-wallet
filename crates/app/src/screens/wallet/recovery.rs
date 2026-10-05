//! W12, account recovery: the acknowledged advance, on the owner's terms
//! (docs/DECISIONS.md D19; docs/SCREENS.md W12).
//!
//! Reached on purpose, from a paused account's page or from Settings, and
//! never offered as a button on a refusal. It shows the library's whole
//! report for every account in the store before anything else, because the
//! evidence that one account's advance is wrong is most often in another's
//! report. The person types the index the report names (nothing fills it
//! in) and confirms that no other wallet uses this recovery phrase; then
//! the library advances, only to the index its live report names, or
//! refuses. An account the chain was not found for among the keys searched
//! can be searched for further instead.

use iced::widget::text::Wrapping;
use iced::widget::{checkbox, column, container, row, text_input};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::AccountReport;

use super::{action, frame, kind, name, report, row_of, status};
use crate::app::{Message, Model, RecoveryPage, Remedy, ReportKey, WalletMsg, WalletPage, remedy};
use crate::theme::{self, color, radius, space as sp};
use crate::ui::{self, Size, t, ty};

pub fn view<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    r: &'a RecoveryPage,
) -> Element<'a, Message> {
    let mut body = vec![ui::warning_callout(
        "Nothing here moves a key index until you say so",
        "Read every account's report first. An advance moves an account forward to the key the \
         chain shows, once you have typed that index and confirmed that no other wallet uses \
         this recovery phrase. The library warns that advancing can destroy keys when a second \
         wallet is live on the same phrase.",
    )];
    match &r.reports {
        None => body.push(
            ui::card(ui::helper(if model.busy.is_some() {
                "Reading every account from the chain…"
            } else {
                "Not read yet: Read again asks the chain about every account."
            }))
            .into(),
        ),
        Some(reports) => {
            let mut list = column![ui::section_label("What the chain shows for every account")]
                .spacing(sp::S12);
            for (n, a) in reports.iter().enumerate() {
                list = list.push(account(model, page, r, n, a));
            }
            body.push(list.into());
            if let Some(target) = r.target
                && let Some(a) = reports.iter().find(|a| a.account == target)
                && let Some(m) = remedy(&a.state)
            {
                body.push(act(model, r, a, m));
            } else if !reports.iter().any(|a| remedy(&a.state).is_some()) {
                body.push(
                    ui::card(ui::helper(
                        "No account's report names an index to advance to, and none was \
                         missed among the keys searched: there is nothing to recover here.",
                    ))
                    .into(),
                );
            }
        }
    }
    if let Some((ok, text)) = &r.result {
        body.push(report::show(
            page,
            ReportKey::Advanced,
            report::advanced(*ok, text),
        ));
    }
    frame(
        model,
        page,
        "Account recovery",
        "Every account's report first; nothing moves until you type the index and confirm",
        vec![action(
            model,
            "Read again",
            theme::Button::Secondary,
            Size::Header,
            Some(crate::icon::Icon::Refresh),
            WalletMsg::ReviewAll,
        )],
        body,
    )
}

/// One account: its name and state, its report whole, and the choice to
/// act on it when the report offers something.
fn account<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    r: &'a RecoveryPage,
    n: usize,
    a: &'a AccountReport,
) -> Element<'a, Message> {
    let (word, ink) = status(&a.state);
    let kind_word = row_of(model, a.account).map_or_else(String::new, kind);
    let mut head = row![
        column![
            t(name(a.account), ty::ROW_TITLE, color::TEXT_PRIMARY),
            t(kind_word, ty::TINY, color::TEXT_MUTED),
        ]
        .spacing(sp::S2)
        .width(Length::Fill),
        row![ui::dot(ink, 6.0), t(word, ty::TABLE_BODY, ink)]
            .spacing(sp::S6)
            .align_y(Alignment::Center),
    ]
    .spacing(sp::S12)
    .align_y(Alignment::Center);
    if let Some(m) = remedy(&a.state) {
        head = head.push(if r.target == Some(a.account) {
            Element::from(t("Chosen: see below", ty::LINK_SMALL, color::WARNING))
        } else {
            action(
                model,
                match m {
                    Remedy::Advance(_) => "Advance it",
                    Remedy::SearchFurther => "Search further",
                },
                theme::Button::WarningOutline,
                Size::Small,
                None,
                WalletMsg::Target(a.account),
            )
        });
    }
    let key = ReportKey::Review(u16::try_from(n).unwrap_or(u16::MAX));
    ui::card(column![head, report::show(page, key, report::review(a))].spacing(sp::S12))
        .padding(sp::S18)
        .into()
}

/// What can be done for the chosen account: the advance, typed and
/// confirmed, or a further search.
fn act<'a>(
    model: &'a Model,
    r: &'a RecoveryPage,
    a: &'a AccountReport,
    m: Remedy,
) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let mut field = text_input("", &r.index)
        .font(ty::MONO.font)
        .size(ty::MONO.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .width(Length::Fixed(160.0))
        .style(theme::field(radius::R12));
    if idle {
        field = field.on_input(|s| WalletMsg::RecoveryIndex(s).into());
    }
    let card = match m {
        Remedy::Advance(_) => {
            let mut confirm = checkbox(r.confirmed)
                .label("No other wallet uses this recovery phrase")
                .font(crate::fonts::BODY)
                .text_size(13);
            if idle {
                confirm = confirm.on_toggle(|on| WalletMsg::Confirm(on).into());
            }
            column![
                t(
                    format!("Advance {}", name(a.account)),
                    ty::CARD_TITLE,
                    color::TEXT_PRIMARY
                ),
                t(
                    "Type the key index its report names. The library advances only to exactly \
                     that index, and every key below it is retired for good.",
                    ty::BODY_SMALL,
                    color::TEXT_SECONDARY,
                )
                .wrapping(Wrapping::WordOrGlyph),
                row![
                    t(
                        "Advance to key index",
                        ty::FORM_LABEL,
                        color::TEXT_SECONDARY
                    ),
                    field,
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center),
                confirm,
                action(
                    model,
                    "Advance",
                    theme::Button::WarningPrimary,
                    Size::Medium,
                    None,
                    WalletMsg::Advance,
                ),
            ]
        }
        Remedy::SearchFurther => column![
            t(
                format!("Search further for {}", name(a.account)),
                ty::CARD_TITLE,
                color::TEXT_PRIMARY
            ),
            t(
                "The chain's address for it was not among the keys searched. A wider search \
                 reads every key up to the index you type: about 1.6 ms each, and it can be \
                 cancelled. Nothing is written.",
                ty::BODY_SMALL,
                color::TEXT_SECONDARY,
            )
            .wrapping(Wrapping::WordOrGlyph),
            row![
                t("Search to key index", ty::FORM_LABEL, color::TEXT_SECONDARY),
                field,
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center),
            action(
                model,
                "Search",
                theme::Button::Secondary,
                Size::Medium,
                None,
                WalletMsg::SearchFurther,
            ),
        ],
    };
    container(card.spacing(sp::S14))
        .padding(sp::S24)
        .width(Length::Fill)
        .style(theme::callout_warning)
        .into()
}
