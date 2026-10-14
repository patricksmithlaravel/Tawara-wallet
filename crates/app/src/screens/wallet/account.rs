//! W7, an account: its figures, and the panel for its state with the
//! actions that state allows (docs/PLAN.md section 4.9; docs/SCREENS.md,
//! "The states that fail closed"). Nothing on this page moves a key index:
//! a diverged account's remedy is the account-recovery screen, reached on
//! purpose (docs/DECISIONS.md D19, D27 item 3).

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};
use tawara_wallet_core::view::{AccountRow, AccountState, DivergenceKind, ReservationState};

use super::{action, copy, destination, figure, frame, kind, name, report, row_of, status};
use crate::app::{AccountPage, Message, Model, ReportKey, To, WalletMsg, WalletPage};
use crate::icon::Icon;
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, t, ty};

pub fn view<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    a: &'a AccountPage,
) -> Element<'a, Message> {
    let Some(account) = row_of(model, a.account) else {
        return frame(
            model,
            page,
            "Account",
            "This store holds no such account.",
            Vec::new(),
            Vec::new(),
        );
    };
    let unit = model.prefs.unit;
    let shown = destination(account.id);
    let (word, ink) = status(&account.state);
    let figures = ui::card_for(
        model.compact(),
        column![
            row![
                ui::section_label("Destination"),
                iced::widget::space().width(Length::Fill),
                copy(page, &shown, "Copy"),
            ]
            .align_y(Alignment::Center),
            t(shown.clone(), ty::MONO_WORD, color::TEXT_PRIMARY).wrapping(Wrapping::WordOrGlyph),
            ui::divider(),
            figure(
                "Balance",
                account.state.balance().map_or_else(
                    || "not known".to_owned(),
                    |b| format!("{} {}", ui::amount(b, unit), ui::unit_name(unit))
                )
            ),
            figure("Next key", format!("#{}", account.index)),
            figure("Kind", kind(account)),
            row![
                t("State", ty::BODY_SMALL, color::TEXT_SECONDARY).width(Length::Fill),
                ui::dot(ink, 6.0),
                t(word, ty::TABLE_BODY, ink),
            ]
            .spacing(sp::S6)
            .align_y(Alignment::Center),
        ]
        .spacing(sp::S12),
    );
    let mut body = vec![figures.into(), panel(model, page, account)];
    if let Some((done, text)) = &a.report {
        body.push(report::show_in(
            model.compact(),
            page,
            ReportKey::Done,
            report::done(*done, account, text),
        ));
    }
    // The name is part of a Base58 destination, so it is not set in the
    // title's capitals.
    frame(
        model,
        page,
        "Account",
        format!("{} · {}", name(account.id), kind(account)),
        vec![action(
            model,
            "Check now",
            theme::Button::Secondary,
            Size::Header,
            Some(Icon::Refresh),
            WalletMsg::Check(account.id),
        )],
        body,
    )
}

