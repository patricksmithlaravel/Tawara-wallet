//! The wallet, inside the sidebar of renderings 02 to 08.
//!
//! This pull request builds the shell (the sidebar, its network panel and
//! the lock) and the first of the wallet's pages: the accounts, as the
//! worker reports them, with the store's notice whole above them. The rest
//! of the dashboard (02), and the other pages the sidebar names, follow in
//! the next pull requests (docs/DECISIONS.md D27, item 16); their items are
//! shown and not yet enabled.

use iced::widget::{column, container, row, rule, scrollable, space};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::preferences::AmountUnit;
use tawara_wallet_core::view::{AccountKind, AccountRow, AccountState, ReservationState};

use crate::app::{Back, Go, Message, Model, WalletPage, minutes};
use crate::icon::{self, Icon};
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, t, ty};

/// The sidebar's items (5.3), in the renderings' order.
const NAV: [(Icon, &str); 6] = [
    (Icon::Wallet, "Wallet"),
    (Icon::Send, "Send"),
    (Icon::Receive, "Receive"),
    (Icon::Activity, "Activity"),
    (Icon::Cube, "Explorer"),
    (Icon::Sliders, "Settings"),
];

/// The wallet's page with the sidebar.
pub fn view<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    row![
        sidebar(model),
        rule::vertical(1).style(theme::divider),
        scrollable(
            container(content(model, page))
                .max_width(1240.0)
                .padding(Padding {
                    top: sp::S32,
                    right: sp::S40,
                    bottom: sp::S48,
                    left: sp::S40,
                })
                .center_x(Length::Fill),
        )
        .style(theme::scroll)
        .width(Length::Fill)
        .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

/// The sidebar (`design/TOKENS.md` 3.2): the wordmark, the items, and the
/// network panel with the lock at its foot.
fn sidebar(model: &Model) -> Element<'_, Message> {
    let mut nav = column![].spacing(sp::S4);
    for (i, (glyph, name)) in NAV.into_iter().enumerate() {
        let active = i == 0;
        let ink = if active {
            color::ACCENT
        } else if i == 0 {
            color::TEXT_SECONDARY
        } else {
            color::TEXT_MUTED
        };
        nav = nav.push(
            iced::widget::button(
                container(
                    row![
                        icon::icon(glyph, 20.0, 2.0, ink),
                        ui::label(name, if active { ty::NAV_ACTIVE } else { ty::NAV }),
                    ]
                    .spacing(sp::S12)
                    .align_y(Alignment::Center),
                )
                .center_y(Length::Fill),
            )
            .height(Length::Fixed(44.0))
            .width(Length::Fill)
            .padding(Padding::from([0.0, sp::S12]))
            .style(theme::button(theme::Button::Nav { active }))
            .on_press_maybe(active.then_some(Message::Go(Go::Wallet))),
        );
    }
    container(
        column![
            container(t("TAWARA", ty::WORDMARK, color::TEXT_PRIMARY))
                .padding(Padding::from([0.0, sp::S10])),
            nav,
            space().height(Length::Fill),
            network_panel(model),
        ]
        .spacing(sp::S32)
        .height(Length::Fill),
    )
    .padding(Padding::from([sp::S28, sp::S16]))
    .width(Length::Fixed(232.0))
    .height(Length::Fill)
    .style(theme::sidebar)
    .into()
}

