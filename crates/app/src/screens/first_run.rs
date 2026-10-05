//! First run and unlocking (docs/SCREENS.md S1 to S8): the layout of
//! rendering 01, the brand panel on the left and the form on the right.

use iced::widget::text::LineHeight;
use iced::widget::{button, checkbox, column, container, progress_bar, row, scrollable, space};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::{Activity, MIN_PASSWORD_LEN, SCHEME_WARNING};

use crate::app::{
    Back, Busy, Go, Message, Model, NodeForm, PasswordForm, PhraseState, RestoreForm, Screen,
    StartChoice, Typed, UnlockForm,
};
use crate::fonts;
use crate::theme::{self, color, space as sp};
use crate::ui::{self, Size, Type, t, ty};

/// The first-run and unlock screens.
pub fn view(model: &Model) -> Element<'_, Message> {
    let form: Element<'_, Message> = match (&model.busy, &model.screen) {
        (Some(busy), _) => opening(busy),
        (None, Screen::Start { choice }) => start(model, *choice),
        (None, Screen::Node(f)) => node(model, f),
        (None, Screen::NewWallet(f)) => new_wallet(model, f),
        (None, Screen::Phrase(p)) => phrase(p),
        (None, Screen::Confirm(p)) => confirm(p),
        (None, Screen::Restore(r)) => restore(r),
        (None, Screen::Unlock(u)) => unlock(model, u),
        (None, Screen::Wallet(_)) => column![].into(),
    };
    row![
        brand(),
        container(
            scrollable(
                container(container(form).max_width(480.0))
                    .padding(sp::S56)
                    .center_x(Length::Fill),
            )
            .height(Length::Shrink)
            .style(theme::scroll),
        )
        .center(Length::Fill)
        .width(Length::FillPortion(1)),
    ]
    .height(Length::Fill)
    .into()
}

/// The brand panel (`design/TOKENS.md` section 7): the green field, the
/// application's own wordmark, the headline and the feature pills. No coin
/// mark and no pattern (docs/DECISIONS.md D27, item 2).
fn brand<'a>() -> Element<'a, Message> {
    let ink = color::TEXT_ON_ACCENT;
    let wordmark = column![
        t("TAWARA", ty::WORDMARK_LARGE, ink),
        t(
            "A wallet for Mochimo",
            Type {
                font: fonts::BODY_MEDIUM,
                size: 13.0
            },
            ink
        ),
    ]
    .spacing(sp::S2);
    let headline = column![
        t("YOUR CRYPTO JOURNEY", ty::HERO, ink)
            .line_height(LineHeight::Relative(1.05))
            .width(Length::Fill),
        t("STARTS WITH US.", ty::HERO_ITALIC, ink).line_height(LineHeight::Relative(1.05)),
    ];
    let pill = |label| ui::pill(label, ty::PILL, color::BG_APP, color::TEXT_PRIMARY, 36.0);
    let pills = row![
        pill("WOTS+ post-quantum signatures"),
        pill("Perpetual account tags"),
        pill("256 destinations per send"),
    ]
    .spacing(sp::S10)
    .wrap()
    .vertical_spacing(sp::S10);
    container(
        column![
            wordmark,
            space().height(Length::Fill),
            column![headline, pills].spacing(sp::S24),
        ]
        .height(Length::Fill),
    )
    .padding(Padding::from([sp::S48, sp::S56]))
    .width(Length::FillPortion(1))
    .height(Length::Fill)
    .style(theme::brand_panel)
    .into()
}

/// A title over an introduction.
fn heading<'a>(title: &str, intro: &'a str) -> Element<'a, Message> {
    column![
        t(
            title.to_uppercase(),
            ty::ONBOARDING_TITLE,
            color::TEXT_PRIMARY
        ),
        t(intro, ty::INTRO, color::TEXT_SECONDARY).line_height(LineHeight::Relative(1.55)),
    ]
    .spacing(sp::S8)
    .into()
}

