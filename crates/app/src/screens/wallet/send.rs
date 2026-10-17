//! The send flow (rendering 03; docs/SCREENS.md W4 to W6), re-signing the
//! reserved spend (W8), and submitting a saved artifact (W9).
//!
//! The person composes, then reads the library's "NOT SIGNED" page, then
//! signs on a step of its own: there is no password field, because the
//! store is open and the library's page is the confirmation
//! (docs/DECISIONS.md D27, item 3). After signing, the library's page with
//! its three facts is shown whole, and the signed bytes, which the store
//! does not keep, can be saved or copied (D27, item 5).
//!
//! On a phone the composing and the signing are four pages before the sent
//! page: the destinations, the amounts, the review, and the signing
//! (docs/DECISIONS.md D40). The desktop keeps W4 and W5 as one page each.

use std::collections::BTreeMap;

use iced::widget::text::Wrapping;
use iced::widget::{Row, checkbox, column, container, row, space, text_input};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::preferences::AmountUnit;
use tawara_wallet_core::view::{AccountId, AccountState};
use tawara_wallet_core::{MAX_DESTINATIONS, PlanView, PlannedDestination, REFERENCE_RULE};

use super::{
    account_choice, action, beside, copy, destination, figure, frame, frame_beside, name, pair,
    report, row_of, short,
};
use crate::app::{
    Message, Model, PasteInto, RefusedField, ReportKey, ResignPage, SendPage, SendStage, SentPage,
    SpendForm, SubmitMode, SubmitPage, To, WalletMsg, WalletPage,
};
use crate::icon::Icon;
use crate::theme::{self, color, radius, space as sp};
use crate::ui::{self, Size, t, ty};
use crate::{fonts, paste};

/// The node's floor for the fee, per destination, in nanoMCM: what the fee
/// is when the person gives none (`tawara_wallet_core::spend`).
const FEE_FLOOR: u64 = 500;

/// Whether Return in W9's field broadcasts the signed bytes (docs/DECISIONS.md
/// D32 item 9). On the desktop it does, as it always has, since Return is how
/// a keyboard submits there. On a phone the soft keyboard's Return is easy to
/// press straight after a paste, and a broadcast cannot be undone, so Return
/// does nothing there and the Submit button is the only way to send. The
/// button sends the same message and is disabled while busy, as the field is.
///
/// An expression rather than a `cfg` attribute, so that both branches are
/// compiled and type-checked on every target. The tests run on the desktop,
/// so they give the phones' value to [`submit_form`] directly; that this
/// constant is false on Android and iOS is checked by reading it and by the
/// phone builds, not by a test.
const RETURN_BROADCASTS: bool = !cfg!(any(target_os = "android", target_os = "ios"));

/// The page each stage of the send flow is drawn as. On a phone the flow is the
/// four pages of docs/DECISIONS.md D40, and its review is page 3; on the
/// desktop it is W4, W5 and W6 (docs/SCREENS.md), since a desktop does not reach
/// the phone's pages.
pub fn view<'a>(model: &'a Model, page: &'a WalletPage, s: &'a SendPage) -> Element<'a, Message> {
    match &s.stage {
        // A phone is never on W4, whose one page is the desktop's: its first
        // page is drawn in its place.
        SendStage::Compose if model.phone() => to_page(model, page, s, &NO_REFUSALS),
        SendStage::Compose => compose(model, page, s),
        SendStage::To(refused) => to_page(model, page, s, refused),
        SendStage::Amount => amount_page(model, page, s),
        SendStage::Review(plan) if model.phone() => review_page(model, page, plan),
        SendStage::Review(plan) => review(model, page, plan),
        SendStage::Sign(plan) => sign_page(model, page, plan),
        SendStage::Sent(sent) => sent_page(model, page, sent),
    }
}

/// No refusals, for a page drawn without any. A static, since the page
/// borrows what it shows for its whole life.
static NO_REFUSALS: BTreeMap<(usize, RefusedField), String> = BTreeMap::new();

/// The accounts a spend may be laid out from now, in the wallet's order.
fn spendable_of(model: &Model) -> Vec<AccountId> {
    model
        .wallet
        .iter()
        .flat_map(|w| w.accounts.iter())
        .filter(|a| a.spendable)
        .map(|a| a.id)
        .collect()
}

/// Why no spend can be laid out from the account chosen: it is `blocked`, the
/// account asked for, which cannot spend now, or none can (`none_can`). `None`
/// when a spend can be laid out.
fn cannot_spend<'a>(blocked: Option<AccountId>, none_can: bool) -> Option<Element<'a, Message>> {
    if let Some(id) = blocked {
        Some(ui::warning_callout(
            "The account chosen cannot spend now",
            format!(
                "{} spends once the wallet has reconciled it on opening and it is in sync; an \
                 account set aside when the wallet opened spends again after Refresh. Choose \
                 another account, or Refresh the wallet first.",
                name(id)
            ),
        ))
    } else if none_can {
        Some(ui::warning_callout(
            "No account can spend now",
            "A spend needs the wallet open against the node, from an account that reconciled \
             and is in sync. Refresh, or open an account's page from the wallet to see why it \
             cannot.",
        ))
    } else {
        None
    }
}

/// W4: from which account, to whom, how much, with which references.
fn compose<'a>(model: &'a Model, page: &'a WalletPage, s: &'a SendPage) -> Element<'a, Message> {
    let spendable = spendable_of(model);
    // The account asked for, when it cannot spend now: it stays the one
    // chosen, and the page says why, rather than spending from another.
    let blocked = s.from.filter(|f| !spendable.contains(f));
    let mut form = column![ui::section_label("From account")].spacing(sp::S12);
    if let Some(callout) = cannot_spend(blocked, spendable.is_empty()) {
        form = form.push(callout);
    }
    if !spendable.is_empty() {
        form = form.push(account_choice(model, spendable.into_iter(), s.from, |id| {
            WalletMsg::From(id).into()
        }));
    }
    form = form.push(destinations(model, &s.form));
    let summary = column![
        // What signing would do is said only of an account that can sign.
        summary(model, s, s.from.filter(|_| blocked.is_none())),
        key_callout(model, s.from.filter(|_| blocked.is_none())),
        steps(model.compact(), Step::Pending, Step::Pending, Step::Pending),
        ui::button_with(
            "Review spend",
            theme::Button::Primary,
            Size::Xl,
            None,
            (model.busy.is_none() && s.from.is_some() && blocked.is_none())
                .then_some(WalletMsg::Review.into()),
        ),
        ui::helper(
            "Nothing is signed yet: the next step shows the spend as the wallet library lays \
             it out, and asks again.",
        ),
    ]
    .spacing(sp::S16);
    frame(
        model,
        page,
        "Send",
        format!(
            "One transaction, up to {MAX_DESTINATIONS} destinations, each with its own reference."
        ),
        Vec::new(),
        vec![pair(
            model,
            ui::card_for(model.compact(), form.spacing(sp::S20)).into(),
            summary.into(),
            7,
            4,
        )],
    )
}

/// A text field of a phone's page: the field of [`destinations`], in the
/// monospace face for an address or a reference, and in the body's for an
/// amount.
fn entry<'a>(
    value: &'a str,
    placeholder: &'a str,
    mono: bool,
) -> iced::widget::TextInput<'a, Message> {
    let face = if mono { ty::MONO } else { ty::FIELD };
    text_input(placeholder, value)
        .font(face.font)
        .size(face.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .style(theme::field(radius::R12))
}

/// The place a phone's page holds in the flow, "n of 4" (docs/DECISIONS.md
/// D40).
fn place<'a>(n: u8) -> Element<'a, Message> {
    t(format!("{n} of 4"), ty::NOTE, color::TEXT_SECONDARY).into()
}

/// The Back at the top of a phone's page after the first: back one page,
/// keeping what was typed (docs/DECISIONS.md D40 item 2).
fn back<'a>(model: &Model) -> Element<'a, Message> {
    action(
        model,
        "Back",
        theme::Button::Secondary,
        Size::Medium,
        Some(Icon::ChevronLeft),
        WalletMsg::Back,
    )
}

