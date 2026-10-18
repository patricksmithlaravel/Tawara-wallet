//! The wallet, inside the sidebar of renderings 02 to 08: the shell (the
//! sidebar, its network panel and the lock), and the pages of
//! docs/SCREENS.md W1 to W9, one module each. Activity, the explorer and
//! settings follow in the next pull requests (docs/DECISIONS.md D27, item
//! 16); their sidebar items are shown and not yet enabled.

mod account;
mod activity;
mod add_account;
mod dashboard;
mod explorer;
mod receive;
mod recovery;
mod report;
mod send;
mod settings;

use iced::widget::{column, container, row, rule, scrollable, space};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::view::{
    AccountId, AccountKind, AccountRow, AccountState, ReservationState,
};

use crate::app::{
    Back, Busy, Go, Message, Model, Page, ReportKey, SentPage, Signed, To, WalletMsg, WalletPage,
};
use crate::host::DeviceCheck;
use crate::icon::{self, Icon};
use crate::tap::tap;
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, t, ty};

/// The sidebar's items (5.3), in the renderings' order, and where each
/// leads.
const NAV: [(Icon, &str, Option<To>); 6] = [
    (Icon::Wallet, "Wallet", Some(To::Dashboard)),
    (Icon::Send, "Send", Some(To::Send(None))),
    (Icon::Receive, "Receive", Some(To::Receive(None))),
    (Icon::Activity, "Activity", Some(To::Activity)),
    (Icon::Cube, "Explorer", Some(To::Explorer)),
    (Icon::Sliders, "Settings", Some(To::Settings)),
];