/// The panel for the account's state (section 4.9): what it means, and
/// the actions it allows.
fn panel<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    account: &'a AccountRow,
) -> Element<'a, Message> {
    let id = account.id;
    let button = |label: &'a str, variant, msg: WalletMsg| {
        action(model, label, variant, Size::Medium, None, msg)
    };
    let (callout, buttons): (Element<'a, Message>, Vec<Element<'a, Message>>) = match &account.state
    {
        AccountState::InSync { .. } if account.spendable => (
            ui::accent_callout(
                Icon::ShieldCheck,
                "Reconciled",
                "The chain holds this account at the key this store expects.",
            ),
            vec![
                button(
                    "Send from it",
                    theme::Button::Primary,
                    WalletMsg::Open(To::Send(Some(id))),
                ),
                button(
                    "Receive",
                    theme::Button::Secondary,
                    WalletMsg::Open(To::Receive(Some(id))),
                ),
            ],
        ),
        // In sync now, and set aside when the wallet opened: it spends only
        // once a refresh opens the wallet with it.
        AccountState::InSync { .. } => (
            ui::accent_callout(
                Icon::ShieldCheck,
                "Reconciled, not yet spendable",
                "The chain holds this account at the key this store expects. It was set aside \
                 when the wallet opened, so it spends again once Refresh on the wallet's page \
                 opens the wallet with it.",
            ),
            vec![button(
                "Receive",
                theme::Button::Secondary,
                WalletMsg::Open(To::Receive(Some(id))),
            )],
        ),
        AccountState::SpendOutstanding { reservation, .. } => {
            let why = match reservation {
                ReservationState::Live => {
                    "A spend from this account is signed and waiting to land. Settle once the \
                     chain shows it landed; until then the account does not spend again. If \
                     the signed bytes were lost, re-sign the same spend, or submit the bytes \
                     saved from the sent page."
                }
                // A dead reservation never settles, so its copy promises no
                // settle (docs/DECISIONS.md D38). Its last sentence is the
                // library's own, from its page on a dead reservation, reused
                // rather than rewritten (docs/PLAN.md section 3, D25).
                ReservationState::Dead {
                    balance_moved: true,
                    ..
                } => {
                    "This reservation can no longer be accepted: the balance it was built on \
                     has moved. Re-signing would reproduce bytes the ledger refuses. A dead \
                     reservation has no route out."
                }
                ReservationState::Dead { .. } => {
                    "This reservation can no longer be accepted: its block-to-live has passed. \
                     Re-signing would reproduce bytes the ledger refuses. A dead reservation \
                     has no route out."
                }
                ReservationState::Unrecorded => {
                    "A spend from this account is reserved, and the store records no figures \
                     for it (a store from before they were recorded). Check now for the wallet \
                     library's report; settle once the chain shows it landed."
                }
                ReservationState::Unclassified => {
                    "A spend from this account is reserved, and its expiry could not be read. \
                     Check now for the wallet library's report on it; settle once the chain \
                     shows it landed."
                }
            };
            let mut buttons = Vec::new();
            if settle_offered(&account.state) {
                buttons.push(button(
                    "Settle",
                    theme::Button::WarningPrimary,
                    WalletMsg::Settle(id),
                ));
            }
            buttons.push(button(
                "Re-sign",
                theme::Button::Secondary,
                WalletMsg::Open(To::Resign(id)),
            ));
            buttons.push(button(
                "Submit saved artifact",
                theme::Button::Secondary,
                WalletMsg::Open(To::Submit),
            ));
            (
                ui::warning_callout("Reserved and not settled", why),
                buttons,
            )
        }
        AccountState::SpendLanded { .. } => (
            ui::warning_callout(
                "The spend landed",
                "The chain shows the change key: the spend from this account landed. Settle \
                 records it and frees the account to spend again.",
            ),
            // The fallback for a settle the automatic step did not make
            // (docs/DECISIONS.md D38).
            vec![button(
                "Settle",
                theme::Button::WarningPrimary,
                WalletMsg::Settle(id),
            )],
        ),
        AccountState::Diverged {
            kind,
            report: text,
            advance_to,
        } => {
            let parts = column![report::show_in(
                model.compact(),
                page,
                ReportKey::Diverged,
                report::diverged(*kind, text)
            )]
            .spacing(sp::S12);
            // Account recovery is reached on purpose, from here or from
            // Settings, and moves nothing until the person types the index
            // and confirms (docs/DECISIONS.md D19).
            let recover = || {
                button(
                    "Account recovery",
                    theme::Button::WarningTonal,
                    WalletMsg::Open(To::Recovery(Some(id))),
                )
            };
            (
                parts.into(),
                match kind {
                    DivergenceKind::NotFound => vec![button(
                        "Receive to it again",
                        theme::Button::Secondary,
                        WalletMsg::Open(To::Receive(Some(id))),
                    )],
                    _ if advance_to.is_some() => vec![recover()],
                    DivergenceKind::Unlocated => vec![recover()],
                    _ => Vec::new(),
                },
            )
        }
        AccountState::NotReconciled => (
            ui::warning_callout(
                "Not reconciled",
                "No node has been asked about this account, so its balance and state are not \
                 known and it cannot spend. Choose a node, or Refresh once it answers.",
            ),
            Vec::new(),
        ),
    };
    let mut panel = column![callout].spacing(sp::S16);
    if !buttons.is_empty() {
        if model.compact() {
            // On a phone the buttons wrap onto the next line when the row
            // is too narrow for them (docs/SCREENS.md, "Compact layout"). Each
            // sits in a container with an id, so that the tests can find where
            // it is drawn.
            let buttons = buttons
                .into_iter()
                .enumerate()
                .map(|(n, b)| Element::from(container(b).id(button_id(n))));
            panel = panel.push(
                iced::widget::Row::with_children(buttons)
                    .spacing(sp::S10)
                    .wrap()
                    .vertical_spacing(sp::S10),
            );
        } else {
            let mut line = row![].spacing(sp::S10);
            for b in buttons {
                line = line.push(b);
            }
            panel = panel.push(line);
        }
    }
    ui::card_for(model.compact(), panel).into()
}