/// Page 1 of 4 on a phone (docs/DECISIONS.md D40 item 1): the account the spend
/// comes from, with "Change" beside it when another account can spend too, and
/// the destinations, each with its Paste, its reference and its Remove; then
/// "Add another destination" and "Continue". What "Continue" found wrong with
/// an address or a reference is shown under that field, and what it refused
/// beside it (docs/DECISIONS.md D40 item 4).
fn to_page<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    s: &'a SendPage,
    refused: &'a BTreeMap<(usize, RefusedField), String>,
) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let compact = model.compact();
    let spendable = spendable_of(model);
    let blocked = s.from.filter(|f| !spendable.contains(f));
    let mut from = column![ui::section_label("From account")].spacing(sp::S12);
    if let Some(id) = s.from {
        let mut line = row![
            column![
                t(name(id), ty::RADIO_TITLE, color::TEXT_PRIMARY),
                t(destination(id), ty::MONO_TINY, color::TEXT_MUTED)
                    .wrapping(Wrapping::WordOrGlyph),
            ]
            .spacing(sp::S2)
            .width(Length::Fill),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center);
        // Another account that can spend is the choice the callout below asks
        // for, when the one chosen cannot (docs/DECISIONS.md D40 item 7 (d)).
        if spendable.iter().any(|other| *other != id) {
            line = line.push(ui::button_with(
                "Change",
                theme::Button::Secondary,
                Size::Small,
                None,
                idle.then(|| Message::Wallet(WalletMsg::ChooseAccount(!page.choosing))),
            ));
        }
        from = from.push(line);
    }
    if let Some(callout) = cannot_spend(blocked, spendable.is_empty()) {
        from = from.push(callout);
    }
    if page.choosing {
        from = from.push(account_choice(
            model,
            spendable.iter().copied(),
            s.from,
            |id| WalletMsg::From(id).into(),
        ));
    }
    let shown = model.form_of().filter(|_| idle);
    let several = s.form.rows.len() > 1;
    let mut list = column![].spacing(sp::S16);
    for (i, r) in s.form.rows.iter().enumerate() {
        if i > 0 {
            list = list.push(ui::divider());
        }
        let mut to = entry(&r.to, "Base58 destination", true).id(crate::screens::destination_id(i));
        let mut reference = entry(&r.reference, "none", true).id(crate::screens::reference_id(i));
        if idle {
            to = to.on_input(move |v| WalletMsg::DestinationTo(i, v).into());
            reference = reference.on_input(move |v| WalletMsg::Reference(i, v).into());
        }
        let remove = ui::button_with(
            "Remove",
            theme::Button::Ghost,
            Size::Small,
            Some(Icon::Trash),
            (idle && several).then(|| Message::Wallet(WalletMsg::RemoveDestination(i))),
        );
        // The Paste button beside the field, in the compact layout only, as
        // [`destinations`] has it (docs/DECISIONS.md D32 item 10 (g)).
        let mut address = row![to.width(Length::Fill)];
        if compact {
            address = address.push(paste::button(shown.map(|(form, rows)| {
                Message::Paste(PasteInto::Destination { form, row: i, rows })
            })));
        }
        let mut block = column![
            row![
                t(
                    format!("Destination {}", i + 1),
                    ty::FORM_LABEL,
                    color::TEXT_SECONDARY
                )
                .width(Length::Fill),
                remove,
            ]
            .align_y(Alignment::Center),
            address
                .spacing(sp::S8)
                .align_y(Alignment::Center)
                .width(Length::Fill),
        ]
        .spacing(sp::S12);
        if let Some(why) = refused.get(&(i, RefusedField::Address)) {
            block = block.push(ui::refusal(why.as_str()));
        }
        block = block.push(
            column![
                t("Reference", ty::FORM_LABEL, color::TEXT_SECONDARY),
                reference
            ]
            .spacing(sp::S6),
        );
        if let Some(why) = refused.get(&(i, RefusedField::Reference)) {
            block = block.push(ui::refusal(why.as_str()));
        }
        list = list.push(block);
    }
    let section = column![
        row![
            ui::section_label("Destinations"),
            space().width(Length::Fill),
            t(
                format!("{} of {MAX_DESTINATIONS}", s.form.rows.len()),
                ty::NOTE,
                color::TEXT_SECONDARY
            ),
        ],
        list,
        ui::button_with(
            "Add another destination",
            theme::Button::Dashed,
            Size::Medium,
            Some(Icon::Plus),
            (idle && s.form.rows.len() < usize::from(MAX_DESTINATIONS))
                .then(|| Message::Wallet(WalletMsg::AddDestination)),
        )
        .width(Length::Fill),
    ]
    .spacing(sp::S12);
    let form = column![
        from,
        section,
        ui::helper(format!(
            "The reference rule, in the wallet library's words: {REFERENCE_RULE}."
        )),
    ]
    .spacing(sp::S20);
    let mut next = column![action(
        model,
        "Continue",
        theme::Button::Primary,
        Size::Xl,
        None,
        WalletMsg::Continue,
    )]
    .spacing(sp::S16);
    for part in beside(model, page) {
        next = next.push(part);
    }
    frame_beside(
        model,
        page,
        "Send",
        format!(
            "One transaction, up to {MAX_DESTINATIONS} destinations, each with its own reference."
        ),
        Vec::new(),
        vec![
            place(1),
            ui::card_for(model.compact(), form).into(),
            next.into(),
        ],
    )
}

/// Page 2 of 4 on a phone (docs/DECISIONS.md D40 item 1): the amount to each
/// destination, "Send everything" for a single one, the fee and the
/// block-to-live, the figures as they stand, and "Review spend", with what it
/// refused beside it.
fn amount_page<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    s: &'a SendPage,
) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let spendable = spendable_of(model);
    let blocked = s.from.filter(|f| !spendable.contains(f));
    let several = s.form.rows.len() > 1;
    let mut amounts = column![ui::section_label("Amounts")].spacing(sp::S12);
    for (i, r) in s.form.rows.iter().enumerate() {
        if i > 0 {
            amounts = amounts.push(ui::divider());
        }
        let amount: Element<'a, Message> = if s.form.everything && !several {
            container(t("everything", ty::FIELD, color::TEXT_MUTED))
                .padding(Padding::from([sp::S12, sp::S14]))
                .width(Length::Fill)
                .style(theme::field_box)
                .into()
        } else {
            let mut amount = entry(&r.amount, "0.000000000", false).align_x(Alignment::End);
            if idle {
                amount = amount.on_input(move |v| WalletMsg::Amount(i, v).into());
            }
            amount.into()
        };
        amounts = amounts.push(
            column![
                row![
                    t(
                        format!("Destination {}", i + 1),
                        ty::FORM_LABEL,
                        color::TEXT_SECONDARY
                    ),
                    space().width(Length::Fill),
                    t(short(&r.to), ty::MONO_SMALL, color::TEXT_MUTED),
                ]
                .align_y(Alignment::Center),
                column![
                    t("Amount (MCM)", ty::FORM_LABEL, color::TEXT_SECONDARY),
                    amount
                ]
                .spacing(sp::S6),
            ]
            .spacing(sp::S12),
        );
    }
    if !several {
        let mut all = checkbox(s.form.everything)
            .label("Send everything: the whole balance less the fee, to this one destination.")
            .font(fonts::BODY)
            .text_size(13);
        if idle {
            all = all.on_toggle(|on| WalletMsg::Everything(on).into());
        }
        amounts = amounts.push(all);
    }
    let mut fee = entry(&s.form.fee, "the node's floor", true);
    let mut btl = entry(&s.form.blk_to_live, "none", true);
    if idle {
        fee = fee.on_input(|v| WalletMsg::Fee(v).into());
        btl = btl.on_input(|v| WalletMsg::BlockToLive(v).into());
    }
    amounts = amounts
        .push(ui::divider())
        .push(
            column![
                t(
                    "Fee, nanoMCM in total",
                    ty::FORM_LABEL,
                    color::TEXT_SECONDARY
                ),
                fee
            ]
            .spacing(sp::S6),
        )
        .push(
            column![
                t("Block-to-live", ty::FORM_LABEL, color::TEXT_SECONDARY),
                btl
            ]
            .spacing(sp::S6),
        )
        .push(ui::helper(format!(
            "The fee is {FEE_FLOOR} nanoMCM per destination, the node's floor, unless you give \
             one. A block-to-live is the last block the spend may land in; none means it never \
             expires."
        )));
    let mut next = column![ui::button_with(
        "Review spend",
        theme::Button::Primary,
        Size::Xl,
        None,
        (idle && s.from.is_some() && blocked.is_none()).then(|| Message::Wallet(WalletMsg::Review)),
    )]
    .spacing(sp::S16);
    if let Some(callout) = cannot_spend(blocked, spendable.is_empty()) {
        next = next.push(callout);
    }
    for part in beside(model, page) {
        next = next.push(part);
    }
    next = next.push(ui::helper(
        "Nothing is signed yet: the next step shows the spend as the wallet library lays it \
         out, and asks again.",
    ));
    frame_beside(
        model,
        page,
        "Send",
        "The amounts, the fee and the block-to-live. Nothing is signed yet.",
        vec![back(model)],
        vec![
            place(2),
            ui::card_for(model.compact(), amounts).into(),
            summary(model, s, s.from.filter(|_| blocked.is_none())),
            next.into(),
        ],
    )
}

/// The destinations as typed, the "everything" choice, and the fee and
/// block-to-live: W4's form and W8's. On a phone each destination is a block
/// of its own, with no table header (docs/SCREENS.md, "Compact layout";
/// docs/DECISIONS.md D32 item 10 (b)).
fn destinations<'a>(model: &'a Model, f: &'a SpendForm) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let compact = model.compact();
    let field = |value: &'a str, placeholder: &'a str, mono: bool| {
        let face = if mono { ty::MONO } else { ty::FIELD };
        text_input(placeholder, value)
            .font(face.font)
            .size(face.size)
            .padding(Padding::from([sp::S12, sp::S14]))
            .style(theme::field(radius::R12))
    };
    let mut list = if compact {
        column![].spacing(sp::S16)
    } else {
        column![
            row![
                t("#", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::Fixed(20.0)),
                t("Destination", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::FillPortion(6)),
                // Typed in MCM whatever unit the wallet shows, exactly to nine
                // places, so a figure is never read in the wrong unit.
                t("Amount (MCM)", ty::TABLE_HEADER, color::TEXT_MUTED)
                    .width(Length::FillPortion(3)),
                t("Reference", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::FillPortion(3)),
                space().width(Length::Fixed(36.0)),
            ]
            .spacing(sp::S12),
        ]
        .spacing(sp::S10)
    };
    let several = f.rows.len() > 1;
    // Each Paste button records the form it was drawn on and its rows, so
    // that a read which comes back late goes nowhere else (docs/DECISIONS.md
    // D32 item 10 (g)).
    let shown = model.form_of().filter(|_| idle);
    for (i, r) in f.rows.iter().enumerate() {
        let mut to = field(&r.to, "Base58 destination", true).id(crate::screens::destination_id(i));
        let mut reference = field(&r.reference, "none", true);
        let amount: Element<'a, Message> = if f.everything && !several {
            container(t("everything", ty::FIELD, color::TEXT_MUTED))
                .padding(Padding::from([sp::S12, sp::S14]))
                .width(Length::Fill)
                .style(theme::field_box)
                .into()
        } else {
            let mut amount = field(&r.amount, "0.000000000", false).align_x(Alignment::End);
            if idle {
                amount = amount.on_input(move |v| WalletMsg::Amount(i, v).into());
            }
            amount.into()
        };
        if idle {
            to = to.on_input(move |v| WalletMsg::DestinationTo(i, v).into());
            reference = reference.on_input(move |v| WalletMsg::Reference(i, v).into());
        }
        let remove =
            iced::widget::button(crate::icon::icon(Icon::Trash, 16.0, 2.0, color::TEXT_MUTED))
                .padding(sp::S10)
                .style(theme::button(theme::Button::Ghost))
                .on_press_maybe(
                    (idle && several).then_some(WalletMsg::RemoveDestination(i).into()),
                );
        if compact {
            // The blocks are divided, with none above the first.
            if i > 0 {
                list = list.push(ui::divider());
            }
            // The destination takes the width, and the amount and the
            // reference sit side by side beneath it, so that a long
            // destination is not cut short on a phone (D32 item 10 (b)).
            list = list.push(
                column![
                    row![
                        t(
                            format!("Destination {}", i + 1),
                            ty::FORM_LABEL,
                            color::TEXT_SECONDARY
                        )
                        .width(Length::Fill),
                        remove,
                    ]
                    .width(Length::Fill)
                    .align_y(Alignment::Center),
                    // A Paste button beside it (docs/DECISIONS.md D32 item
                    // 10 (g)).
                    row![
                        to.width(Length::Fill),
                        paste::button(shown.map(|(form, rows)| {
                            Message::Paste(PasteInto::Destination { form, row: i, rows })
                        })),
                    ]
                    .spacing(sp::S8)
                    .align_y(Alignment::Center)
                    .width(Length::Fill),
                    row![
                        column![
                            t("Amount (MCM)", ty::FORM_LABEL, color::TEXT_SECONDARY),
                            amount
                        ]
                        .spacing(sp::S6)
                        .width(Length::Fill),
                        column![
                            t("Reference", ty::FORM_LABEL, color::TEXT_SECONDARY),
                            reference
                        ]
                        .spacing(sp::S6)
                        .width(Length::Fill),
                    ]
                    .spacing(sp::S12)
                    .width(Length::Fill),
                ]
                .spacing(sp::S12),
            );
        } else {
            list = list.push(
                row![
                    t((i + 1).to_string(), ty::MONO, color::TEXT_MUTED).width(Length::Fixed(20.0)),
                    container(to).width(Length::FillPortion(6)),
                    container(amount).width(Length::FillPortion(3)),
                    container(reference).width(Length::FillPortion(3)),
                    remove,
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center),
            );
        }
    }
    let mut part = column![
        row![
            ui::section_label("Destinations"),
            space().width(Length::Fill),
            t(
                format!("{} of {MAX_DESTINATIONS}", f.rows.len()),
                ty::NOTE,
                color::TEXT_SECONDARY
            ),
        ],
        list,
        ui::button_with(
            "Add destination",
            theme::Button::Dashed,
            Size::Medium,
            Some(Icon::Plus),
            (idle && f.rows.len() < usize::from(MAX_DESTINATIONS))
                .then_some(WalletMsg::AddDestination.into()),
        )
        .width(Length::Fill),
    ]
    .spacing(sp::S12);
    if !several {
        let mut all = checkbox(f.everything)
            .label("Send everything: the whole balance less the fee, to this one destination.")
            .font(fonts::BODY)
            .text_size(13);
        if idle {
            all = all.on_toggle(|on| WalletMsg::Everything(on).into());
        }
        part = part.push(all);
    }
    let mut fee = field(&f.fee, "the node's floor", true);
    let mut btl = field(&f.blk_to_live, "none", true);
    if idle {
        fee = fee.on_input(|v| WalletMsg::Fee(v).into());
        btl = btl.on_input(|v| WalletMsg::BlockToLive(v).into());
    }
    let fee = column![
        t(
            "Fee, nanoMCM in total",
            ty::FORM_LABEL,
            color::TEXT_SECONDARY
        ),
        fee
    ]
    .spacing(sp::S6)
    .width(Length::Fill);
    let btl = column![
        t("Block-to-live", ty::FORM_LABEL, color::TEXT_SECONDARY),
        btl
    ]
    .spacing(sp::S6)
    .width(Length::Fill);
    // At 360 a half-width fee field cuts its placeholder, "the node's floor",
    // short, so on a phone the two fields stand one over the other
    // (docs/DECISIONS.md D32 item 10 (b)).
    let fees: Element<'a, Message> = if compact {
        column![fee, btl].spacing(sp::S12).into()
    } else {
        row![fee, btl].spacing(sp::S12).into()
    };
    part.push(fees)
        .push(ui::helper(format!(
            "The fee is {FEE_FLOOR} nanoMCM per destination, the node's floor, unless you give \
         one. A block-to-live is the last block the spend may land in; none means it never \
         expires. The reference rule, in the wallet library's words: {REFERENCE_RULE}."
        )))
        .into()
}