/// The wallet's page with the sidebar.
pub fn view<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    let content: Element<'a, Message> = match &page.page {
        // A scan that draws its camera's preview in the application stands its
        // page in place of the page it began on, which is back once it ends
        // (docs/DECISIONS.md D42 item 3).
        _ if model.previewing() => send::scanner(model, page),
        Page::Dashboard => dashboard::view(model, page),
        Page::Receive(r) => receive::view(model, page, r),
        Page::AddAccount(a) => add_account::view(model, page, a),
        Page::Send(s) => send::view(model, page, s),
        Page::Resign(r) => send::resign(model, page, r),
        Page::Submit(s) => send::submit(model, page, s),
        Page::Account(a) => account::view(model, page, a),
        Page::Activity(a) => activity::view(model, page, a),
        Page::Settings(s) => settings::view(model, page, s),
        Page::Recovery(r) => recovery::view(model, page, r),
        Page::Explorer(e) => explorer::overview(model, page, e),
        Page::Block(b) => explorer::block(model, page, b),
        Page::Tag(t) => explorer::tag(model, page, t),
        Page::Transaction(t) => explorer::transaction(model, page, t),
        Page::Queue(q) => explorer::queue_page(model, page, q),
    };
    if model.compact() {
        return compact_frame(model, page, content);
    }
    row![
        sidebar(model, page),
        rule::vertical(1).style(theme::divider),
        scrollable(
            container(content)
                .max_width(1240.0)
                .padding(Padding {
                    top: sp::S32,
                    right: sp::S40,
                    bottom: sp::S48,
                    left: sp::S40,
                })
                .center_x(Length::Fill),
        )
        // The page has the id the phone's page has, so that entering a review
        // scrolls it to its top on the desktop too (docs/SCREENS.md W5).
        .id(super::page_id())
        .style(theme::scroll)
        .width(Length::Fill)
        .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

/// The sidebar (`design/TOKENS.md` 3.2): the wordmark, the items, and the
/// network panel with the lock at its foot. While the page waits on the
/// worker, no item leads anywhere.
fn sidebar<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let here = page.page.nav();
    let mut nav = column![].spacing(sp::S4);
    for (i, (glyph, name, to)) in NAV.into_iter().enumerate() {
        let active = i == here;
        let ink = if active {
            color::ACCENT
        } else if to.is_some() {
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
            .on_press_maybe(to.filter(|_| idle).map(|to| WalletMsg::Open(to).into())),
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
/// answer, a way to change it, and the lock.
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
    let change: Element<'_, Message> = if model.busy.is_none() {
        ui::link(
            "Change node",
            ty::LINK_SMALL,
            Message::Go(Go::Node(Back::Wallet)),
        )
        .into()
    } else {
        t("Change node", ty::LINK_SMALL, color::TEXT_MUTED).into()
    };
    container(
        column![
            row![
                ui::dot(dot, 8.0),
                t(host, ty::TABLE_NAME, color::TEXT_PRIMARY).width(Length::Fill),
            ]
            .spacing(sp::S8)
            .align_y(Alignment::Center),
            line("Block", block),
            line("Latency", latency),
            change,
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

/// The phone's wallet (docs/SCREENS.md, "Compact layout"): a bar at the top,
/// the page between margins of 16, and, while the keyboard is not up, a bar at
/// the foot with the six items (docs/DECISIONS.md D32 item 10 (b)). The page is
/// a scrollable with [`super::page_id`], so the screenshot example can scroll
/// it to its end.
fn compact_frame<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let scrolled = scrollable(
        container(content)
            .padding(Padding {
                top: sp::S16,
                right: sp::S16,
                bottom: sp::S24,
                left: sp::S16,
            })
            .width(Length::Fill),
    )
    .id(super::page_id())
    .style(theme::scroll)
    .width(Length::Fill)
    .height(Length::Fill);
    let mut shell = column![
        top_bar(model),
        rule::horizontal(1).style(theme::divider),
        container(scrolled)
            .style(theme::page)
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .height(Length::Fill);
    // The keyboard takes the foot of the window: the bar is not drawn under it.
    if model.safe_area.keyboard <= 0.0 {
        shell = shell
            .push(rule::horizontal(1).style(theme::divider))
            .push(tab_bar(model, page));
    }
    shell.into()
}

/// The phone's top bar (docs/SCREENS.md, "Compact layout"): the wordmark, the
/// node chip, and Lock.
fn top_bar(model: &Model) -> Element<'_, Message> {
    container(
        row![
            t("TAWARA", ty::WORDMARK, color::TEXT_PRIMARY),
            space().width(Length::Fill),
            node_chip(model),
            ui::button_with(
                "Lock",
                theme::Button::Secondary,
                Size::Small,
                Some(Icon::Lock),
                Some(Message::Lock),
            ),
        ]
        .spacing(sp::S8)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, sp::S16]))
    .width(Length::Fill)
    .center_y(Length::Fixed(56.0))
    .style(theme::sidebar)
    .into()
}

/// The node chip (docs/SCREENS.md, "Compact layout"): the node's dot and its
/// chain tip, grouped, or a dash. It goes to the node screen while the wallet
/// is idle, as the sidebar's "Change node" does.
fn node_chip(model: &Model) -> Element<'_, Message> {
    let (dot, tip) = match (&model.node.tip, &model.node.error) {
        (Some((tip, _)), None) => (color::ACCENT, ui::group(&tip.to_string())),
        (_, Some(_)) => (color::WARNING, "—".to_owned()),
        _ => (color::TEXT_MUTED, "—".to_owned()),
    };
    iced::widget::button(
        row![
            ui::dot(dot, 8.0),
            t(tip, ty::MONO_SMALL, color::TEXT_PRIMARY)
        ]
        .spacing(sp::S8)
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(36.0))
    .padding(Padding::from([0.0, sp::S8]))
    .style(theme::button(theme::Button::Ghost))
    .on_press_maybe(
        model
            .busy
            .is_none()
            .then_some(Message::Go(Go::Node(Back::Wallet))),
    )
    .into()
}

