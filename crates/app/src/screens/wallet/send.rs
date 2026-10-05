//! The send flow (rendering 03; docs/SCREENS.md W4 to W6), re-signing the
//! reserved spend (W8), and submitting a saved artifact (W9).
//!
//! The person composes, then reads the library's "NOT SIGNED" page, then
//! signs on a step of its own: there is no password field, because the
//! store is open and the library's page is the confirmation
//! (docs/DECISIONS.md D27, item 3). After signing, the library's page with
//! its three facts is shown whole, and the signed bytes, which the store
//! does not keep, can be saved or copied (D27, item 5).

use iced::widget::text::Wrapping;
use iced::widget::{checkbox, column, container, row, space, text_input};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::{MAX_DESTINATIONS, PlanView, REFERENCE_RULE};

use super::{account_choice, action, copy, figure, frame, name, pair, report, row_of};
use crate::app::{
    Message, Model, ReportKey, ResignPage, SendPage, SendStage, SentPage, SpendForm, SubmitPage,
    To, WalletMsg, WalletPage,
};
use crate::fonts;
use crate::icon::Icon;
use crate::theme::{self, color, radius, space as sp};
use crate::ui::{self, Size, t, ty};

/// The node's floor for the fee, per destination, in nanoMCM: what the fee
/// is when the person gives none (`tawara_wallet_core::spend`).
const FEE_FLOOR: u64 = 500;

/// W4 to W6.
pub fn view<'a>(model: &'a Model, page: &'a WalletPage, s: &'a SendPage) -> Element<'a, Message> {
    match &s.stage {
        SendStage::Compose => compose(model, page, s),
        SendStage::Review(plan) => review(model, page, plan),
        SendStage::Sent(sent) => sent_page(model, page, sent),
    }
}

/// W4: from which account, to whom, how much, with which references.
fn compose<'a>(model: &'a Model, page: &'a WalletPage, s: &'a SendPage) -> Element<'a, Message> {
    let spendable: Vec<_> = model
        .wallet
        .iter()
        .flat_map(|w| w.accounts.iter())
        .filter(|a| a.spendable)
        .map(|a| a.id)
        .collect();
    // The account asked for, when it cannot spend now: it stays the one
    // chosen, and the page says why, rather than spending from another.
    let blocked = s.from.filter(|f| !spendable.contains(f));
    let mut form = column![ui::section_label("From account")].spacing(sp::S12);
    if let Some(id) = blocked {
        form = form.push(ui::warning_callout(
            "The account chosen cannot spend now",
            format!(
                "{} spends once the wallet has reconciled it on opening and it is in sync; an \
                 account set aside when the wallet opened spends again after Refresh. Choose \
                 another account, or Refresh the wallet first.",
                name(id)
            ),
        ));
    } else if spendable.is_empty() {
        form = form.push(ui::warning_callout(
            "No account can spend now",
            "A spend needs the wallet open against the node, from an account that reconciled \
             and is in sync. Refresh, or open an account's page from the wallet to see why it \
             cannot.",
        ));
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
        steps(Step::Pending, Step::Pending, Step::Pending),
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
            ui::card(form.spacing(sp::S20)).into(),
            summary.into(),
            7,
            4,
        )],
    )
}