/// The node footer (5.23): which node the wallet asks, and a way to change
/// it.
fn node_footer(model: &Model, back: Back) -> Element<'_, Message> {
    let (dot, label, url) = match (&model.node.url, &model.node.error) {
        (Some(url), None) => (color::ACCENT, "Mesh API node", url.as_str()),
        (Some(url), Some(_)) => (color::WARNING, "Mesh API node, not answering", url.as_str()),
        (None, _) => (color::TEXT_MUTED, "No node chosen", ""),
    };
    let change = if model.node.url.is_some() {
        "Change node"
    } else {
        "Choose a node"
    };
    column![
        ui::divider(),
        row![
            ui::dot(dot, 8.0),
            t(label, ty::NOTE, color::TEXT_MUTED),
            t(url, ty::MONO_TINY, color::TEXT_SECONDARY),
            space().width(Length::Fill),
            ui::link(change, ty::LINK_SMALL, Message::Go(Go::Node(back))),
        ]
        .spacing(sp::S8)
        .align_y(Alignment::Center),
    ]
    .spacing(sp::S16)
    .into()
}

/// S1: create or restore (01).
fn start(model: &Model, choice: StartChoice) -> Element<'_, Message> {
    column![
        heading(
            "Get started",
            "Your keys are created and encrypted on this computer. Nothing secret ever \
             leaves it.",
        ),
        column![
            radio_card(
                choice == StartChoice::Create,
                "Create a new wallet",
                Some("Recommended"),
                "A fresh 24-word recovery phrase and your first account.",
                Message::Choose(StartChoice::Create),
            ),
            radio_card(
                choice == StartChoice::Restore,
                "Restore from recovery phrase",
                None,
                "12 to 24 words you already have. The store is made from them on this \
                 computer; accounts already on the chain are found from the wallet after.",
                Message::Choose(StartChoice::Restore),
            ),
        ]
        .spacing(sp::S12),
        ui::button_with(
            "Continue",
            theme::Button::Primary,
            Size::Xl,
            None,
            Some(Message::Continue),
        ),
        ui::link(
            "Open a wallet already on this computer",
            ty::LINK,
            Message::Go(Go::Unlock),
        ),
        node_footer(model, Back::Start),
    ]
    .spacing(sp::S24)
    .into()
}

/// A radio card (5.11).
fn radio_card<'a>(
    selected: bool,
    title: &'a str,
    badge: Option<&'a str>,
    description: &'a str,
    on_press: Message,
) -> Element<'a, Message> {
    let mut title_row = row![t(title, ty::RADIO_TITLE, color::TEXT_PRIMARY)]
        .spacing(sp::S8)
        .align_y(Alignment::Center);
    if let Some(b) = badge {
        title_row = title_row.push(ui::pill(
            b,
            ty::BADGE,
            color::ACCENT,
            color::TEXT_ON_ACCENT,
            18.0,
        ));
    }
    button(
        container(
            row![
                radio_mark(selected),
                column![
                    title_row,
                    t(description, ty::RADIO_TEXT, color::TEXT_SECONDARY)
                        .line_height(LineHeight::Relative(1.5)),
                ]
                .spacing(sp::S4)
                .width(Length::Fill),
            ]
            .spacing(sp::S14)
            .align_y(Alignment::Start),
        )
        .padding(sp::S18)
        .width(Length::Fill)
        .style(theme::radio_card(selected)),
    )
    .padding(0)
    .width(Length::Fill)
    .style(theme::button(theme::Button::Bare))
    .on_press(on_press)
    .into()
}

/// The radio's mark (5.11): a white disc, or a green ring round a dark gap
/// and a green dot, as measured.
fn radio_mark<'a>(selected: bool) -> Element<'a, Message> {
    let disc = |size: f32, fill| {
        container(space())
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .style(theme::pill(fill, fill))
    };
    let mark: Element<'a, Message> = if selected {
        container(
            container(disc(10.0, color::ACCENT))
                .center(Length::Fixed(16.0))
                .style(theme::pill(
                    iced::Color::from_rgb8(0x3B, 0x3B, 0x3B),
                    color::ACCENT,
                )),
        )
        .center(Length::Fixed(20.0))
        .style(theme::pill(color::ACCENT, color::ACCENT))
        .into()
    } else {
        disc(20.0, color::TEXT_PRIMARY).into()
    };
    container(mark).padding(Padding::from([sp::S2, 0.0])).into()
}