/// W4's summary (03): what the typed figures come to, as far as they can be
/// read, and the change left on `from`. The figures that are signed are the
/// library's, on the next step.
fn summary<'a>(
    model: &'a Model,
    s: &'a SendPage,
    from: Option<tawara_wallet_core::view::AccountId>,
) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let rows: Vec<_> = s
        .form
        .rows
        .iter()
        .filter(|r| !r.to.trim().is_empty() || !r.amount.trim().is_empty())
        .collect();
    let count = rows.len().max(1);
    let everything = s.form.everything && s.form.rows.len() == 1;
    let sent: Option<u64> = if everything {
        None
    } else {
        rows.iter()
            .map(|r| tawara_wallet_core::amount::parse_mcm(&r.amount).ok())
            .try_fold(0u64, |sum, a| a.and_then(|a| sum.checked_add(a)))
    };
    let fee = match s.form.fee.trim() {
        "" => Some(FEE_FLOOR * u64::try_from(count).unwrap_or(u64::MAX)),
        typed => tawara_wallet_core::amount::parse_nano(typed).ok(),
    };
    let shown = |v: Option<u64>| v.map_or_else(|| "—".to_owned(), |v| ui::amount(v, unit));
    let total = sent.zip(fee).and_then(|(a, b)| a.checked_add(b));
    let from = from.and_then(|id| row_of(model, id));
    let change = from
        .and_then(|a| a.state.balance())
        .zip(total)
        .and_then(|(balance, total)| balance.checked_sub(total));
    let mut card = column![
        ui::section_label("Summary"),
        figure(
            if count == 1 {
                "1 destination".to_owned()
            } else {
                format!("{count} destinations")
            },
            if everything {
                "everything".to_owned()
            } else {
                shown(sent)
            }
        ),
        figure(
            if s.form.fee.trim().is_empty() {
                format!("Network fee · {FEE_FLOOR} nanoMCM × {count}")
            } else {
                "Network fee".to_owned()
            },
            shown(fee)
        ),
        ui::divider(),
        row![
            t("Total", ty::BODY, color::TEXT_SECONDARY).width(Length::Fill),
            t(
                if everything {
                    "the whole balance".to_owned()
                } else {
                    shown(total)
                },
                ty::STAT_VALUE,
                color::TEXT_PRIMARY
            ),
            t(ui::unit_name(unit), ty::CHIP, color::ACCENT),
        ]
        .spacing(sp::S8)
        .align_y(Alignment::Center),
    ]
    .spacing(sp::S12);
    if let Some(a) = from {
        card = card.push(figure(
            format!("Change to key #{}", a.index.saturating_add(1)),
            if everything {
                "none: this empties the account".to_owned()
            } else {
                shown(change)
            },
        ));
    }
    ui::card_for(model.compact(), card).into()
}

/// The key a spend from `from` signs with, and what signing does to the
/// index first (03 callout).
fn key_callout<'a>(
    model: &'a Model,
    from: Option<tawara_wallet_core::view::AccountId>,
) -> Element<'a, Message> {
    let Some(account) = from.and_then(|id| row_of(model, id)) else {
        return space().into();
    };
    let next = account.index.saturating_add(1);
    ui::accent_callout(
        Icon::Key,
        format!("Signs with one-time key #{}", account.index),
        format!(
            "The index advances to #{next} and is written to disk before the signature \
             exists. The change lands on key #{next} under the same tag, {}.",
            name(account.id)
        ),
    )
}

/// Where a step of a spend is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Pending,
    Done,
    Failed,
}

/// The settle step of W6 follows the account's live state (docs/DECISIONS.md
/// D38): done once the account is in sync again, which settling leaves, and
/// pending until then, whether the spend is still outstanding or has landed
/// and not yet settled.
fn settle_step(model: &Model, from: AccountId) -> Step {
    match row_of(model, from).map(|a| &a.state) {
        Some(AccountState::InSync { .. }) => Step::Done,
        _ => Step::Pending,
    }
}

/// The three steps of a spend (03): reserving the key, submitting to the
/// node's socket, and settling once the chain shows the change. They show
/// what has happened, nothing more (docs/SCREENS.md W6). On a phone they
/// stand one above another in the card.
fn steps<'a>(compact: bool, reserve: Step, submit: Step, settle: Step) -> Element<'a, Message> {
    let step = |title: &'a str, what: &'a str, at: Step| {
        let (bar, ink) = match at {
            Step::Done => (color::ACCENT, color::TEXT_PRIMARY),
            Step::Failed => (color::WARNING, color::WARNING),
            Step::Pending => (color::BORDER_STRONG, color::TEXT_SECONDARY),
        };
        column![
            container(space())
                .height(Length::Fixed(3.0))
                .width(Length::Fill)
                .style(theme::pill(bar, bar)),
            t(title, ty::ROW_TITLE, ink),
            t(what, ty::NOTE, color::TEXT_MUTED),
        ]
        .spacing(sp::S6)
        .width(Length::Fill)
    };
    let (reserved, submitted, settled) = (
        step("Reserve", "key index saved", reserve),
        step("Submit", "to the node's socket", submit),
        step("Settle", "change on the ledger", settle),
    );
    let body: Element<'a, Message> = if compact {
        column![reserved, submitted, settled]
            .spacing(sp::S16)
            .into()
    } else {
        row![reserved, submitted, settled].spacing(sp::S12).into()
    };
    ui::card_for(compact, body).into()
}

/// A row of buttons. On a phone it wraps onto the next line when the buttons
/// do not fit (docs/SCREENS.md, "Compact layout"); on the desktop it is one
/// row, as it has always been drawn.
fn button_row<'a>(compact: bool, buttons: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let buttons = Row::with_children(buttons).spacing(sp::S10);
    if compact {
        buttons.wrap().vertical_spacing(sp::S10).into()
    } else {
        buttons.into()
    }
}