/// The destinations as typed, the "everything" choice, and the fee and
/// block-to-live: W4's form and W8's.
fn destinations<'a>(model: &'a Model, f: &'a SpendForm) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let field = |value: &'a str, placeholder: &'a str, mono: bool| {
        let face = if mono { ty::MONO } else { ty::FIELD };
        text_input(placeholder, value)
            .font(face.font)
            .size(face.size)
            .padding(Padding::from([sp::S12, sp::S14]))
            .style(theme::field(radius::R12))
    };
    let mut list = column![
        row![
            t("#", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::Fixed(20.0)),
            t("Destination", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::FillPortion(6)),
            // Typed in MCM whatever unit the wallet shows, exactly to nine
            // places, so a figure is never read in the wrong unit.
            t("Amount (MCM)", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::FillPortion(3)),
            t("Reference", ty::TABLE_HEADER, color::TEXT_MUTED).width(Length::FillPortion(3)),
            space().width(Length::Fixed(36.0)),
        ]
        .spacing(sp::S12),
    ]
    .spacing(sp::S10);
    let several = f.rows.len() > 1;
    for (i, r) in f.rows.iter().enumerate() {
        let mut to = field(&r.to, "Base58 destination", true);
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
        list = list.push(
            row![
                t((i + 1).to_string(), ty::MONO, color::TEXT_MUTED).width(Length::Fixed(20.0)),
                container(to).width(Length::FillPortion(6)),
                container(amount).width(Length::FillPortion(3)),
                container(reference).width(Length::FillPortion(3)),
                iced::widget::button(crate::icon::icon(Icon::Trash, 16.0, 2.0, color::TEXT_MUTED))
                    .padding(sp::S10)
                    .style(theme::button(theme::Button::Ghost))
                    .on_press_maybe(
                        (idle && several).then_some(WalletMsg::RemoveDestination(i).into())
                    ),
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center),
        );
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
    part.push(
        row![
            column![
                t(
                    "Fee, nanoMCM in total",
                    ty::FORM_LABEL,
                    color::TEXT_SECONDARY
                ),
                fee
            ]
            .spacing(sp::S6)
            .width(Length::Fill),
            column![
                t("Block-to-live", ty::FORM_LABEL, color::TEXT_SECONDARY),
                btl
            ]
            .spacing(sp::S6)
            .width(Length::Fill),
        ]
        .spacing(sp::S12),
    )
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
    ui::card(card).into()
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

/// The three steps of a spend (03): reserving the key, submitting to the
/// node's socket, and settling once the chain shows the change. They show
/// what has happened, nothing more (docs/SCREENS.md W6).
fn steps<'a>(reserve: Step, submit: Step, settle: Step) -> Element<'a, Message> {
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
    ui::card(
        row![
            step("Reserve", "key index saved", reserve),
            step("Submit", "to the node's socket", submit),
            step("Settle", "change on the ledger", settle),
        ]
        .spacing(sp::S12),
    )
    .into()
}

/// W5: the library's page for the spend, not signed, whole; then signing,
/// on its own step.
fn review<'a>(model: &'a Model, page: &'a WalletPage, plan: &'a PlanView) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let index = row_of(model, plan.from).map(|a| a.index);
    // What to check, one destination each, as they go on the wire.
    let mut list = column![
        ui::section_label("Check each destination against its payee"),
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
    ]
    .spacing(sp::S12);
    for (i, d) in plan.destinations.iter().enumerate() {
        list = list.push(ui::divider()).push(
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
            .align_y(Alignment::Center),
        );
    }
    let main = column![
        report::show(page, ReportKey::Plan, report::plan(plan, index)),
        ui::card(list),
    ]
    .spacing(sp::S16);
    let count = plan.destinations.len();
    let summary = column![
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
    .spacing(sp::S12);
    let side = column![
        ui::card(summary),
        key_callout(model, Some(plan.from)),
        steps(Step::Pending, Step::Pending, Step::Pending),
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

/// W6: what happened, the bytes the store does not keep, and the library's
/// page with its three facts, whole.
fn sent_page<'a>(model: &'a Model, page: &'a WalletPage, s: &'a SentPage) -> Element<'a, Message> {
    let sent = &s.sent;
    let submitted = if sent.submitted {
        Step::Done
    } else {
        Step::Failed
    };
    let mut artifact = column![
        ui::section_label("Retry artifact"),
        t(
            "The signed bytes. The store does not keep them, and while the reservation is \
             open they are the only bytes that can move these funds: save them before going \
             further.",
            ty::BODY_SMALL,
            color::TEXT_SECONDARY,
        ),
        row![
            action(
                model,
                "Save artifact",
                theme::Button::Primary,
                Size::Medium,
                Some(Icon::Download),
                WalletMsg::SaveArtifact,
            ),
            copy(page, &sent.artifact_hex, "Copy hex"),
        ]
        .spacing(sp::S10),
    ]
    .spacing(sp::S12);
    match &s.saved {
        Some(Ok(path)) => {
            artifact = artifact.push(
                t(
                    format!("Saved to {}", path.display()),
                    ty::MONO_SMALL,
                    color::ACCENT,
                )
                .wrapping(Wrapping::WordOrGlyph),
            );
        }
        Some(Err(e)) => artifact = artifact.push(ui::refusal(e)),
        None => {}
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
    let body = vec![
        steps(Step::Done, submitted, Step::Pending),
        report::show(page, ReportKey::Sent, report::sent(sent)),
        ui::card(artifact).into(),
        row![
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
        ]
        .spacing(sp::S10)
        .into(),
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
        steps(Step::Done, Step::Pending, Step::Pending),
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
        vec![pair(model, ui::card(form).into(), side.into(), 7, 4)],
    )
}

/// W9: signed bytes saved earlier, written to the node's socket again. No
/// store is used.
pub fn submit<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    s: &'a SubmitPage,
) -> Element<'a, Message> {
    let mut hex = text_input("the hex the sent page saved or copied", &s.hex)
        .font(ty::MONO_SMALL.font)
        .size(ty::MONO_SMALL.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .style(theme::field(radius::R12));
    if model.busy.is_none() {
        hex = hex
            .on_input(|v| WalletMsg::ArtifactHex(v).into())
            .on_submit(WalletMsg::Submit.into());
    }
    let mut body = vec![
        ui::card(
            column![
                t(
                    "Signed bytes, as hex",
                    ty::FORM_LABEL,
                    color::TEXT_SECONDARY
                ),
                hex,
                row![
                    action(
                        model,
                        "Submit",
                        theme::Button::Primary,
                        Size::Medium,
                        None,
                        WalletMsg::Submit,
                    ),
                    ui::helper(
                        "Only the bytes' layout is checked here; the node validates the rest.",
                    ),
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center),
            ]
            .spacing(sp::S12),
        )
        .into(),
    ];
    if let Some((accepted, text)) = &s.result {
        body.push(report::show(
            page,
            ReportKey::Submitted,
            report::submitted(*accepted, text),
        ));
    }
    frame(
        model,
        page,
        "Submit saved artifact",
        "A signed spend saved earlier, written to the node's socket again. Nothing is signed.",
        Vec::new(),
        body,
    )
}