/// S2: choose the node.
fn node<'a>(model: &'a Model, form: &'a NodeForm) -> Element<'a, Message> {
    let back = match form.back {
        Back::Start => Go::Start,
        Back::Unlock => Go::Unlock,
        Back::Wallet => Go::Wallet,
    };
    let mut links = row![ui::link("Back", ty::LINK, Message::Go(back))].spacing(sp::S24);
    if model.node.url.is_some() {
        links = links.push(ui::link("Forget the node", ty::LINK, Message::ForgetNode));
    }
    let mut body = column![
        heading(
            "Choose a node",
            "There is no default node: you choose whom to trust. Every balance, key index and \
             chain tip the wallet works from comes from this node, so use one you trust. \
             https:// is required; http:// is accepted only to this computer.",
        ),
        ui::field(
            "Node URL",
            "https://",
            &form.url,
            ty::MONO,
            false,
            Message::NodeUrl,
            Some(Message::SaveNode),
        ),
    ]
    .spacing(sp::S24);
    if let Some(e) = &form.error {
        body = body.push(ui::refusal(e));
    } else if let Some(e) = &model.node.error {
        body = body.push(ui::refusal(e));
    }
    let save = (!form.url.trim().is_empty()).then_some(Message::SaveNode);
    body.push(ui::button_with(
        "Use this node",
        theme::Button::Primary,
        Size::Xl,
        None,
        save,
    ))
    .push(links)
    .into()
}

/// The folder and the two passwords, shared by S3 and S6.
fn password_fields(form: &PasswordForm, submit: Option<Message>) -> Element<'_, Message> {
    let mut fields = column![ui::field(
        "Folder",
        "",
        &form.dir,
        ty::MONO,
        false,
        Message::Dir,
        None,
    )]
    .spacing(sp::S16);
    if let Some(w) = &form.synced {
        fields = fields.push(ui::warning_callout("This folder is synced", w.as_str()));
    }
    fields
        .push(
            column![
                ui::field(
                    "Password",
                    "",
                    &form.password,
                    ty::FIELD,
                    true,
                    |s| Message::Password(Typed(s)),
                    None,
                ),
                ui::helper(format!("At least {MIN_PASSWORD_LEN} characters.")),
            ]
            .spacing(sp::S6),
        )
        .push(ui::field(
            "Password again",
            "",
            &form.again,
            ty::FIELD,
            true,
            |s| Message::Again(Typed(s)),
            submit,
        ))
        .into()
}

/// S3: the folder and the password for a new store.
fn new_wallet<'a>(model: &'a Model, form: &'a PasswordForm) -> Element<'a, Message> {
    let ready = !form.password.is_empty() && !form.again.is_empty() && !form.dir.trim().is_empty();
    let mut body = column![
        heading(
            "New wallet",
            "Choose where the store is kept and the password that seals it. Nothing is \
             written until you have confirmed the recovery phrase.",
        ),
        password_fields(form, ready.then_some(Message::CreateWallet)),
    ]
    .spacing(sp::S24);
    if let Some(e) = &form.error {
        body = body.push(ui::refusal(e));
    }
    body.push(ui::button_with(
        "Continue",
        theme::Button::Primary,
        Size::Xl,
        None,
        ready.then_some(Message::CreateWallet),
    ))
    .push(ui::link("Back", ty::LINK, Message::Go(Go::Start)))
    .push(node_footer(model, Back::Start))
    .into()
}