/// The phone's bar at the foot (docs/SCREENS.md, "Compact layout"): the six
/// items of the sidebar, each an icon over its name, sharing the width. They
/// lead where the sidebar's do, and only while the wallet is idle.
fn tab_bar<'a>(model: &'a Model, page: &'a WalletPage) -> Element<'a, Message> {
    let idle = model.busy.is_none();
    let here = page.page.nav();
    let mut items = row![].width(Length::Fill).height(Length::Fill);
    for (i, (glyph, name, to)) in NAV.into_iter().enumerate() {
        let active = i == here;
        let ink = if active {
            color::ACCENT
        } else if to.is_some() {
            color::TEXT_SECONDARY
        } else {
            color::TEXT_MUTED
        };
        let label_type = if active {
            ty::NAV_TAB_ACTIVE
        } else {
            ty::NAV_TAB
        };
        items = items.push(
            iced::widget::button(
                container(
                    column![
                        icon::icon(glyph, 20.0, 2.0, ink),
                        ui::label(name, label_type),
                    ]
                    .spacing(sp::S4)
                    .align_x(Alignment::Center),
                )
                .center(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::ZERO)
            .style(theme::button(theme::Button::Nav { active }))
            .on_press_maybe(to.filter(|_| idle).map(|to| WalletMsg::Open(to).into())),
        );
    }
    container(items)
        .width(Length::Fill)
        .height(Length::Fixed(56.0))
        .style(theme::sidebar)
        .into()
}

/// A page: its title and subtitle with the page's own controls beside
/// them, what the worker is doing for it while it waits, the refusal its
/// last command met, then the page.
fn frame<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    title: &str,
    subtitle: impl iced::widget::text::IntoFragment<'a>,
    actions: Vec<Element<'a, Message>>,
    body: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    framed(model, page, title, subtitle, actions, body, true)
}

/// [`frame`] for a page whose buttons draw what their own command met, beside
/// them, with [`beside`]: the busy card and the refusal are not drawn at the
/// top of it (docs/DECISIONS.md D40 item 4).
fn frame_beside<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    title: &str,
    subtitle: impl iced::widget::text::IntoFragment<'a>,
    actions: Vec<Element<'a, Message>>,
    body: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    framed(model, page, title, subtitle, actions, body, false)
}

/// [`frame`], or [`frame_beside`] when `top` is false: whether the busy card
/// and the refusal are drawn under the title.
fn framed<'a>(
    model: &'a Model,
    page: &'a WalletPage,
    title: &str,
    subtitle: impl iced::widget::text::IntoFragment<'a>,
    actions: Vec<Element<'a, Message>>,
    body: Vec<Element<'a, Message>>,
    top: bool,
) -> Element<'a, Message> {
    let compact = model.compact();
    // The title takes what the controls leave, its subtitle wrapping, so the
    // controls keep their size at the smallest window. On a phone the title
    // has the width, and the controls wrap beneath it (docs/SCREENS.md,
    // "Compact layout").
    let header: Element<'a, Message> = if compact {
        let mut head = column![ui::page_header(title, subtitle)];
        if !actions.is_empty() {
            head = head.push(
                iced::widget::Row::with_children(actions)
                    .spacing(sp::S8)
                    .wrap()
                    .vertical_spacing(sp::S8),
            );
        }
        head.spacing(sp::S16).into()
    } else {
        let mut header = row![container(ui::page_header(title, subtitle)).width(Length::Fill)]
            .spacing(sp::S10)
            .align_y(Alignment::Center);
        for action in actions {
            header = header.push(action);
        }
        header.into()
    };
    let mut stack = column![header].spacing(if compact { sp::S16 } else { sp::S24 });
    if let Some(busy) = model.busy.as_ref().filter(|_| top) {
        stack = stack.push(working(compact, busy));
    }
    // The offer of the device unlock stands until it is answered or the store
    // locks, and what the device unlock last found stands under it (docs/DECISIONS.md
    // D45 item 4).
    if let (true, Some(device)) = (model.device_offer, &model.device) {
        stack = stack.push(device_offer(device.check()));
    }
    if let Some(note) = &model.device_note {
        stack = stack.push(ui::refusal(note.as_str()));
    }
    for kept in model.unsaved_signed() {
        stack = stack.push(unsaved(model, kept));
    }
    if let Some(e) = page.error.as_deref().filter(|_| top) {
        stack = stack.push(refusal_of(compact, page, e));
    }
    for part in body {
        stack = stack.push(part);
    }
    stack.into()
}