/// The network panel (5.20): which node, its tip, how long it took to
/// answer, and the lock.
fn network_panel(model: &Model) -> Element<'_, Message> {
    let host = model
        .node
        .url
        .as_deref()
        .map(|u| {
            u.trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
        })
        .unwrap_or("No node chosen");
    let (dot, block, latency) = match (&model.node.tip, &model.node.error) {
        (Some((tip, took)), None) => (
            color::ACCENT,
            ui::group(&tip.to_string()),
            format!("{} ms", took.as_millis()),
        ),
        (_, Some(_)) => (color::WARNING, "—".to_owned(), "not answering".to_owned()),
        _ => (color::TEXT_MUTED, "—".to_owned(), "—".to_owned()),
    };
    let line = |name: &'static str, value: String| {
        row![
            t(name, ty::NOTE, color::TEXT_SECONDARY),
            space().width(Length::Fill),
            t(value, ty::MONO_SMALL, color::TEXT_PRIMARY),
        ]
        .align_y(Alignment::Center)
    };
    container(
        column![
            row![
                ui::dot(dot, 8.0),
                t(host, ty::TABLE_NAME, color::TEXT_PRIMARY),
            ]
            .spacing(sp::S8)
            .align_y(Alignment::Center),
            line("Block", block),
            line("Latency", latency),
            container(ui::button_with(
                "Lock wallet",
                theme::Button::Secondary,
                Size::Small,
                Some(Icon::Lock),
                Some(Message::Lock),
            ))
            .center_x(Length::Fill)
            .padding(Padding {
                top: sp::S4,
                ..Padding::ZERO
            }),
        ]
        .spacing(sp::S8),
    )
    .padding(sp::S14)
    .width(Length::Fill)
    .style(theme::sidebar_panel)
    .into()
}

/// The page itself: the header, the store's notice, and the accounts.
fn content<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let subtitle = format!(
        "Keystore unlocked · auto-locks after {} with nothing done",
        minutes(model.prefs.idle_lock)
    );
    let header = row![
        ui::page_header("Wallet", subtitle),
        space().width(Length::Fill),
        ui::button_with(
            "Node",
            theme::Button::Secondary,
            Size::Medium,
            Some(Icon::Server),
            Some(Message::Go(Go::Node(Back::Wallet))),
        ),
        ui::button_with(
            "Refresh",
            theme::Button::Secondary,
            Size::Medium,
            Some(Icon::Refresh),
            Some(Message::Refresh),
        ),
    ]
    .spacing(sp::S10)
    .align_y(Alignment::Center);
    let mut stack = column![header].spacing(sp::S24);
    let Some(wallet) = &model.wallet else {
        return stack.into();
    };
    if let Some(notice) = &wallet.notice {
        stack = stack.push(ui::library_page(notice));
    }
    if let Some(e) = &page.error {
        stack = stack.push(ui::refusal(e));
    }
    stack = stack.push(balance(model, wallet.total(), wallet.accounts.len()));
    stack.push(accounts(&wallet.accounts, unit)).into()
}

/// The total, as the 02 hero card sets it (5.22), with the number of
/// accounts.
fn balance(model: &Model, total: u128, count: usize) -> Element<'_, Message> {
    let unit = model.prefs.unit;
    let nano = u64::try_from(total).unwrap_or(u64::MAX);
    let shown = ui::amount(nano, unit);
    let (whole, frac) = match unit {
        AmountUnit::Mcm => shown
            .split_once('.')
            .map_or((shown.clone(), String::new()), |(w, f)| {
                (w.to_owned(), format!(".{f}"))
            }),
        AmountUnit::NanoMcm => (shown.clone(), String::new()),
    };
    container(
        column![
            ui::section_label("Total balance"),
            row![
                t(whole, ty::BALANCE, color::TEXT_PRIMARY),
                t(frac, ty::BALANCE_DECIMALS, color::TEXT_SECONDARY),
                container(t(ui::unit_name(unit), ty::BALANCE_UNIT, color::ACCENT)).padding(
                    Padding {
                        left: sp::S12,
                        ..Padding::ZERO
                    }
                ),
            ]
            .align_y(Alignment::End),
            t(
                format!("{} nanoMCM", ui::group(&nano.to_string())),
                ty::MONO_SMALL,
                color::TEXT_MUTED,
            ),
            ui::pill(
                if count == 1 {
                    "1 account".to_owned()
                } else {
                    format!("{count} accounts")
                },
                ty::CHIP,
                color::BG_RAISED,
                color::TEXT_SECONDARY,
                30.0,
            ),
        ]
        .spacing(sp::S12),
    )
    .padding(Padding::from([sp::S28, sp::S32]))
    .width(Length::Fill)
    .style(theme::hero_card)
    .into()
}