/// What the library says when it shows a new phrase (`cli::create`), whole,
/// except its sentence about the terminal's scrollback, which a window does
/// not have (docs/DECISIONS.md D25).
const PHRASE_TITLE: &str = "WRITE THIS DOWN. It is shown once.";
const PHRASE_BODY: &str = "It is the ONLY backup of this wallet. It cannot be recovered from \
                           the store. The store is sealed with the password you just chose; \
                           forget it and these words are the only way back in.";

/// S4: the phrase, shown once, with no way to copy it (docs/PLAN.md 4.3).
fn phrase(p: &PhraseState) -> Element<'_, Message> {
    let words: Vec<&str> = p.phrase.words().collect();
    let rows = words.len().div_ceil(3);
    let mut grid = column![].spacing(sp::S8);
    for r in 0..rows {
        let mut line = row![].spacing(sp::S8);
        for c in 0..3 {
            let n = c * rows + r;
            let cell: Element<'_, Message> = match words.get(n) {
                Some(w) => container(
                    row![
                        t(format!("{:>2}", n + 1), ty::MONO_SMALL, color::TEXT_MUTED),
                        t(*w, ty::MONO_WORD, color::TEXT_PRIMARY),
                    ]
                    .spacing(sp::S10)
                    .align_y(Alignment::Center),
                )
                .padding(Padding::from([sp::S8, sp::S12]))
                .width(Length::FillPortion(1))
                .style(theme::field_box)
                .into(),
                None => space().width(Length::FillPortion(1)).into(),
            };
            line = line.push(cell);
        }
        grid = grid.push(line);
    }
    column![
        t(
            "YOUR RECOVERY PHRASE",
            ty::ONBOARDING_TITLE,
            color::TEXT_PRIMARY
        ),
        ui::warning_callout(PHRASE_TITLE, PHRASE_BODY),
        grid,
        checkbox(p.written)
            .label("I have written down every word, in order.")
            .font(fonts::BODY)
            .text_size(13)
            .on_toggle(Message::Written),
        ui::button_with(
            "Continue",
            theme::Button::Primary,
            Size::Xl,
            None,
            p.written.then_some(Message::ToConfirm),
        ),
        ui::link("Start over with a new phrase", ty::LINK, Message::StartOver,),
    ]
    .spacing(sp::S20)
    .into()
}

/// S5: three words, to show the phrase was written down.
fn confirm(p: &PhraseState) -> Element<'_, Message> {
    let [a, b, c] = p.positions;
    let ready = p.words.iter().all(|w| !w.trim().is_empty());
    let mut fields = column![].spacing(sp::S16);
    for (i, position) in p.positions.iter().enumerate() {
        let last = i == 2;
        fields = fields.push(ui::field(
            format!("Word {position}"),
            "",
            &p.words[i],
            ty::FIELD,
            true,
            move |s| Message::Word(i, Typed(s)),
            (last && ready).then_some(Message::ConfirmWords),
        ));
    }
    let mut body =
        column![
            column![
            t("CONFIRM YOUR PHRASE", ty::ONBOARDING_TITLE, color::TEXT_PRIMARY),
            t(
                format!(
                    "Type words {a}, {b} and {c} of the phrase, as you wrote them down. Nothing \
                     is written until they match."
                ),
                ty::INTRO,
                color::TEXT_SECONDARY,
            )
            .line_height(LineHeight::Relative(1.55)),
        ]
            .spacing(sp::S8),
            fields,
        ]
        .spacing(sp::S24);
    if let Some(e) = &p.error {
        body = body.push(ui::refusal(e));
    }
    body.push(ui::button_with(
        "Create the wallet",
        theme::Button::Primary,
        Size::Xl,
        None,
        ready.then_some(Message::ConfirmWords),
    ))
    .push(ui::link(
        "Show the phrase again",
        ty::LINK,
        Message::BackToPhrase,
    ))
    .into()
}