/// The offer of the device unlock, made once after a password unlock opened the
/// store (docs/DECISIONS.md D45 item 4): "Turn on" keeps the password on this
/// phone, and "Not now" keeps nothing. Provisional wording, for the owner's
/// approval (D45 item 6).
fn device_offer<'a>(check: DeviceCheck) -> Element<'a, Message> {
    let name = check.name();
    ui::accent_callout_with_actions(
        Icon::Lock,
        format!("Unlock with {name} next time?"),
        format!(
            "Keep this wallet's password on this phone, to unlock the wallet with {name} \
             instead of typing it. Anyone who knows this phone's passcode can open the \
             wallet then. Turning the unlock off in Settings deletes the password."
        ),
        row![
            ui::button_with(
                "Turn on",
                theme::Button::Primary,
                Size::Medium,
                None,
                Some(Message::DeviceOffer(true)),
            ),
            ui::button_with(
                "Not now",
                theme::Button::Secondary,
                Size::Medium,
                None,
                Some(Message::DeviceOffer(false)),
            ),
        ]
        .spacing(sp::S10),
    )
}

/// What a page draws beside the button whose command it is for: the busy card
/// while the worker answers, then the refusal that command met, each when
/// there is one (docs/DECISIONS.md D40 item 4). Empty when neither holds.
fn beside<'a>(model: &'a Model, page: &'a WalletPage) -> Vec<Element<'a, Message>> {
    let compact = model.compact();
    let mut out = Vec::new();
    if let Some(busy) = &model.busy {
        out.push(working(compact, busy));
    }
    if let Some(e) = page.error.as_deref() {
        out.push(refusal_of(compact, page, e));
    }
    out
}

/// A refusal in the library's words as a report when it is long, or as a
/// refusal card when it reads as a line.
fn refusal_of<'a>(compact: bool, page: &'a WalletPage, e: &'a str) -> Element<'a, Message> {
    match report::refused(e) {
        Some(r) => report::show_in(compact, page, ReportKey::Refused, r),
        None => ui::refusal(e),
    }
}

/// A spend signed this run whose page was left before its bytes were saved
/// (see [`Model::signed`]): a slim banner, with the way back to its page.
fn unsaved<'a>(model: &Model, kept: &'a Signed) -> Element<'a, Message> {
    let title = t(
        format!(
            "A spend signed from {} is not saved",
            name(kept.sent.sent.from)
        ),
        ty::ROW_TITLE,
        color::TEXT_PRIMARY,
    );
    let body = t(
        "Its signed bytes are kept until they are saved, dismissed or the wallet \
         locks. A reorganisation of the chain can revert a settle, and then a saved \
         copy may be the only way to move the balance. Open its page to save or \
         dismiss them.",
        ty::BODY_SMALL,
        color::TEXT_ON_WARNING_SOFT,
    );
    let open = action(
        model,
        "Open it",
        theme::Button::Secondary,
        Size::Small,
        None,
        WalletMsg::ShowSigned(kept.sent.sent.artifact_hex.clone()),
    );
    let content: Element<'a, Message> = if model.compact() {
        column![
            row![
                icon::icon(Icon::Warning, 18.0, 2.0, color::WARNING),
                title.width(Length::Fill),
            ]
            .spacing(sp::S12)
            .align_y(Alignment::Center),
            body,
            row![space().width(Length::Fill), open].align_y(Alignment::Center),
        ]
        .spacing(sp::S12)
        .into()
    } else {
        row![
            icon::icon(Icon::Warning, 18.0, 2.0, color::WARNING),
            column![title, body].spacing(sp::S4).width(Length::Fill),
            open,
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Center)
        .into()
    };
    container(content)
        .padding([sp::S14, sp::S16])
        .width(Length::Fill)
        .style(theme::callout_warning)
        .into()
}