/// Whether the account's panel offers Settle (docs/DECISIONS.md D38): on a
/// landed spend, where it is the fallback for a settle the automatic step did
/// not make, and on an outstanding spend whose reservation can still land. A
/// dead reservation never settles, so the button is not offered for one.
fn settle_offered(state: &AccountState) -> bool {
    match state {
        AccountState::SpendLanded { .. } => true,
        AccountState::SpendOutstanding { reservation, .. } => {
            !matches!(reservation, ReservationState::Dead { .. })
        }
        _ => false,
    }
}

/// The id of the `n`-th button of a state panel on a phone. It draws nothing:
/// the tests find where the button is drawn by it.
fn button_id(n: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("account-panel-button-{n}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AccountPage, Level, NARROWEST, Page, Screen};
    use crate::harness::fits;
    use iced::advanced::widget::Operation;
    use iced_runtime::user_interface::{Cache, UserInterface};
    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::sample;
    use tawara_wallet_core::view::{AccountId, WalletView};

    /// A phone's window, tall enough that the whole panel is drawn.
    const TALL: (u32, u32) = (360, 1200);

    /// The bounds in the window of the container that carries `id`, as `root`
    /// draws it in a window of `size`.
    fn bounds_of(
        root: Element<'_, Message>,
        size: (u32, u32),
        id: &iced::widget::Id,
    ) -> iced::Rectangle {
        find_bounds(root, size, id).expect("the container is drawn")
    }

    /// The bounds of the container that carries `id`, or `None` when `root`
    /// draws none.
    fn find_bounds(
        root: Element<'_, Message>,
        size: (u32, u32),
        id: &iced::widget::Id,
    ) -> Option<iced::Rectangle> {
        struct Find<'a> {
            id: &'a iced::widget::Id,
            found: Option<iced::Rectangle>,
        }
        impl Operation for Find<'_> {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }

            fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
                if id == Some(self.id) {
                    self.found = Some(bounds);
                }
            }
        }
        let mut renderer = crate::harness::renderer();
        let bounds = iced::Size::new(size.0 as f32, size.1 as f32);
        let mut ui = UserInterface::build(root, bounds, Cache::default(), &mut renderer);
        let mut find = Find { id, found: None };
        ui.operate(&renderer, &mut find);
        find.found
    }

    /// The store `wallet` on W7 for `account`, in a phone 360 logical pixels
    /// wide, with one report of the page opened when `open` is given.
    fn phone(wallet: WalletView, account: AccountId, open: Option<(ReportKey, Level)>) -> Model {
        let mut model = Model::new(Preferences::default(), None);
        model.width = NARROWEST;
        model.wallet = Some(wallet);
        let mut page = WalletPage {
            page: Page::Account(AccountPage {
                account,
                report: None,
            }),
            ..WalletPage::default()
        };
        if let Some((key, level)) = open {
            page.open.insert(key, level);
        }
        model.screen = Screen::Wallet(page);
        model
    }

    #[test]
    fn the_phone_account_pages_fit_at_360_in_each_state_the_example_draws() {
        let wallet = sample::wallet_view();
        let accounts = wallet.accounts.clone();
        // The second account in sync now, and set aside when the wallet opened,
        // as the example draws it.
        let mut aside = sample::wallet_view();
        aside.accounts[1].state = AccountState::InSync {
            balance: 3_180_000_000_000,
        };
        aside.accounts[1].spendable = false;
        let cases = [
            ("reconciled", phone(wallet.clone(), accounts[0].id, None)),
            ("outstanding", phone(wallet.clone(), accounts[1].id, None)),
            ("set aside", phone(aside, accounts[1].id, None)),
            ("diverged", phone(wallet.clone(), accounts[2].id, None)),
            (
                "diverged, summarized",
                phone(
                    wallet.clone(),
                    accounts[2].id,
                    Some((ReportKey::Diverged, Level::Summary)),
                ),
            ),
            (
                "diverged, whole",
                phone(
                    wallet.clone(),
                    accounts[2].id,
                    Some((ReportKey::Diverged, Level::Full)),
                ),
            ),
        ];
        for (label, model) in cases {
            assert_eq!(
                fits(crate::screens::view(&model), NARROWEST),
                Ok(()),
                "{label}"
            );
        }
    }

    #[test]
    fn the_landed_and_dead_panels_fit_at_360() {
        let accounts = sample::wallet_view().accounts;
        let (landed, dead) = landed_and_dead();
        for (label, wallet) in [("landed", landed), ("dead", dead)] {
            let model = phone(wallet, accounts[1].id, None);
            assert_eq!(
                fits(crate::screens::view(&model), NARROWEST),
                Ok(()),
                "{label}"
            );
        }
    }

    #[test]
    fn a_landed_spend_offers_settle_and_a_dead_reservation_does_not() {
        // On a phone each button of the panel is drawn with its id: a landed
        // spend has Settle alone; a dead reservation has Re-sign and Submit
        // saved artifact, and no third button.
        let accounts = sample::wallet_view().accounts;
        let (landed, dead) = landed_and_dead();
        let drawn = |wallet: WalletView| {
            let model = phone(wallet, accounts[1].id, None);
            (0..3)
                .filter(|&n| {
                    find_bounds(crate::screens::view(&model), TALL, &button_id(n)).is_some()
                })
                .count()
        };
        assert_eq!(drawn(landed), 1, "Settle, the fallback");
        assert_eq!(drawn(dead), 2, "no Settle");
    }

    #[test]
    fn settle_is_offered_for_a_landed_spend_and_a_live_one_never_for_a_dead_one() {
        let landed = AccountState::SpendLanded {
            balance: 1,
            spent_index: 1,
            settled_index: 2,
        };
        let outstanding = |reservation| AccountState::SpendOutstanding {
            balance: 1,
            spent_index: 1,
            reservation,
        };
        let dead = |balance_moved, expired| {
            outstanding(ReservationState::Dead {
                balance_moved,
                expired,
            })
        };
        assert!(settle_offered(&landed));
        assert!(settle_offered(&outstanding(ReservationState::Live)));
        assert!(settle_offered(&outstanding(ReservationState::Unrecorded)));
        assert!(!settle_offered(&dead(true, false)));
        assert!(!settle_offered(&dead(false, true)));
        assert!(!settle_offered(&AccountState::InSync { balance: 1 }));
    }

    /// The sample store with the second account's spend landed, and the same
    /// store with that account's reservation dead, the balance it was built
    /// on having moved.
    fn landed_and_dead() -> (WalletView, WalletView) {
        let mut dead = sample::wallet_view();
        dead.accounts[1].state = AccountState::SpendOutstanding {
            balance: 3_180_000_000_000,
            spent_index: 6,
            reservation: ReservationState::Dead {
                balance_moved: true,
                expired: false,
            },
        };
        (sample::landed_view(), dead)
    }

    #[test]
    fn on_a_phone_the_state_buttons_wrap_onto_a_second_line() {
        // Outstanding: Settle, Re-sign and Submit saved artifact. The first two
        // share a line, and the third is too wide for that line at 360, so it
        // wraps. `fits` cannot see this: a row squeezes a child rather than
        // pushing it past the edge.
        let wallet = sample::wallet_view();
        let model = phone(wallet.clone(), wallet.accounts[1].id, None);
        let at = |n: usize| bounds_of(crate::screens::view(&model), TALL, &button_id(n));
        let (first, second, third) = (at(0), at(1), at(2));
        assert_eq!(second.y, first.y, "Re-sign sits beside Settle");
        assert!(
            third.y > first.y,
            "Submit saved artifact wraps onto a line of its own"
        );
    }

    #[test]
    fn a_reverted_settle_is_offered_no_button_and_no_remedy() {
        // docs/DECISIONS.md D38 item 5: a settle that a reorganisation
        // reverted leaves the account one key behind the store, with no
        // reservation. Nothing here may move a key index (PLAN 4.9), and
        // Submit saved artifact belongs to an open reservation, so the panel
        // draws the report and no button, and account recovery offers nothing.
        let mut wallet = sample::wallet_view();
        let id = wallet.accounts[2].id;
        wallet.accounts[2].state = AccountState::Diverged {
            kind: DivergenceKind::Behind { gap: 1 },
            report: "This store recorded a SETTLE at index 4".to_owned(),
            advance_to: None,
        };
        assert_eq!(crate::app::remedy(&wallet.accounts[2].state), None);
        let model = phone(wallet, id, None);
        assert_eq!(
            find_bounds(crate::screens::view(&model), TALL, &button_id(0)),
            None,
            "a Behind account draws no button"
        );
    }
}