/// The library's `SCHEME_WARNING` as a callout: its first line as the
/// title, the rest as the body, every word kept. Its lines are broken for a
/// terminal, so within a paragraph they are joined again.
fn scheme_warning<'a>() -> Element<'a, Message> {
    let (title, rest) = SCHEME_WARNING
        .trim()
        .split_once("\n\n")
        .unwrap_or((SCHEME_WARNING.trim(), ""));
    ui::warning_callout(title, ui::reflow(rest))
}

/// S6: a store from a phrase the person already has.
fn restore(r: &RestoreForm) -> Element<'_, Message> {
    let ready = !r.form.password.is_empty()
        && !r.form.again.is_empty()
        && !r.phrase.trim().is_empty()
        && !r.form.dir.trim().is_empty();
    let mut body = column![
        t(
            "RESTORE FROM A PHRASE",
            ty::ONBOARDING_TITLE,
            color::TEXT_PRIMARY,
        ),
        scheme_warning(),
        password_fields(&r.form, None),
        ui::field(
            "Recovery phrase",
            "",
            &r.phrase,
            ty::FIELD,
            true,
            |s| Message::PhraseText(Typed(s)),
            ready.then_some(Message::RestoreWallet),
        ),
    ]
    .spacing(sp::S20);
    if let Some(e) = &r.form.error {
        body = body.push(ui::refusal(e));
    }
    body.push(ui::button_with(
        "Restore",
        theme::Button::Primary,
        Size::Xl,
        None,
        ready.then_some(Message::RestoreWallet),
    ))
    .push(ui::link("Back", ty::LINK, Message::Go(Go::Start)))
    .into()
}

/// S7: unlock the store on this computer.
fn unlock<'a>(model: &'a Model, form: &'a UnlockForm) -> Element<'a, Message> {
    let ready = !form.password.is_empty() && !form.dir.trim().is_empty();
    let mut body = column![heading(
        "Unlock",
        "Open the store on this computer with its password. It stays open until you lock \
         it, or until nothing has been done for the auto-lock period.",
    )]
    .spacing(sp::S24);
    if let Some(note) = &form.note {
        body = body.push(ui::accent_callout(
            crate::icon::Icon::Lock,
            "The wallet locked",
            note.as_str(),
        ));
    }
    body = body
        .push(ui::field(
            "Folder",
            "",
            &form.dir,
            ty::MONO,
            false,
            Message::Dir,
            None,
        ))
        .push(ui::field(
            "Password",
            "",
            &form.password,
            ty::FIELD,
            true,
            |s| Message::Password(Typed(s)),
            ready.then_some(Message::UnlockWallet),
        ));
    if let Some(e) = &form.error {
        body = body.push(ui::refusal(e));
    }
    body.push(ui::button_with(
        "Unlock",
        theme::Button::Primary,
        Size::Xl,
        None,
        ready.then_some(Message::UnlockWallet),
    ))
    .push(ui::link(
        "Create or restore a different wallet",
        ty::LINK,
        Message::Go(Go::Start),
    ))
    .push(node_footer(model, Back::Unlock))
    .into()
}

/// S8: what the worker is doing while a store is made or opened, and a way
/// to stop it.
fn opening(busy: &Busy) -> Element<'_, Message> {
    let (title, what) = match busy.activity {
        Activity::DerivingKey => (
            "Opening the store",
            "Deriving the store's key from the password. It takes a few seconds and 64 MiB \
             of memory on purpose: that is what makes guessing a password expensive.",
        ),
        Activity::AskingNode => (
            "Reconciling",
            "Asking the node about each account and comparing the key index this store holds \
             with the one the chain shows.",
        ),
    };
    let mut body = column![heading(title, what)].spacing(sp::S20);
    if let Some(p) = busy.progress {
        let mut detail = column![t(
            format!("Account {} of {}", p.account + 1, p.accounts),
            ty::ROW_TITLE,
            color::TEXT_PRIMARY,
        )]
        .spacing(sp::S8);
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
        body = body.push(detail);
    }
    body.push(ui::button_with(
        "Cancel",
        theme::Button::Secondary,
        Size::Medium,
        None,
        Some(Message::Cancel),
    ))
    .into()
}
