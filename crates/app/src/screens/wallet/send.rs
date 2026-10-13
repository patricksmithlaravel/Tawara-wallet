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
use iced::widget::{Row, checkbox, column, container, row, space, text_input};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::view::{AccountId, AccountState};
use tawara_wallet_core::{MAX_DESTINATIONS, PlanView, REFERENCE_RULE};

use super::{account_choice, action, copy, figure, frame, name, pair, report, row_of};
use crate::app::{
    Message, Model, PasteInto, ReportKey, ResignPage, SendPage, SendStage, SentPage, SpendForm,
    SubmitPage, To, WalletMsg, WalletPage,
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
            // The number and the amount on one line, then the destination and
            // the reference, each wrapping at its glyphs.
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
        ui::card_for(compact, summary),
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
    let mut buttons = vec![
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
    // Dismiss drops bytes that are not saved (docs/DECISIONS.md D38 item 5, as
    // revised on 2026-10-13); saved bytes need no dismissal.
    if s.unsaved() {
        buttons.push(action(
            model,
            "Dismiss",
            theme::Button::Secondary,
            Size::Medium,
            None,
            WalletMsg::DismissSigned(sent.artifact_hex.clone()),
        ));
    }
    let mut artifact = column![
        ui::section_label("Retry artifact"),
        t(
            "The signed bytes. The store does not keep them. Tawara keeps them until they are \
             saved, dismissed or the wallet locks, and a reorganisation of the chain can revert \
             a settle, so save them before going further.",
            ty::BODY_SMALL,
            color::TEXT_SECONDARY,
        ),
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
    let helper = ui::helper("Only the bytes' layout is checked here; the node validates the rest.");
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
        body.push(report::show_in(
            compact,
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
    use crate::app::{DestinationRow, Level, NARROWEST, Page, Screen};
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

    #[test]
    fn w4_with_one_destination_fits_a_phone_screen() {
        let model = phone(compose(typed()), None);
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn w4_with_three_destinations_fits_a_phone_screen() {
        let mut form = typed();
        form.rows.push(DestinationRow {
            to: form.rows[0].to.clone(),
            amount: "1".to_owned(),
            reference: "PAYROLL-13".to_owned(),
        });
        let model = phone(compose(form), None);
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn w4_that_cannot_spend_fits_a_phone_screen() {
        // The second account set aside when the wallet opened, as the sample
        // draws it (`w4-send-cannot-spend`).
        let mut wallet = sample::wallet_view();
        wallet.accounts[1].state = AccountState::InSync {
            balance: 3_180_000_000_000,
        };
        wallet.accounts[1].spendable = false;
        let mut model = phone(
            Page::Send(SendPage {
                from: Some(accounts()[1]),
                form: typed(),
                stage: SendStage::Compose,
            }),
            None,
        );
        model.wallet = Some(wallet);
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn w5_fits_a_phone_screen_with_its_report_open() {
        let model = phone(
            Page::Send(SendPage {
                from: Some(accounts()[0]),
                form: typed(),
                stage: SendStage::Review(Box::new(sample::plan_view())),
            }),
            Some(ReportKey::Plan),
        );
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn w6_fits_a_phone_screen_with_its_report_open() {
        // On a phone Save artifact asks the system for the share sheet, and the
        // page says so (docs/DECISIONS.md D32 item 10 (i)).
        let sent = SentPage {
            sent: sample::sent_view(),
            resigned: false,
            saved: Some(Err(crate::app::SHARED.to_owned())),
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
            }),
            None,
        );
        assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()));
    }

    #[test]
    fn on_a_phone_the_first_destination_takes_typing() {
        // Tab reaches the first destination's field, and what is typed into it
        // is that destination's (docs/DECISIONS.md D32 item 10 (b)).
        let model = phone(compose(SpendForm::default()), None);
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
        let model = phone(compose(typed()), None);
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
        // W4 and W8 with the sample's two destinations, and W9 (docs/DECISIONS.md
        // D32 item 10 (g)).
        let pages = || {
            [
                (compose(typed()), 2),
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
            assert_eq!(pastes(&phone(page, None)).len(), fields);
        }
        for (page, _) in pages() {
            let mut desktop = phone(page, None);
            desktop.width = 1440.0;
            let drawn = texts(screens::view(&desktop), (1440.0, 1200.0));
            assert!(
                !drawn.iter().any(|(text, _)| text == "Paste"),
                "the desktop pastes from the keyboard"
            );
        }
    }

    #[test]
    fn a_tap_on_a_paste_button_asks_for_its_own_fields_paste() {
        let form = typed();
        let rows = form.rows.len();
        let model = phone(compose(form), None);
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