/// The stopped screen's spends signed this run (see [`Model::signed`]):
/// with the worker gone nothing can be unlocked, so each is offered to
/// save or copy here. `None` when there are none.
pub fn kept_after_stop(model: &Model) -> Option<Element<'_, Message>> {
    if model.signed.is_empty() {
        return None;
    }
    let compact = model.compact();
    let mut list = column![
        ui::section_label("Spends signed in this run"),
        t(
            "Until each is saved, its signed bytes are kept, and they go when Tawara closes: \
             save them first.",
            ty::BODY_SMALL,
            color::TEXT_SECONDARY,
        ),
    ]
    .spacing(sp::S12);
    for kept in &model.signed {
        let sent = &kept.sent.sent;
        let what = match (&sent.tx_id, sent.submitted) {
            (Some(id), _) => format!("Transaction id {}", short(id)),
            (None, true) => "Written to the node's socket".to_owned(),
            (None, false) => "Not written to the node's socket".to_owned(),
        };
        let copied = kept.copied;
        let details = column![
            t(name(sent.from), ty::ROW_TITLE, color::TEXT_PRIMARY),
            t(what, ty::MONO_SMALL, color::TEXT_MUTED),
        ]
        .spacing(sp::S2);
        let save = ui::button_with(
            "Save artifact",
            theme::Button::Primary,
            Size::Small,
            Some(Icon::Download),
            Some(WalletMsg::SaveSigned(sent.artifact_hex.clone()).into()),
        );
        let copy = ui::button_with(
            if copied { "Copied" } else { "Copy hex" },
            theme::Button::Secondary,
            Size::Small,
            Some(if copied { Icon::Check } else { Icon::Copy }),
            Some(WalletMsg::Copy(sent.artifact_hex.clone()).into()),
        );
        // On a phone the buttons wrap beneath the details.
        let line: Element<'_, Message> = if compact {
            column![
                details,
                row![save, copy]
                    .spacing(sp::S10)
                    .wrap()
                    .vertical_spacing(sp::S8),
            ]
            .spacing(sp::S8)
            .into()
        } else {
            row![details.width(Length::Fill), save, copy]
                .spacing(sp::S10)
                .align_y(Alignment::Center)
                .into()
        };
        let mut item = column![line].spacing(sp::S8);
        if let Some(note) = saved_note(&kept.sent) {
            item = item.push(note);
        }
        list = list.push(ui::divider()).push(item);
    }
    Some(ui::card_for(compact, list).into())
}

/// What became of Save artifact for a signed spend's page: where the bytes
/// were written, why they were not, or that the share sheet was asked for.
/// The sheet's outcome is not known, so the spend is still not saved, but
/// nothing was refused, so it is a note in the page's own colours rather than
/// a refusal (docs/DECISIONS.md D32 item 10 (i)).
fn saved_note<'a>(page: &'a SentPage) -> Option<Element<'a, Message>> {
    match &page.saved {
        Some(Ok(path)) => Some(
            t(
                format!("Saved to {}", path.display()),
                ty::MONO_SMALL,
                color::ACCENT,
            )
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            .into(),
        ),
        Some(Err(e)) if page.shared() => Some(t(e, ty::BODY_SMALL, color::TEXT_SECONDARY).into()),
        Some(Err(e)) => Some(ui::refusal(e)),
        None => None,
    }
}

/// What the worker is doing for the page (S8's words), how far it has got,
/// and its Cancel. Until it answers, the page's controls wait.
fn working(compact: bool, busy: &Busy) -> Element<'_, Message> {
    let (title, what) = super::activity(busy);
    let mut text = column![
        t(title, ty::CARD_TITLE, color::TEXT_PRIMARY),
        t(what, ty::BODY_SMALL, color::TEXT_SECONDARY),
    ]
    .spacing(sp::S6);
    if let Some(detail) = super::progress(busy) {
        text = text.push(detail);
    }
    let cancel = ui::button_with(
        "Cancel",
        theme::Button::Secondary,
        Size::Medium,
        None,
        Some(Message::Cancel),
    );
    // On a phone Cancel sits beneath the progress, not beside it.
    let inside: Element<'_, Message> = if compact {
        column![text.width(Length::Fill), cancel]
            .spacing(sp::S16)
            .into()
    } else {
        row![text.width(Length::Fill), cancel]
            .spacing(sp::S24)
            .align_y(Alignment::Center)
            .into()
    };
    ui::card_for(compact, inside).into()
}

/// Two cards side by side in a wide window, `left` parts of the width to
/// `right`; stacked below [`crate::app::WIDE`] (docs/DECISIONS.md D27, item
/// 9).
fn pair<'a>(
    model: &Model,
    first: Element<'a, Message>,
    second: Element<'a, Message>,
    left: u16,
    right: u16,
) -> Element<'a, Message> {
    if model.width >= crate::app::WIDE {
        row![
            container(first).width(Length::FillPortion(left)),
            container(second).width(Length::FillPortion(right)),
        ]
        .spacing(sp::S24)
        .into()
    } else {
        column![first, second]
            .spacing(if model.compact() { sp::S16 } else { sp::S24 })
            .into()
    }
}