/// W5: the library's page for the spend, not signed, whole; then signing,
/// on its own step.
fn review<'a>(model: &'a Model, page: &'a WalletPage, plan: &'a PlanView) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let compact = model.compact();
    let index = row_of(model, plan.from).map(|a| a.index);
    // What to check, one destination each, as they go on the wire. A phone
    // has no table header: each destination is a block (docs/SCREENS.md,
    // "Compact layout").
    let mut list = column![ui::section_label(
        "Check each destination against its payee"
    )]
    .spacing(sp::S12);
    if !compact {
        list = list.push(
            row![
                t("#", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::Fixed(20.0)),
                t("Destination", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::FillPortion(6)),
                t("Reference", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::FillPortion(3)),
                container(t(
                    format!("Amount ({})", ui::unit_name(unit)),
                    ty::TABLE_HEADER,
                    color::TEXT_MUTED
                ))
                .width(Length::FillPortion(3))
                .align_x(Alignment::End),
            ]
            .spacing(sp::S12),
        );
    }
    for (i, d) in plan.destinations.iter().enumerate() {
        let entry: Element<'a, Message> = if compact {
            planned_block(unit, i, d)
        } else {
            row![
                t((i + 1).to_string(), ty::MONO, color::TEXT_MUTED).width(Length::Fixed(20.0)),
                t(d.destination.clone(), ty::MONO, color::TEXT_PRIMARY)
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::FillPortion(6)),
                t(
                    if d.reference.is_empty() {
                        "none".to_owned()
                    } else {
                        d.reference.clone()
                    },
                    ty::MONO,
                    color::TEXT_SECONDARY,
                )
                .width(Length::FillPortion(3)),
                container(
                    t(
                        ui::amount(d.amount, unit),
                        ty::TABLE_AMOUNT,
                        color::TEXT_PRIMARY
                    )
                    .wrapping(Wrapping::WordOrGlyph),
                )
                .width(Length::FillPortion(3))
                .align_x(Alignment::End),
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center)
            .into()
        };
        list = list.push(ui::divider()).push(entry);
    }
    let main = column![
        report::show_in(compact, page, ReportKey::Plan, report::plan(plan, index)),
        ui::card_for(compact, list),
    ]
    .spacing(sp::S16);
    let side = column![
        ui::card_for(compact, plan_totals(unit, plan, index)),
        key_callout(model, Some(plan.from)),
        steps(compact, Step::Pending, Step::Pending, Step::Pending),
        action(
            model,
            "Sign & submit",
            theme::Button::Primary,
            Size::Xl,
            None,
            WalletMsg::Sign,
        ),
        action(
            model,
            "Edit the spend",
            theme::Button::Secondary,
            Size::Medium,
            None,
            WalletMsg::Edit,
        ),
    ]
    .spacing(sp::S16);
    frame(
        model,
        page,
        "Send",
        "Check every destination against its payee. Nothing has been signed yet.",
        Vec::new(),
        vec![pair(model, main.into(), side.into(), 7, 4)],
    )
}

/// One destination of a plan as a phone's review draws it: the number and the
/// amount on one line, then the destination and the reference, each wrapping
/// at its glyphs (docs/SCREENS.md, "Compact layout").
fn planned_block<'a>(
    unit: AmountUnit,
    i: usize,
    d: &'a PlannedDestination,
) -> Element<'a, Message> {
    let reference = if d.reference.is_empty() {
        "none"
    } else {
        d.reference.as_str()
    };
    column![
        row![
            t(format!("#{}", i + 1), ty::MONO, color::TEXT_MUTED),
            space().width(Length::Fill),
            t(
                ui::amount(d.amount, unit),
                ty::TABLE_AMOUNT,
                color::TEXT_PRIMARY
            )
            .wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center),
        t(d.destination.clone(), ty::MONO_SMALL, color::TEXT_PRIMARY)
            .wrapping(Wrapping::WordOrGlyph),
        t(
            format!("Reference: {reference}"),
            ty::MONO_SMALL,
            color::TEXT_SECONDARY
        )
        .wrapping(Wrapping::WordOrGlyph),
    ]
    .spacing(sp::S6)
    .into()
}

/// The library's figures for a plan: how many destinations, what they send,
/// the fee and the total, the change to the account's next key, and the
/// balance the plan was laid out on (docs/SCREENS.md W5).
fn plan_totals<'a>(
    unit: AmountUnit,
    plan: &'a PlanView,
    index: Option<u32>,
) -> Element<'a, Message> {
    let count = plan.destinations.len();
    column![
        ui::section_label("Summary"),
        figure(
            if count == 1 {
                "1 destination".to_owned()
            } else {
                format!("{count} destinations")
            },
            ui::amount(plan.send_total, unit)
        ),
        figure("Network fee", ui::amount(plan.fee_total, unit)),
        ui::divider(),
        row![
            t("Total", ty::BODY, color::TEXT_SECONDARY).width(Length::Fill),
            t(
                ui::amount(
                    u128::from(plan.send_total) + u128::from(plan.fee_total),
                    unit
                ),
                ty::STAT_VALUE,
                color::TEXT_PRIMARY
            ),
            t(ui::unit_name(unit), ty::CHIP, color::ACCENT),
        ]
        .spacing(sp::S8)
        .align_y(Alignment::Center),
        figure(
            index.map_or_else(
                || "Change".to_owned(),
                |i| format!("Change to key #{}", i.saturating_add(1))
            ),
            if plan.empties_account {
                "none: this empties the account".to_owned()
            } else {
                ui::amount(plan.change_total, unit)
            }
        ),
        figure("Balance it was laid out on", ui::amount(plan.balance, unit)),
    ]
    .spacing(sp::S12)
    .into()
}

/// Page 3 of 4 on a phone (docs/DECISIONS.md D40 item 1): the library's page
/// for the spend, not signed, as its banner; the destinations in full; the
/// library's figures; and "Continue to signing", with no Sign button.
fn review_page<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    plan: &'a PlanView,
) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let compact = model.compact();
    let index = row_of(model, plan.from).map(|a| a.index);
    let mut list = column![ui::section_label(
        "Check each destination against its payee"
    )]
    .spacing(sp::S12);
    for (i, d) in plan.destinations.iter().enumerate() {
        list = list.push(ui::divider()).push(planned_block(unit, i, d));
    }
    let mut next = column![action(
        model,
        "Continue to signing",
        theme::Button::Primary,
        Size::Xl,
        None,
        WalletMsg::ToSigning,
    )]
    .spacing(sp::S16);
    for part in beside(model, page) {
        next = next.push(part);
    }
    frame_beside(
        model,
        page,
        "Send",
        "Check every destination against its payee. Nothing has been signed yet.",
        vec![back(model)],
        vec![
            place(3),
            report::show_in(compact, page, ReportKey::Plan, report::plan(plan, index)),
            ui::card_for(compact, list).into(),
            ui::card_for(compact, plan_totals(unit, plan, index)).into(),
            next.into(),
        ],
    )
}

/// Page 4 of 4 on a phone (docs/DECISIONS.md D40 item 1): the one-time key the
/// spend signs with and its three steps, then "Sign & submit" alone, with a
/// line in W6's words that a submit is a write to the node's socket, not the
/// node's verdict.
fn sign_page<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    plan: &'a PlanView,
) -> Element<'a, Message> {
    let compact = model.compact();
    let mut next = column![
        action(
            model,
            "Sign & submit",
            theme::Button::Primary,
            Size::Xl,
            None,
            WalletMsg::Sign,
        ),
        ui::helper(
            "Submitting writes the signed spend to the node's socket. That is not the node's \
             verdict: settle once the chain has moved.",
        ),
    ]
    .spacing(sp::S16);
    for part in beside(model, page) {
        next = next.push(part);
    }
    frame_beside(
        model,
        page,
        "Send",
        "The key is reserved and the spend signed when the button below is pressed.",
        vec![back(model)],
        vec![
            place(4),
            key_callout(model, Some(plan.from)),
            steps(compact, Step::Pending, Step::Pending, Step::Pending),
            next.into(),
        ],
    )
}

/// W6's Retry artifact card, for bytes not saved (docs/DECISIONS.md D38 item
/// 5, as revised on 2026-10-13). Provisional wording, for the owner's
/// approval.
const UNSAVED_ARTIFACT: &str = "The signed bytes. The store does not keep them. Tawara keeps \
                                them until they are saved, dismissed or the wallet locks, and \
                                a reorganisation of the chain can revert a settle, so save \
                                them before going further.";

/// W6's Retry artifact card, for bytes saved to a file (docs/DECISIONS.md D38
/// item 5, as revised on 2026-10-13). Provisional wording, for the owner's
/// approval.
const SAVED_ARTIFACT: &str = "The signed bytes, saved to the file named below. The store does \
                              not keep them, so keep that file: a reorganisation of the chain \
                              can revert a settle, and then it may be the only way to move \
                              these funds.";

/// What W6 says once Dismiss is pressed, before it is confirmed
/// (docs/DECISIONS.md D38 item 5, as revised on 2026-10-13). Provisional
/// wording, for the owner's approval.
pub(crate) const DISMISS_WARNING: &str = "Dismissing wipes the only copy of these bytes that \
                                          Tawara holds, and Tawara wrote none to disk: unless a \
                                          copy was kept elsewhere, they are gone. If a \
                                          reorganisation of the chain reverts a settle, a copy \
                                          may be the only way to move these funds.";

/// W6: Dismiss for bytes not saved, on its own row below the card's other
/// controls. Pressed once, it asks to be confirmed and says what is lost.
fn dismiss<'a>(model: &Model, compact: bool, s: &'a SentPage) -> Element<'a, Message> {
    if !s.dismissing {
        return button_row(
            compact,
            vec![action(
                model,
                "Dismiss",
                theme::Button::Secondary,
                Size::Medium,
                None,
                WalletMsg::AskDismiss,
            )],
        );
    }
    column![
        ui::warning_callout("Dismiss the signed bytes?", DISMISS_WARNING),
        button_row(
            compact,
            vec![
                action(
                    model,
                    "Dismiss the bytes",
                    theme::Button::WarningPrimary,
                    Size::Medium,
                    None,
                    WalletMsg::DismissSigned(s.sent.artifact_hex.clone()),
                ),
                action(
                    model,
                    "Keep them",
                    theme::Button::Secondary,
                    Size::Medium,
                    None,
                    WalletMsg::CancelDismiss,
                ),
            ],
        ),
    ]
    .spacing(sp::S12)
    .into()
}