/// The accounts table (02, `design/TOKENS.md` 5.12).
fn accounts(rows: &[AccountRow], unit: AmountUnit) -> Element<'_, Message> {
    const ACCOUNT: f32 = 170.0;
    const BALANCE: f32 = 180.0;
    const KEY: f32 = 100.0;
    const STATUS: f32 = 230.0;
    let head = |label: &'static str, width: Length, right: bool| {
        container(t(label, ty::TABLE_HEADER, color::TEXT_MUTED))
            .width(width)
            .align_x(if right {
                Alignment::End
            } else {
                Alignment::Start
            })
    };
    let mut table = column![
        row![
            head("Account", Length::Fixed(ACCOUNT), false),
            head("Destination", Length::Fill, false),
            head(
                if matches!(unit, AmountUnit::Mcm) {
                    "Balance (MCM)"
                } else {
                    "Balance (nanoMCM)"
                },
                Length::Fixed(BALANCE),
                true,
            ),
            container(space()).width(Length::Fixed(sp::S24)),
            head("Next key", Length::Fixed(KEY), false),
            head("Status", Length::Fixed(STATUS), false),
        ]
        .padding(Padding::from([sp::S10, 0.0])),
        ui::divider(),
    ];
    for (n, account) in rows.iter().enumerate() {
        let destination = account.id.destination().unwrap_or_else(|| account.id.hex());
        let (status, ink) = status(&account.state);
        let balance = account
            .state
            .balance()
            .map_or_else(|| "—".to_owned(), |b| ui::amount(b, unit));
        table = table.push(
            row![
                column![
                    t(short(&destination), ty::TABLE_NAME, color::TEXT_PRIMARY),
                    t(
                        match account.kind {
                            AccountKind::Derived => "derived",
                            AccountKind::Imported => "imported",
                        },
                        ty::TINY,
                        color::TEXT_MUTED,
                    ),
                ]
                .spacing(sp::S2)
                .width(Length::Fixed(ACCOUNT)),
                t(destination, ty::MONO, color::TEXT_SECONDARY).width(Length::Fill),
                container(t(balance, ty::TABLE_AMOUNT, color::TEXT_PRIMARY))
                    .width(Length::Fixed(BALANCE))
                    .align_x(Alignment::End),
                container(space()).width(Length::Fixed(sp::S24)),
                t(format!("#{}", account.index), ty::MONO, color::TEXT_PRIMARY)
                    .width(Length::Fixed(KEY)),
                row![ui::dot(ink, 6.0), t(status, ty::TABLE_BODY, ink)]
                    .spacing(sp::S6)
                    .align_y(Alignment::Center)
                    .width(Length::Fixed(STATUS)),
            ]
            .align_y(Alignment::Center)
            .padding(Padding::from([sp::S14, 0.0])),
        );
        if n + 1 < rows.len() {
            table = table.push(ui::divider());
        }
    }
    container(column![ui::section_label("Accounts"), table].spacing(sp::S8))
        .padding(Padding {
            top: sp::S20,
            right: sp::S24,
            bottom: sp::S8,
            left: sp::S24,
        })
        .width(Length::Fill)
        .style(theme::card)
        .into()
}

/// A destination shortened as the renderings shorten one ("9xQmT4…3cYdAf"):
/// its first and last six characters.
fn short(destination: &str) -> String {
    let chars: Vec<char> = destination.chars().collect();
    if chars.len() <= 14 {
        return destination.to_owned();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 6..].iter().collect();
    format!("{head}…{tail}")
}

/// An account's state in a word or two, and its colour (1.7). The
/// library's report for a state that needs one is on the account's own
/// page.
fn status(state: &AccountState) -> (&'static str, iced::Color) {
    match state {
        AccountState::InSync { .. } => ("Reconciled", color::ACCENT),
        AccountState::SpendOutstanding {
            reservation: ReservationState::Dead { .. },
            ..
        } => ("Reservation cannot land", color::WARNING),
        AccountState::SpendOutstanding { .. } => ("Spend outstanding", color::WARNING),
        AccountState::SpendLanded { .. } => ("Landed: settle it", color::WARNING),
        AccountState::Diverged { .. } => ("Spending paused", color::WARNING),
        AccountState::NotReconciled => ("Not reconciled", color::TEXT_MUTED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_shorten_to_their_ends() {
        assert_eq!(short("9xQmT4vRbN2kWe7HpLsZ3cYdAf"), "9xQmT4…3cYdAf");
        assert_eq!(short("short"), "short");
    }
}