/// A button the page enables only while it waits on nothing.
fn action<'a>(
    model: &Model,
    content: &'a str,
    variant: theme::Button,
    size: Size,
    lead: Option<Icon>,
    on_press: impl Into<Message>,
) -> Element<'a, Message> {
    ui::button_with(
        content,
        variant,
        size,
        lead,
        model.busy.is_none().then(|| on_press.into()),
    )
    .into()
}

/// A button that puts `text` on the clipboard: `label`, or "Copied" once
/// `text` is what the page last copied.
fn copy<'a>(page: &WalletPage, text: &str, label: &'a str) -> Element<'a, Message> {
    let copied = page.copied.as_deref() == Some(text);
    ui::button_with(
        if copied { "Copied" } else { label },
        theme::Button::Secondary,
        Size::Small,
        Some(if copied { Icon::Check } else { Icon::Copy }),
        Some(WalletMsg::Copy(text.to_owned()).into()),
    )
    .into()
}

/// The account's destination, or its tag in hex when the library cannot
/// render one (it documents that as unreachable).
fn destination(id: AccountId) -> String {
    id.destination().unwrap_or_else(|| id.hex())
}

/// An account's name: its destination's first and last six characters
/// (docs/DECISIONS.md D27, item 3).
fn name(id: AccountId) -> String {
    short(&destination(id))
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

/// "derived · account 3", "derived" while its number is not known, or
/// "imported". The number is the command line's name for a derived account
/// (`restore --account N`).
fn kind(row: &AccountRow) -> String {
    match (row.kind, row.number) {
        (AccountKind::Derived, Some(n)) => format!("derived · account {n}"),
        (AccountKind::Derived, None) => "derived".to_owned(),
        (AccountKind::Imported, _) => "imported".to_owned(),
    }
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

/// The account in the open store with this id.
fn row_of(model: &Model, id: AccountId) -> Option<&AccountRow> {
    model.wallet.as_ref()?.accounts.iter().find(|a| a.id == id)
}

/// A choice of account (03 "From account"): one row each, with its name,
/// destination, balance and next key, `selected` marked. While the page
/// waits on the worker, the choice stands.
fn account_choice<'a>(
    model: &'a Model,
    ids: impl Iterator<Item = AccountId>,
    selected: Option<AccountId>,
    on_pick: impl Fn(AccountId) -> Message,
) -> Element<'a, Message> {
    let unit = model.prefs.unit;
    let idle = model.busy.is_none();
    let mut list = column![].spacing(sp::S8);
    for id in ids {
        let Some(account) = row_of(model, id) else {
            continue;
        };
        let chosen = selected == Some(id);
        let balance = account.state.balance().map_or_else(
            || "balance not known".to_owned(),
            |b| format!("{} {}", ui::amount(b, unit), ui::unit_name(unit)),
        );
        let card = if model.compact() {
            // The phone: the name, the destination, then the balance and the
            // next key on one line.
            container(
                column![
                    t(name(id), ty::RADIO_TITLE, color::TEXT_PRIMARY),
                    t(destination(id), ty::MONO_TINY, color::TEXT_MUTED)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    row![
                        t(balance, ty::TABLE_AMOUNT, color::TEXT_PRIMARY),
                        space().width(Length::Fill),
                        t(
                            format!("next key #{}", account.index),
                            ty::NOTE,
                            color::TEXT_MUTED
                        ),
                    ]
                    .spacing(sp::S12)
                    .align_y(Alignment::Center),
                ]
                .spacing(sp::S4),
            )
            .padding(iced::Padding::from([sp::S12, sp::S16]))
            .width(Length::Fill)
            .style(theme::radio_card(chosen))
        } else {
            container(
                row![
                    column![
                        t(name(id), ty::RADIO_TITLE, color::TEXT_PRIMARY),
                        t(destination(id), ty::MONO_TINY, color::TEXT_MUTED)
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    ]
                    .spacing(sp::S2)
                    .width(Length::Fill),
                    column![
                        t(balance, ty::TABLE_AMOUNT, color::TEXT_PRIMARY),
                        t(
                            format!("next key #{}", account.index),
                            ty::NOTE,
                            color::TEXT_MUTED
                        ),
                    ]
                    .spacing(sp::S2)
                    .align_x(Alignment::End),
                ]
                .spacing(sp::S12)
                .align_y(Alignment::Center),
            )
            .padding(iced::Padding::from([sp::S12, sp::S16]))
            .width(Length::Fill)
            .style(theme::radio_card(chosen))
        };
        // A row acts on release, so a drag through the list scrolls it
        // (docs/DECISIONS.md D32 item 10 (d)).
        list = list.push(tap(card).on_tap_maybe((idle && !chosen).then(|| on_pick(id))));
    }
    list.into()
}