/// W6: what happened, the bytes the store does not keep, and the library's
/// page with its three facts, whole.
fn sent_page<'a>(model: &'a Model, page: &'a WalletPage, s: &'a SentPage) -> Element<'a, Message> {
    let sent = &s.sent;
    let compact = model.compact();
    let submitted = if sent.submitted {
        Step::Done
    } else {
        Step::Failed
    };
    let buttons = vec![
        action(
            model,
            "Save artifact",
            theme::Button::Primary,
            Size::Medium,
            Some(Icon::Download),
            WalletMsg::SaveArtifact,
        ),
        copy(page, &sent.artifact_hex, "Copy hex"),
    ];
    // What the card says follows whether the bytes are saved
    // (docs/DECISIONS.md D38 item 5, as revised on 2026-10-13).
    let about = if s.unsaved() {
        UNSAVED_ARTIFACT
    } else {
        SAVED_ARTIFACT
    };
    let mut artifact = column![
        ui::section_label("Retry artifact"),
        t(about, ty::BODY_SMALL, color::TEXT_SECONDARY),
        button_row(compact, buttons),
    ]
    .spacing(sp::S12);
    if let Some(note) = super::saved_note(s) {
        artifact = artifact.push(note);
    }
    if let Some(id) = &sent.tx_id {
        artifact = artifact.push(
            t(
                format!("Transaction id {id}"),
                ty::MONO_SMALL,
                color::TEXT_MUTED,
            )
            .wrapping(Wrapping::WordOrGlyph),
        );
    }
    // Dismiss is offered only for bytes that are not saved, apart from Save
    // and only once confirmed, since it wipes the only copy Tawara holds
    // (docs/DECISIONS.md D38 item 5, as revised on 2026-10-13).
    if s.unsaved() {
        artifact = artifact.push(dismiss(model, compact, s));
    }
    let body = vec![
        steps(
            compact,
            Step::Done,
            submitted,
            settle_step(model, sent.from),
        ),
        report::show_in(compact, page, ReportKey::Sent, report::sent(sent)),
        ui::card_for(compact, artifact).into(),
        button_row(
            compact,
            vec![
                action(
                    model,
                    "Back to the wallet",
                    theme::Button::Secondary,
                    Size::Medium,
                    None,
                    WalletMsg::Open(To::Dashboard),
                ),
                action(
                    model,
                    "Open the account",
                    theme::Button::Secondary,
                    Size::Medium,
                    None,
                    WalletMsg::Open(To::Account(sent.from)),
                ),
            ],
        ),
    ];
    frame(
        model,
        page,
        if s.resigned { "Re-signed" } else { "Sent" },
        if sent.submitted {
            "Signed, and written to the node's socket. That is not the node's verdict: settle \
             once the chain has moved."
        } else {
            "Signed, and not written to the node's socket: the bytes below are the spend."
        },
        Vec::new(),
        body,
    )
}

/// W8: the reserved spend from one account, typed again exactly, signed
/// again and submitted.
pub fn resign<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    r: &'a ResignPage,
) -> Element<'a, Message> {
    let form = column![
        ui::warning_callout(
            "The same spend, exactly",
            "Give the destinations, amounts, references, fee and block-to-live the reserved \
             spend was made with. Anything else is refused as a different spend, and nothing \
             is signed.",
        ),
        destinations(model, &r.form),
    ]
    .spacing(sp::S20);
    let side = column![
        steps(model.compact(), Step::Done, Step::Pending, Step::Pending),
        action(
            model,
            "Re-sign & submit",
            theme::Button::Primary,
            Size::Xl,
            None,
            WalletMsg::Resign,
        ),
        ui::helper(
            "Re-signing reproduces the reserved spend's bytes with its key, which is already \
             reserved: no new key is used.",
        ),
    ]
    .spacing(sp::S16);
    frame(
        model,
        page,
        "Re-sign",
        format!(
            "The spend reserved from {}, signed again and submitted.",
            name(r.account)
        ),
        Vec::new(),
        vec![pair(
            model,
            ui::card_for(model.compact(), form).into(),
            side.into(),
            7,
            4,
        )],
    )
}

/// W9: signed bytes saved earlier, written to the node's socket again. No
/// store is used.
pub fn submit<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    s: &'a SubmitPage,
) -> Element<'a, Message> {
    submit_form(model, page, s, RETURN_BROADCASTS)
}

/// W9 with Return's binding given, so that the tests, which run on the
/// desktop, can draw the page as a phone has it (docs/DECISIONS.md D32 item
/// 9).
fn submit_form<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    s: &'a SubmitPage,
    return_broadcasts: bool,
) -> Element<'a, Message> {
    let mut hex = text_input("the hex the sent page saved or copied", &s.hex)
        .font(ty::MONO_SMALL.font)
        .size(ty::MONO_SMALL.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .style(theme::field(radius::R12));
    if model.busy.is_none() {
        hex = hex
            .on_input(|v| WalletMsg::ArtifactHex(v).into())
            .on_submit_maybe(return_broadcasts.then_some(WalletMsg::Submit.into()));
    }
    let compact = model.compact();
    let label = t(
        "Signed bytes, as hex",
        ty::FORM_LABEL,
        color::TEXT_SECONDARY,
    );
    let submit = action(
        model,
        "Submit",
        theme::Button::Primary,
        Size::Medium,
        None,
        WalletMsg::Submit,
    );
    // What the page says of the bytes depends on what they are sent as
    // (docs/DECISIONS.md D38 item 5 (b)).
    let (helper, subtitle) = match s.mode {
        SubmitMode::Artifact => (
            ui::helper("Only the bytes' layout is checked here; the node validates the rest."),
            "A signed spend saved earlier, written to the node's socket again. Nothing is signed.",
        ),
        SubmitMode::Resubmit(_) => (
            ui::helper(
                "Only the identical saved copy is accepted, and only while the balance at its \
                 source has not moved and its block-to-live has not passed. The library checks \
                 the copy before anything is written.",
            ),
            "A saved copy of a settle the chain reverted, written to the node's socket again. \
             Nothing is signed, and no key index moves.",
        ),
    };
    // On a phone the field has the width of its row, less the Paste button
    // beside it (docs/DECISIONS.md D32 item 10 (g)), and Submit stands above
    // its helper (docs/SCREENS.md, "Compact layout").
    let paste = paste::button(
        model
            .busy
            .is_none()
            .then_some(Message::Paste(PasteInto::SignedBytes)),
    );
    let entry: Element<'a, Message> = if compact {
        column![
            label,
            row![hex, paste]
                .spacing(sp::S8)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            column![submit, helper].spacing(sp::S12),
        ]
        .spacing(sp::S12)
        .into()
    } else {
        column![
            label,
            hex,
            row![submit, helper]
                .spacing(sp::S12)
                .align_y(Alignment::Center),
        ]
        .spacing(sp::S12)
        .into()
    };
    let mut body = vec![ui::card_for(compact, entry).into()];
    if let Some((accepted, text)) = &s.result {
        // A saved copy's answer is a write: a refusal is the page's error.
        let answer = match s.mode {
            SubmitMode::Artifact => report::submitted(*accepted, text),
            SubmitMode::Resubmit(_) => report::resubmitted(text),
        };
        body.push(report::show_in(compact, page, ReportKey::Submitted, answer));
    }
    frame(
        model,
        page,
        "Submit saved artifact",
        subtitle,
        Vec::new(),
        body,
    )
}

#[cfg(test)]
mod tests {
    use iced::advanced::clipboard::Null;
    use iced::advanced::layout;
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::operation::{Focusable, Outcome, focusable};
    use iced::advanced::widget::{Operation, Tree};
    use iced::keyboard::key::{Named, NativeCode, Physical};
    use iced::keyboard::{Key, Location, Modifiers};
    use iced::{Event, Rectangle, keyboard, mouse};
    use iced_runtime::user_interface::{Cache, UserInterface};
    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::sample;
    use tawara_wallet_core::view::AccountState;

    use super::*;
    use crate::app::{Busy, DestinationRow, Level, NARROWEST, Page, Screen};
    use crate::harness::{Driver, fits, mouse_move, mouse_press, mouse_release, texts};
    use crate::screens;

    /// A key press as the keyboard sends it, with the text it types, if any.
    fn press(key: Key, text: Option<&str>) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: text.map(Into::into),
            repeat: false,
        })
    }

    /// Draws W9 with `view`, gives its field Tab's first focus, then sends
    /// `events` one at a time and returns the messages each produced.
    fn drive(
        view: impl for<'a> Fn(&'a Model, &'a WalletPage, &'a SubmitPage) -> Element<'a, Message>,
        events: &[Event],
    ) -> Vec<Vec<Message>> {
        let model = Model::new(Preferences::default(), None);
        let page = WalletPage {
            page: Page::Submit(SubmitPage::default()),
            error: None,
            copied: None,
            open: Default::default(),
            expanded: None,
            choosing: false,
        };
        let s = SubmitPage::default();
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("the software renderer starts");
        let mut ui = UserInterface::build(
            view(&model, &page, &s),
            iced::Size::new(1440.0, 900.0),
            Cache::default(),
            &mut renderer,
        );
        // Tab's first focus: the field is the page's first focusable widget,
        // so the keys below reach it.
        let mut next: Option<Box<dyn Operation>> = Some(Box::new(focusable::focus_next()));
        while let Some(mut operation) = next.take() {
            ui.operate(&renderer, operation.as_mut());
            if let Outcome::Chain(chained) = operation.finish() {
                next = Some(chained);
            }
        }
        events
            .iter()
            .map(|event| {
                let mut messages = Vec::new();
                ui.update(
                    std::slice::from_ref(event),
                    mouse::Cursor::Unavailable,
                    &mut renderer,
                    &mut Null,
                    &mut messages,
                );
                messages
            })
            .collect()
    }

    #[test]
    fn return_in_the_signed_bytes_field_submits_on_the_host() {
        // The page as the application draws it, so this pins the desktop's
        // value of RETURN_BROADCASTS (docs/DECISIONS.md D32 item 9).
        let sent = drive(submit, &[press(Key::Named(Named::Enter), None)]);
        assert!(
            matches!(sent.as_slice(), [m] if matches!(m.as_slice(), [Message::Wallet(WalletMsg::Submit)])),
            "{sent:?}"
        );
    }

    #[test]
    fn return_in_the_signed_bytes_field_does_nothing_on_a_phone() {
        // The page as a phone draws it (docs/DECISIONS.md D32 item 9). The
        // letter after Return shows that the field had the focus and still
        // took input, so the silence on Return is the binding's and not a
        // missed focus.
        let sent = drive(
            |model, page, s| submit_form(model, page, s, false),
            &[
                press(Key::Named(Named::Enter), None),
                press(Key::Character("a".into()), Some("a")),
            ],
        );
        assert!(
            matches!(
                sent.as_slice(),
                [on_return, on_letter]
                    if on_return.is_empty()
                        && matches!(on_letter.as_slice(), [Message::Wallet(WalletMsg::ArtifactHex(v))] if v == "a")
            ),
            "{sent:?}"
        );
    }

    /// The sample wallet open on `page` at the phone's narrowest width, with
    /// the report `open` (if any) opened to its summary.
    fn phone(page: Page, open: Option<ReportKey>) -> Model {
        let mut model = Model::new(Preferences::default(), None);
        model.width = NARROWEST;
        model.wallet = Some(sample::wallet_view());
        let mut wallet = WalletPage {
            page,
            ..WalletPage::default()
        };
        if let Some(key) = open {
            wallet.open.insert(key, Level::Summary);
        }
        model.screen = Screen::Wallet(wallet);
        model
    }

    /// The sample's spend as a person types it: its two payees, with the
    /// figures the sample lays out (`sample::plan_view`).
    fn typed() -> SpendForm {
        SpendForm {
            rows: sample::plan_view()
                .destinations
                .iter()
                .map(|d| DestinationRow {
                    to: d.destination.clone(),
                    amount: tawara_wallet_core::amount::format_mcm(d.amount)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_owned(),
                    reference: d.reference.clone(),
                })
                .collect(),
            ..SpendForm::default()
        }
    }

    /// The sample wallet's accounts, the first of which the sample spend comes
    /// from.
    fn accounts() -> Vec<tawara_wallet_core::view::AccountId> {
        sample::wallet_view()
            .accounts
            .iter()
            .map(|a| a.id)
            .collect()
    }

    /// W4 from the sample's first account, with `form`.
    fn compose(form: SpendForm) -> Page {
        Page::Send(SendPage {
            from: Some(accounts()[0]),
            form,
            stage: SendStage::Compose,
        })
    }

    /// The send flow in `stage` from the sample's first account, with `form`.
    fn stage_page(stage: SendStage, form: SpendForm) -> Page {
        Page::Send(SendPage {
            from: Some(accounts()[0]),
            form,
            stage,
        })
    }

    /// The sample's first account's plan, as the worker holds it on page 3 and
    /// page 4.
    fn plan() -> Box<PlanView> {
        Box::new(sample::plan_view())
    }

    /// `page` in the sample wallet as a phone holds it: its folder is the
    /// phone's, so the page is a phone's, and its window is `width` wide.
    fn phone_at(page: Page, width: f32) -> Model {
        let mut model = phone(page, None);
        model.width = width;
        model.confine(std::env::temp_dir().join("tawara-phone-folder"));
        model
    }

    /// `page` in the sample wallet as the desktop holds it, `width` wide.
    fn desktop_at(page: Page, width: f32) -> Model {
        let mut model = phone(page, None);
        model.width = width;
        model
    }

    /// The sample wallet with its second account able to spend as well.
    fn two_can_spend() -> tawara_wallet_core::view::WalletView {
        let mut wallet = sample::wallet_view();
        wallet.accounts[1].state = AccountState::InSync {
            balance: 3_180_000_000_000,
        };
        wallet.accounts[1].spendable = true;
        wallet
    }

    /// `model` with its page's account list open, or shut.
    fn choosing(mut model: Model, open: bool) -> Model {
        if let Screen::Wallet(page) = &mut model.screen {
            page.choosing = open;
        }
        model
    }

    /// Every node and word of `root` that passes the edge of a window `width`
    /// by `height`, as [`fits`] finds them at 844 (docs/DECISIONS.md D32 item
    /// 10 (b)): the phone's two sizes are held here, 360 by 640 and 390 by 844.
    fn past_the_edge(root: Element<'_, Message>, width: f32, height: f32) -> Vec<Rectangle> {
        crate::overflow::past_the_edge(root, &crate::harness::renderer(), width, height)
            .into_iter()
            .map(|past| past.bounds)
            .collect()
    }

    /// The phone's two sizes, the narrowest and the common one.
    const PHONES: [(f32, f32); 2] = [(360.0, 640.0), (390.0, 844.0)];

    /// Every text `model` draws in a window 2400 pixels tall, at its width.
    fn drawn_on(model: &Model) -> Vec<(String, Rectangle)> {
        texts(screens::view(model), (model.width, 2400.0))
    }

    /// Where the first text `drawn` holds that is `what` lies, or a panic.
    fn at(drawn: &[(String, Rectangle)], what: &str) -> Rectangle {
        drawn
            .iter()
            .find(|(text, _)| text == what)
            .map(|(_, at)| *at)
            .unwrap_or_else(|| panic!("{what:?} is not drawn: {drawn:?}"))
    }

    /// Whether `drawn` holds the text `what`.
    fn has(drawn: &[(String, Rectangle)], what: &str) -> bool {
        drawn.iter().any(|(text, _)| text == what)
    }

    /// The bounds of the focusable widgets of `root` in a phone's window, in
    /// Tab's order: a text field's bounds are its drawn box.
    fn focusable_bounds(root: Element<'_, Message>) -> Vec<Rectangle> {
        #[derive(Default)]
        struct Bounds(Vec<Rectangle>);

        impl Operation for Bounds {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }

            fn focusable(
                &mut self,
                _: Option<&iced::advanced::widget::Id>,
                bounds: Rectangle,
                _: &mut dyn Focusable,
            ) {
                self.0.push(bounds);
            }
        }

        let mut renderer = crate::harness::renderer();
        let mut ui = UserInterface::build(
            root,
            iced::Size::new(NARROWEST, 844.0),
            Cache::default(),
            &mut renderer,
        );
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        bounds.0
    }

    /// The sample wallet with its second account set aside when the wallet
    /// opened, as the sample draws it (`w4-send-cannot-spend`): in sync, and
    /// not spendable until Refresh.
    fn set_aside() -> tawara_wallet_core::view::WalletView {
        let mut wallet = sample::wallet_view();
        wallet.accounts[1].state = AccountState::InSync {
            balance: 3_180_000_000_000,
        };
        wallet.accounts[1].spendable = false;
        wallet
    }

    /// `model` with a refusal `why` shown on its page, as a command gives it.
    fn with_error(mut model: Model, why: &str) -> Model {
        if let Screen::Wallet(page) = &mut model.screen {
            page.error = Some(why.to_owned());
        }
        model
    }

    /// `model` with its page's report `key` open to its summary.
    fn opened(mut model: Model, key: ReportKey) -> Model {
        if let Screen::Wallet(page) = &mut model.screen {
            page.open.insert(key, Level::Summary);
        }
        model
    }

    /// `model` with the worker answering a command of the page: busy, not yet
    /// started on.
    fn busy(mut model: Model) -> Model {
        model.busy = Some(Busy {
            id: tawara_wallet_core::RequestId::example(),
            activity: None,
            progress: None,
        });
        model
    }

    /// The phone's pages, each in the state that makes it longest, at `width`:
    /// the most destinations, the account set aside, the list of accounts
    /// open, refusals under the fields and beside the buttons, the busy card,
    /// and the review with its report open and shut (docs/DECISIONS.md D40).
    fn phone_pages(width: f32) -> Vec<(&'static str, Model)> {
        let mut three = typed();
        three.rows.push(DestinationRow {
            to: three.rows[0].to.clone(),
            amount: "1".to_owned(),
            reference: "PAYROLL-13".to_owned(),
        });
        let one = SpendForm {
            rows: typed().rows[..1].to_vec(),
            everything: true,
            ..SpendForm::default()
        };
        let mut under_fields = BTreeMap::new();
        under_fields.insert(
            (0, RefusedField::Address),
            "Give this destination's address.".to_owned(),
        );
        under_fields.insert((1, RefusedField::Reference), REFERENCE_RULE.to_owned());
        let to = || SendStage::To(BTreeMap::new());
        let mut set_aside_model = phone_at(stage_page(to(), typed()), width);
        set_aside_model.wallet = Some(set_aside());
        let mut set_aside_amounts = phone_at(
            Page::Send(SendPage {
                from: Some(accounts()[1]),
                form: typed(),
                stage: SendStage::Amount,
            }),
            width,
        );
        set_aside_amounts.wallet = Some(set_aside());
        let mut list = choosing(phone_at(stage_page(to(), typed()), width), true);
        list.wallet = Some(two_can_spend());
        let busy_planning = busy(phone_at(stage_page(SendStage::Amount, typed()), width));
        vec![
            ("page 1", phone_at(stage_page(to(), typed()), width)),
            (
                "page 1 with three destinations",
                phone_at(stage_page(to(), three.clone()), width),
            ),
            ("page 1 with the account set aside", set_aside_model),
            ("page 1 with the account list open", list),
            (
                "page 1 with refusals under its fields",
                phone_at(
                    stage_page(SendStage::To(under_fields), three.clone()),
                    width,
                ),
            ),
            (
                "page 1 with a refusal beside Continue",
                with_error(
                    phone_at(stage_page(to(), typed()), width),
                    "Give at least one destination.",
                ),
            ),
            (
                "page 2",
                phone_at(stage_page(SendStage::Amount, typed()), width),
            ),
            (
                "page 2 with one destination, sent everything",
                phone_at(stage_page(SendStage::Amount, one), width),
            ),
            ("page 2 with the account set aside", set_aside_amounts),
            (
                "page 2 with a refusal beside Review spend",
                with_error(
                    phone_at(stage_page(SendStage::Amount, typed()), width),
                    "The amount to destination 1 is not a number.",
                ),
            ),
            ("page 2 while the worker plans", busy_planning),
            (
                "page 3 with its report open",
                opened(
                    phone_at(stage_page(SendStage::Review(plan()), typed()), width),
                    ReportKey::Plan,
                ),
            ),
            (
                "page 3 with its report shut",
                phone_at(stage_page(SendStage::Review(plan()), typed()), width),
            ),
            (
                "page 4",
                phone_at(stage_page(SendStage::Sign(plan()), typed()), width),
            ),
            (
                "page 4 while the worker signs",
                busy(phone_at(
                    stage_page(SendStage::Sign(plan()), typed()),
                    width,
                )),
            ),
        ]
    }

    #[test]
    fn each_phone_page_fits_at_360_by_640_and_390_by_844() {
        for (width, height) in PHONES {
            for (name, model) in phone_pages(width) {
                let found = past_the_edge(screens::view(&model), width, height);
                assert!(found.is_empty(), "{name} at {width} by {height}: {found:?}");
            }
        }
    }

    #[test]
    fn each_phone_page_shows_its_place_and_its_buttons_and_only_page_four_signs() {
        // What each page draws, and what it must not: "Sign & submit" is on
        // page 4 alone, and "Continue to signing" on page 3 (docs/DECISIONS.md
        // D40 item 1).
        let pages: [(&str, Model, &[&str], &[&str]); 4] = [
            (
                "page 1",
                phone_at(
                    stage_page(SendStage::To(BTreeMap::new()), typed()),
                    NARROWEST,
                ),
                &[
                    "1 of 4",
                    "FROM ACCOUNT",
                    "Destination 1",
                    "Reference",
                    "Remove",
                    "Add another destination",
                    "Continue",
                    "Paste",
                ],
                &[
                    "Back",
                    "Review spend",
                    "Continue to signing",
                    "Sign & submit",
                    "Edit the spend",
                ],
            ),
            (
                "page 2",
                phone_at(stage_page(SendStage::Amount, typed()), NARROWEST),
                &[
                    "2 of 4",
                    "AMOUNTS",
                    "Amount (MCM)",
                    "Fee, nanoMCM in total",
                    "Block-to-live",
                    "Review spend",
                    "Back",
                ],
                &[
                    "Continue",
                    "Continue to signing",
                    "Sign & submit",
                    "Remove",
                    "Paste",
                    "Edit the spend",
                ],
            ),
            (
                "page 3",
                phone_at(stage_page(SendStage::Review(plan()), typed()), NARROWEST),
                &[
                    "3 of 4",
                    "Not signed yet: check every destination against its payee",
                    "SUMMARY",
                    "Continue to signing",
                    "Back",
                ],
                &[
                    "Continue",
                    "Review spend",
                    "Sign & submit",
                    "Edit the spend",
                    "Paste",
                ],
            ),
            (
                "page 4",
                phone_at(stage_page(SendStage::Sign(plan()), typed()), NARROWEST),
                &[
                    "4 of 4",
                    "Sign & submit",
                    "Back",
                    "Reserve",
                    "Submit",
                    "Settle",
                    "Submitting writes the signed spend to the node's socket. That is not the \
                     node's verdict: settle once the chain has moved.",
                ],
                &[
                    "Continue",
                    "Continue to signing",
                    "Review spend",
                    "Edit the spend",
                    "Paste",
                    "Remove",
                ],
            ),
        ];
        for (name, model, must, must_not) in &pages {
            let drawn = drawn_on(model);
            for what in *must {
                assert!(has(&drawn, what), "{name} draws {what:?}");
            }
            for what in *must_not {
                assert!(!has(&drawn, what), "{name} does not draw {what:?}");
            }
        }
        // Page 2 has "Send everything" for a single destination.
        let one = SpendForm {
            rows: typed().rows[..1].to_vec(),
            ..SpendForm::default()
        };
        let drawn = drawn_on(&phone_at(stage_page(SendStage::Amount, one), NARROWEST));
        assert!(has(
            &drawn,
            "Send everything: the whole balance less the fee, to this one destination."
        ));
    }

    #[test]
    fn change_is_drawn_only_when_another_account_can_spend_and_opens_the_list() {
        let one = phone_at(
            stage_page(SendStage::To(BTreeMap::new()), typed()),
            NARROWEST,
        );
        assert!(
            !has(&drawn_on(&one), "Change"),
            "no other account can spend"
        );

        let mut two = phone_at(
            stage_page(SendStage::To(BTreeMap::new()), typed()),
            NARROWEST,
        );
        two.wallet = Some(two_can_spend());
        let shut = drawn_on(&two);
        assert!(has(&shut, "Change"), "{shut:?}");
        assert!(
            !has(&shut, &name(accounts()[1])),
            "the list is shut: {shut:?}"
        );

        // Tapping Change asks to open the list; with the list open, Change
        // asks to shut it.
        let press = |model: &Model, spot: Rectangle| {
            let mut driver = Driver::new(screens::view(model), (360, 844));
            let (messages, _, _) = driver.send(&[
                mouse_move(spot.center_x(), spot.center_y()),
                mouse_press(),
                mouse_release(),
            ]);
            messages
        };
        let messages = press(&two, at(&shut, "Change"));
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Wallet(WalletMsg::ChooseAccount(true))]
            ),
            "{messages:?}"
        );

        let open = choosing(two, true);
        let drawn = drawn_on(&open);
        assert!(
            has(&drawn, &name(accounts()[1])),
            "the list is open: {drawn:?}"
        );
        let messages = press(&open, at(&drawn, "Change"));
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Wallet(WalletMsg::ChooseAccount(false))]
            ),
            "{messages:?}"
        );

        // Picking the second account chooses it.
        let messages = press(&open, at(&drawn, &name(accounts()[1])));
        assert!(
            matches!(messages.as_slice(), [Message::Wallet(WalletMsg::From(id))] if *id == accounts()[1]),
            "{messages:?}"
        );
    }

    #[test]
    fn change_is_drawn_when_the_account_chosen_cannot_spend_and_another_can() {
        // The account asked for cannot spend now, and the sample's first can.
        // The callout says to choose another account, so "Change" is drawn to
        // choose it, though it is the only other account that can spend
        // (docs/DECISIONS.md D40 item 7 (d)).
        let model = phone_at(
            Page::Send(SendPage {
                from: Some(accounts()[1]),
                form: typed(),
                stage: SendStage::To(BTreeMap::new()),
            }),
            NARROWEST,
        );
        let shut = drawn_on(&model);
        assert!(has(&shut, "Change"), "{shut:?}");
        assert!(
            !has(&shut, &name(accounts()[0])),
            "the list is shut: {shut:?}"
        );

        let spot = at(&shut, "Change");
        let mut driver = Driver::new(screens::view(&model), (360, 844));
        let (messages, _, _) = driver.send(&[
            mouse_move(spot.center_x(), spot.center_y()),
            mouse_press(),
            mouse_release(),
        ]);
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Wallet(WalletMsg::ChooseAccount(true))]
            ),
            "{messages:?}"
        );
    }

    #[test]
    fn a_phone_refusal_stands_beneath_the_button_that_caused_it() {
        // Review spend's refusal is drawn under that button, and Continue's
        // under Continue. The desktop draws its refusal at the top of the page,
        // as it always has (docs/DECISIONS.md D40 item 4).
        let why = "The amount to destination 1 is not a number.";
        let model = with_error(
            phone_at(stage_page(SendStage::Amount, typed()), NARROWEST),
            why,
        );
        let drawn = drawn_on(&model);
        assert!(
            at(&drawn, why).y > at(&drawn, "Review spend").y,
            "{drawn:?}"
        );

        let model = with_error(
            phone_at(
                stage_page(SendStage::To(BTreeMap::new()), typed()),
                NARROWEST,
            ),
            "Give at least one destination.",
        );
        let drawn = drawn_on(&model);
        assert!(
            at(&drawn, "Give at least one destination.").y > at(&drawn, "Continue").y,
            "{drawn:?}"
        );

        let model = with_error(desktop_at(compose(typed()), 1440.0), why);
        let drawn = drawn_on(&model);
        assert!(
            at(&drawn, why).y < at(&drawn, "Review spend").y,
            "{drawn:?}"
        );
    }

    #[test]
    fn the_busy_card_stands_beneath_the_button_its_command_came_from() {
        // While the worker signs, the card is under Sign & submit, which takes
        // no press (docs/DECISIONS.md D40 item 4).
        let model = busy(phone_at(
            stage_page(SendStage::Sign(plan()), typed()),
            NARROWEST,
        ));
        let drawn = drawn_on(&model);
        assert!(
            at(&drawn, "Waiting").y > at(&drawn, "Sign & submit").y,
            "{drawn:?}"
        );
        let sign = at(&drawn, "Sign & submit");
        let mut driver = Driver::new(screens::view(&model), (360, 844));
        let (messages, _, _) = driver.send(&[
            mouse_move(sign.center_x(), sign.center_y()),
            mouse_press(),
            mouse_release(),
        ]);
        assert!(messages.is_empty(), "{messages:?}");
    }

    #[test]
    fn the_desktop_keeps_w4_and_w5_as_one_page_each() {
        // The desktop's W4 has its form and "Review spend" on one page, with no
        // place in a flow; its W5 has the review, "Sign & submit" and "Edit the
        // spend" (docs/SCREENS.md W4 and W5). A desktop is never on the phone's
        // pages.
        let w4 = drawn_on(&desktop_at(compose(typed()), 1440.0));
        assert!(
            has(&w4, "Review spend") && has(&w4, "Add destination") && has(&w4, "FROM ACCOUNT"),
            "{w4:?}"
        );
        assert!(
            !has(&w4, "1 of 4") && !has(&w4, "Continue") && !has(&w4, "Remove"),
            "{w4:?}"
        );
        let w5 = drawn_on(&desktop_at(
            stage_page(SendStage::Review(plan()), typed()),
            1440.0,
        ));
        assert!(
            has(&w5, "Sign & submit") && has(&w5, "Edit the spend"),
            "{w5:?}"
        );
        assert!(
            !has(&w5, "3 of 4") && !has(&w5, "Continue to signing"),
            "{w5:?}"
        );
    }

    #[test]
    fn w6_fits_a_phone_screen_with_its_report_open() {
        // On a phone Save artifact asks the system for the share sheet, and the
        // page says so (docs/DECISIONS.md D32 item 10 (i)).
        let sent = SentPage {
            sent: sample::sent_view(),
            order: 0,
            resigned: false,
            saved: Some(Err(crate::app::SHARED.to_owned())),
            dismissing: false,
        };
        let model = phone(
            Page::Send(SendPage {
                from: Some(accounts()[0]),
                form: typed(),
                stage: SendStage::Sent(Box::new(sent)),
            }),
            Some(ReportKey::Sent),
        );
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
        let drawn = texts(screens::view(&model), (NARROWEST, 2400.0));
        assert!(
            drawn.iter().any(|(text, _)| text == crate::app::SHARED),
            "the page says the sheet was asked for"
        );
    }

    #[test]
    fn w6_offers_dismiss_only_for_unsaved_bytes_and_asks_before_it_drops_them() {
        // docs/DECISIONS.md D38 item 5, as revised on 2026-10-13: Dismiss is
        // for bytes not saved, and wipes the only copy Tawara holds, so it is
        // confirmed first, with what is lost said beside it.
        let drawn = |saved: Option<Result<std::path::PathBuf, String>>, dismissing: bool| {
            let sent = SentPage {
                sent: sample::sent_view(),
                order: 0,
                resigned: false,
                saved,
                dismissing,
            };
            let model = phone(
                Page::Send(SendPage {
                    from: Some(accounts()[0]),
                    form: typed(),
                    stage: SendStage::Sent(Box::new(sent)),
                }),
                Some(ReportKey::Sent),
            );
            assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
            texts(screens::view(&model), (NARROWEST, 3000.0))
                .into_iter()
                .map(|(text, _)| text)
                .collect::<Vec<_>>()
        };
        let has = |texts: &[String], what: &str| texts.iter().any(|t| t == what);

        let unsaved = drawn(None, false);
        assert!(has(&unsaved, "Dismiss"), "{unsaved:?}");
        assert!(has(&unsaved, UNSAVED_ARTIFACT));
        assert!(!has(&unsaved, DISMISS_WARNING));

        let asked = drawn(Some(Err(crate::app::SHARED.to_owned())), true);
        assert!(has(&asked, DISMISS_WARNING), "{asked:?}");
        assert!(has(&asked, "Dismiss the bytes"));
        assert!(has(&asked, "Keep them"));

        let saved = drawn(
            Some(Ok(std::path::PathBuf::from(
                "/home/you/Downloads/spend.hex",
            ))),
            false,
        );
        assert!(!has(&saved, "Dismiss"), "{saved:?}");
        assert!(!has(&saved, "Dismiss the bytes"));
        assert!(has(&saved, SAVED_ARTIFACT));
        assert!(!has(&saved, UNSAVED_ARTIFACT));
    }

    #[test]
    fn w8_fits_a_phone_screen() {
        let model = phone(
            Page::Resign(ResignPage {
                account: accounts()[1],
                form: typed(),
            }),
            None,
        );
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn w9_fits_a_phone_screen() {
        let model = phone(
            Page::Submit(SubmitPage {
                hex: sample::sent_view().artifact_hex,
                result: Some((true, sample::submitted_text())),
                ..SubmitPage::default()
            }),
            None,
        );
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn w9_in_resubmit_mode_says_what_is_accepted_and_what_a_write_establishes() {
        // docs/DECISIONS.md D38 item 5 (b): the page says that only the identical
        // saved copy is accepted, under the library's two conditions, and the
        // written copy's report says that the write establishes no more than
        // that. Both fit a phone.
        let written = "The saved copy was written to the node's socket.";
        let model = phone(
            Page::Submit(SubmitPage {
                hex: sample::sent_view().artifact_hex,
                mode: SubmitMode::Resubmit(accounts()[2]),
                result: Some((true, written.to_owned())),
            }),
            Some(ReportKey::Submitted),
        );
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
        let drawn: Vec<String> = texts(screens::view(&model), (NARROWEST, 844.0))
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        for line in [
            "Only the identical saved copy is accepted, and only while the balance at its source \
             has not moved and its block-to-live has not passed. The library checks the copy \
             before anything is written.",
            "Only the write is confirmed here. The node checks the copy after Tawara has gone, \
             and if it refuses, nothing says so here.",
        ] {
            assert!(drawn.iter().any(|text| text == line), "{line}: {drawn:?}");
        }
    }

    #[test]
    fn w9_of_an_open_reservation_keeps_its_copy_and_no_resubmit_copy() {
        // The copy follows the mode (docs/DECISIONS.md D38 item 5 (b)): the
        // bytes of an open reservation keep the copy they had. The command each
        // mode sends is tested with `submit_command`, in app/wallet.rs.
        let model = phone(
            Page::Submit(SubmitPage {
                hex: sample::sent_view().artifact_hex,
                ..SubmitPage::default()
            }),
            None,
        );
        let drawn: Vec<String> = texts(screens::view(&model), (NARROWEST, 844.0))
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        assert!(
            drawn.iter().any(|text| text
                == "Only the bytes' layout is checked here; the node validates the rest."),
            "{drawn:?}"
        );
        assert!(
            !drawn
                .iter()
                .any(|text| text.starts_with("Only the identical saved copy")),
            "{drawn:?}"
        );
    }

    #[test]
    fn on_a_phone_the_first_destination_takes_typing() {
        // Tab reaches the first destination's field, and what is typed into it
        // is that destination's (docs/DECISIONS.md D32 item 10 (b)).
        let model = phone_at(
            stage_page(SendStage::To(BTreeMap::new()), SpendForm::default()),
            NARROWEST,
        );
        let mut driver = Driver::new(screens::view(&model), (360, 844));
        driver.focus_next();
        let (messages, _, _) = driver.send(&[press(Key::Character("a".into()), Some("a"))]);
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Wallet(WalletMsg::DestinationTo(0, to))] if to == "a"
            ),
            "{messages:?}"
        );
    }

    /// Where each Paste button's label lies in `model`'s window at the
    /// phone's narrowest width.
    fn pastes(model: &Model) -> Vec<Rectangle> {
        texts(screens::view(model), (NARROWEST, 844.0))
            .into_iter()
            .filter(|(text, _)| text == "Paste")
            .map(|(_, at)| at)
            .collect()
    }

    #[test]
    fn on_a_phone_a_destination_takes_the_width_of_its_card_beside_its_paste() {
        // The destination has a block of its own, so its field is as wide as the
        // card's inside less the Paste button: a share of a row with the amount
        // and the reference would be well under the 200 pixels asked for here.
        let model = phone_at(
            stage_page(SendStage::To(BTreeMap::new()), typed()),
            NARROWEST,
        );
        let fields = focusable_bounds(screens::view(&model));
        let field = fields.first().copied().expect("the first destination");
        let paste = pastes(&model)[0];
        assert!(field.width >= 200.0, "{fields:?}");
        assert!(
            paste.x > field.x + field.width && paste.y < field.y + field.height,
            "the Paste button stands to the field's right: {paste:?}, {field:?}"
        );
    }

    #[test]
    fn on_a_phone_the_signed_bytes_field_takes_its_row_beside_its_paste() {
        let model = phone(
            Page::Submit(SubmitPage {
                hex: sample::sent_view().artifact_hex,
                result: None,
                ..SubmitPage::default()
            }),
            None,
        );
        let fields = focusable_bounds(screens::view(&model));
        let field = fields.first().copied().expect("the signed-bytes field");
        let paste = pastes(&model)[0];
        assert!(field.width >= 200.0, "{fields:?}");
        assert!(
            paste.x > field.x + field.width && paste.y < field.y + field.height,
            "{paste:?}, {field:?}"
        );
    }

    #[test]
    fn a_paste_button_stands_by_each_pasted_field_on_a_phone_and_none_on_the_desktop() {
        // Page 1 has a Paste for each destination, and the amounts, the review
        // and the signing have none. W8 and W9 keep theirs (docs/DECISIONS.md
        // D32 item 10 (g), D40 item 5).
        let pages = || {
            [
                (stage_page(SendStage::To(BTreeMap::new()), typed()), 2),
                (stage_page(SendStage::Amount, typed()), 0),
                (stage_page(SendStage::Review(plan()), typed()), 0),
                (stage_page(SendStage::Sign(plan()), typed()), 0),
                (
                    Page::Resign(ResignPage {
                        account: accounts()[1],
                        form: typed(),
                    }),
                    2,
                ),
                (Page::Submit(SubmitPage::default()), 1),
            ]
        };
        for (page, fields) in pages() {
            assert_eq!(pastes(&phone_at(page, NARROWEST)).len(), fields);
        }
        for (page, _) in pages() {
            let drawn = drawn_on(&desktop_at(page, 1440.0));
            assert!(
                !has(&drawn, "Paste"),
                "the desktop pastes from the keyboard"
            );
        }
    }

    #[test]
    fn a_tap_on_a_paste_button_asks_for_its_own_fields_paste() {
        let form = typed();
        let rows = form.rows.len();
        let model = phone_at(stage_page(SendStage::To(BTreeMap::new()), form), NARROWEST);
        let second = pastes(&model)[1];
        let mut driver = Driver::new(screens::view(&model), (360, 844));
        let (messages, _, _) = driver.send(&[
            mouse_move(second.center_x(), second.center_y()),
            mouse_press(),
            mouse_release(),
        ]);
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Paste(PasteInto::Destination {
                    form: crate::app::FormOf::Send(Some(from)),
                    row: 1,
                    rows: r,
                })] if *from == accounts()[0] && *r == rows
            ),
            "{messages:?}"
        );

        let model = phone(Page::Submit(SubmitPage::default()), None);
        let at = pastes(&model)[0];
        let mut driver = Driver::new(screens::view(&model), (360, 844));
        let (messages, _, _) = driver.send(&[
            mouse_move(at.center_x(), at.center_y()),
            mouse_press(),
            mouse_release(),
        ]);
        assert!(
            matches!(
                messages.as_slice(),
                [Message::Paste(PasteInto::SignedBytes)]
            ),
            "{messages:?}"
        );
    }

    /// The bounds of the children of `root` as the layout gives them at the
    /// phone's narrowest width, each relative to `root`. A row gives each child
    /// no more than the room left in it, so a child that does not fit is drawn
    /// narrower, not past the edge: `fits` cannot see that, and this can.
    fn child_bounds(mut root: Element<'_, Message>) -> Vec<Rectangle> {
        let renderer = crate::harness::renderer();
        let mut tree = Tree::new(root.as_widget());
        let limits = layout::Limits::new(iced::Size::ZERO, iced::Size::new(NARROWEST, 844.0));
        let node = root.as_widget_mut().layout(&mut tree, &renderer, &limits);
        node.children().iter().map(|child| child.bounds()).collect()
    }

    #[test]
    fn a_row_of_buttons_wraps_on_a_phone_and_stays_one_row_on_the_desktop() {
        // Two buttons 200 pixels across, which 360 cannot hold in one line: on a
        // phone the second goes onto a line of its own and keeps its width; the
        // desktop's row stays one row, as it has always been drawn.
        let wide = || -> Element<'static, Message> {
            space()
                .width(Length::Fixed(200.0))
                .height(Length::Fixed(40.0))
                .into()
        };
        let phone = child_bounds(button_row(true, vec![wide(), wide()]));
        assert!(
            phone[1].y > phone[0].y && phone.iter().all(|b| b.width == 200.0),
            "{phone:?}"
        );
        let desktop = child_bounds(button_row(false, vec![wide(), wide()]));
        assert_eq!(desktop[1].y, desktop[0].y, "{desktop:?}");
    }

    #[test]
    fn the_settle_step_is_done_once_the_account_is_in_sync_again() {
        let mut wallet = sample::wallet_view();
        let account = wallet.accounts[1].id;
        let mut model = Model::new(Preferences::default(), None);
        // Outstanding: the step waits.
        model.wallet = Some(wallet.clone());
        assert!(matches!(settle_step(&model, account), Step::Pending));
        // Landed and not yet settled: it waits for the settle that follows.
        wallet.accounts[1].state = AccountState::SpendLanded {
            balance: 1,
            spent_index: 1,
            settled_index: 2,
        };
        model.wallet = Some(wallet.clone());
        assert!(matches!(settle_step(&model, account), Step::Pending));
        // Settled: in sync again, and the step is done, as the live view says.
        wallet.accounts[1].state = AccountState::InSync { balance: 1 };
        model.wallet = Some(wallet.clone());
        assert!(matches!(settle_step(&model, account), Step::Done));
        // With no store open there is nothing that says it settled.
        model.wallet = None;
        assert!(matches!(settle_step(&model, account), Step::Pending));
    }
}