/// A labelled figure on one line, as the renderings' summary cards set
/// them.
fn figure<'a>(
    name: impl iced::widget::text::IntoFragment<'a>,
    value: String,
) -> Element<'a, Message> {
    row![
        t(name, ty::BODY_SMALL, color::TEXT_SECONDARY).width(Length::Fill),
        t(value, ty::TABLE_AMOUNT, color::TEXT_PRIMARY),
    ]
    .spacing(sp::S12)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{NARROWEST, Screen};
    use crate::harness::Driver;
    use tawara_wallet_core::sample;

    #[test]
    fn destinations_shorten_to_their_ends() {
        assert_eq!(short("9xQmT4vRbN2kWe7HpLsZ3cYdAf"), "9xQmT4…3cYdAf");
        assert_eq!(short("short"), "short");
    }

    #[test]
    fn the_phone_shell_fits_in_360_with_nothing_in_its_page() {
        // The bars and the page's margins at the narrowest width (docs/
        // DECISIONS.md D32 item 10 (b)): nothing is cut off, nothing scrolls
        // sideways. The screens' own content has its own checks.
        use crate::app::{NARROWEST, Page};
        use tawara_wallet_core::preferences::Preferences;
        let mut model = Model::new(Preferences::default(), None);
        model.width = NARROWEST;
        model.safe_area = iced::mobile::SafeArea::new(iced::Padding {
            top: 47.0,
            right: 0.0,
            bottom: 34.0,
            left: 0.0,
        });
        model.wallet = Some(tawara_wallet_core::sample::wallet_view());
        let page = WalletPage {
            page: Page::Dashboard,
            ..WalletPage::default()
        };
        let shell = compact_frame(&model, &page, iced::widget::column![].into());
        assert_eq!(crate::harness::fits(shell, NARROWEST), Ok(()));
    }

    #[test]
    fn an_asked_for_share_sheet_is_a_note_and_a_refused_save_a_refusal() {
        // docs/DECISIONS.md D32 item 10 (i): the sheet's outcome is not known,
        // so the spend is not saved, but nothing was refused. The note is drawn
        // as the page's own text; a save that failed keeps its refusal, here
        // the one Save artifact gives when the platform was not asked for the
        // sheet.
        use crate::app::SHARED;
        use crate::harness::Driver;
        const NOT_SHOWN: &str = "The bytes could not be shared: no window. Copy the hex \
                                 instead and keep it somewhere safe.";
        let page = |saved: &str| SentPage {
            sent: tawara_wallet_core::sample::sent_view(),
            order: 0,
            resigned: false,
            saved: Some(Err(saved.to_owned())),
            dismissing: false,
        };
        let size = (360, 200);
        let draw = |page: &SentPage| {
            let note: Element<'_, Message> = saved_note(page).expect("a note is drawn");
            Driver::new(note, size).draw()
        };
        let plain =
            Driver::<Message>::new(t(SHARED, ty::BODY_SMALL, color::TEXT_SECONDARY), size).draw();
        let refused = Driver::<Message>::new(ui::refusal(NOT_SHOWN), size).draw();
        assert!(
            draw(&page(SHARED)) == plain,
            "a share sheet asked for is a note"
        );
        assert!(
            draw(&page(NOT_SHOWN)) == refused,
            "a failed save is a refusal"
        );
    }

    /// The dashboard of a phone, as the wallet shows it once a password unlock
    /// opened the store: with the device unlock offered by its shell or not, and
    /// with the offer on the page or not (docs/DECISIONS.md D45 item 4).
    fn phone_wallet(device: bool, offer: bool, width: f32) -> Model {
        use tawara_wallet_core::preferences::Preferences;
        let mut model = Model::new(Preferences::default(), None);
        model.width = width;
        model.wallet = Some(sample::wallet_view());
        model.screen = Screen::Wallet(WalletPage {
            page: Page::Dashboard,
            ..WalletPage::default()
        });
        if device {
            model.device = Some(crate::harness::face_id_device());
        }
        model.device_offer = offer;
        model
    }

    /// The texts the wallet page draws on a phone of `width` by 844.
    fn wallet_texts(model: &Model, width: f32) -> Vec<String> {
        let page = match &model.screen {
            Screen::Wallet(page) => page,
            _ => panic!("the wallet's page"),
        };
        crate::harness::texts(view(model, page), (width, 844.0))
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    #[test]
    fn the_offer_stands_on_the_wallet_only_while_the_device_unlock_is_offered_and_being_made() {
        // The offer comes once a password unlock opened the store, and stays until
        // it is answered or the store locks (docs/DECISIONS.md D45 item 4).
        let offer = "Unlock with Face ID next time?";
        let on = wallet_texts(&phone_wallet(true, true, NARROWEST), NARROWEST);
        assert!(on.iter().any(|t| t == offer), "{on:?}");
        assert!(on.iter().any(|t| t == "Turn on"));
        assert!(on.iter().any(|t| t == "Not now"));
        let none = wallet_texts(&phone_wallet(true, false, NARROWEST), NARROWEST);
        assert!(!none.iter().any(|t| t == offer), "no offer is being made");
        let unshown = wallet_texts(&phone_wallet(false, true, NARROWEST), NARROWEST);
        assert!(!unshown.iter().any(|t| t == offer), "no shell offers it");
    }

    #[test]
    fn the_offer_fits_a_phone_of_360_by_640_and_390_by_844() {
        for (width, height) in [(NARROWEST, 640.0), (390.0, 844.0)] {
            let model = phone_wallet(true, true, width);
            let page = match &model.screen {
                Screen::Wallet(page) => page,
                _ => unreachable!(),
            };
            assert_eq!(
                crate::harness::fits_at(view(&model, page), width, height),
                Ok(()),
                "the wallet with the offer, at {width} by {height}"
            );
        }
    }

    #[test]
    fn a_tap_on_the_offers_buttons_answers_it_yes_and_no() {
        let model = phone_wallet(true, true, NARROWEST);
        let size = (NARROWEST, 844.0);
        let answer = |text: &str| {
            let page = match &model.screen {
                Screen::Wallet(page) => page,
                _ => unreachable!(),
            };
            let (_, bounds) = crate::harness::texts(view(&model, page), size)
                .into_iter()
                .find(|(drawn, _)| drawn == text)
                .unwrap_or_else(|| panic!("no {text:?}"));
            let page = match &model.screen {
                Screen::Wallet(page) => page,
                _ => unreachable!(),
            };
            let mut driver = Driver::new(view(&model, page), (360, 844));
            let (x, y) = (
                bounds.x + bounds.width / 2.0,
                bounds.y + bounds.height / 2.0,
            );
            let (messages, _, _) = driver.send(&[
                crate::harness::mouse_move(x, y),
                crate::harness::mouse_press(),
                crate::harness::mouse_release(),
            ]);
            messages
        };
        assert!(
            answer("Turn on")
                .iter()
                .any(|m| matches!(m, Message::DeviceOffer(true))),
            "Turn on keeps the password"
        );
        assert!(
            answer("Not now")
                .iter()
                .any(|m| matches!(m, Message::DeviceOffer(false))),
            "Not now keeps nothing"
        );
    }

    #[test]
    fn what_the_device_unlock_last_found_stands_on_the_wallet_page() {
        let mut model = phone_wallet(true, false, NARROWEST);
        model.device_note = Some(
            "The password could not be kept on this device (the keychain refused).".to_owned(),
        );
        let texts = wallet_texts(&model, NARROWEST);
        assert!(
            texts
                .iter()
                .any(|t| t
                    == "The password could not be kept on this device (the keychain refused)."),
            "{texts:?}"
        );
    }
}
