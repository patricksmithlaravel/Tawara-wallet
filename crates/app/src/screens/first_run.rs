//! First run and unlocking (docs/SCREENS.md S1 to S8): the layout of
//! rendering 01, the brand panel on the left and the form on the right. On a
//! phone the brand panel is not drawn: the form is one page under the wordmark
//! (docs/DECISIONS.md D32 item 10 (b)).

use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{checkbox, column, container, row, scrollable, space, text_input};
use iced::{Alignment, Element, Length, Padding};
use tawara_wallet_core::{MIN_PASSWORD_LEN, SCHEME_WARNING};

use crate::app::{
    Back, Busy, Go, Message, Model, NodeForm, PasswordForm, PasteInto, PhraseState, RestoreForm,
    Screen, StartChoice, Typed, UnlockForm,
};
use crate::fonts;
use crate::paste;
use crate::tap::tap;
use crate::theme::{self, color, radius, space as sp};
use crate::ui::{self, Size, Type, t, ty};

/// The first-run and unlock screens.
pub fn view(model: &Model) -> Element<'_, Message> {
    let form: Element<'_, Message> = match (&model.busy, &model.screen) {
        (Some(busy), _) => opening(busy),
        (None, Screen::Start { choice }) => start(model, *choice),
        (None, Screen::Node(f)) => node(model, f),
        (None, Screen::NewWallet(f)) => new_wallet(model, f),
        (None, Screen::Phrase(p)) => phrase(model.compact(), p),
        (None, Screen::Confirm(p)) => confirm(p, model.drawn()),
        (None, Screen::Restore(r)) => restore(model, r, model.drawn()),
        (None, Screen::Unlock(u)) => unlock(model, u),
        (None, Screen::Wallet(_)) => column![].into(),
    };
    if model.compact() {
        return phone_frame(form);
    }
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

/// The phone's frame (docs/DECISIONS.md D32 item 10 (b)): the wordmark stands
/// over "A wallet for Mochimo" where the brand panel was, and the form follows
/// it on one page. The page scrolls as a whole, between 16-pixel margins,
/// because a phone has no room for a second column.
fn phone_frame(form: Element<'_, Message>) -> Element<'_, Message> {
    let strip = column![
        t("TAWARA", ty::WORDMARK, color::TEXT_PRIMARY),
        t(
            "A wallet for Mochimo",
            Type {
                font: fonts::BODY_MEDIUM,
                size: 13.0
            },
            color::TEXT_SECONDARY
        ),
    ]
    .spacing(sp::S4);
    // The strip and the form share one column, so that they align at every
    // width below the compact limit. The 24 pixels between them are the form
    // block's top padding.
    let page = column![
        phone_block(strip, sp::S16, 0.0),
        phone_block(form, sp::S24, sp::S32),
    ];
    // The page takes the window's size in both directions, since a scrollable
    // is otherwise only as large as its content.
    container(
        scrollable(page)
            .id(crate::screens::page_id())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::scroll),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// A block of a phone's page: its content at most 480 wide and centred, with
/// 16 pixels of margin at each side and `top` and `bottom` above and below.
fn phone_block<'a>(
    content: impl Into<Element<'a, Message>>,
    top: f32,
    bottom: f32,
) -> Element<'a, Message> {
    container(container(content).max_width(480.0).width(Length::Fill))
        .padding(Padding {
            top,
            right: sp::S16,
            bottom,
            left: sp::S16,
        })
        .width(Length::Fill)
        .center_x(Length::Fill)
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

/// Where the person's keys are, as the first-run screens say it: "computer",
/// or "phone" in a phone shell (docs/DECISIONS.md D32 item 10 (a)).
fn place(model: &Model) -> &'static str {
    if model.phone() { "phone" } else { "computer" }
}

/// A title over an introduction.
fn heading<'a>(
    title: &str,
    intro: impl iced::widget::text::IntoFragment<'a>,
) -> Element<'a, Message> {
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
/// it. On a phone the URL takes a line of its own, broken at its glyphs when
/// it is long, so that a long address cannot push the link off the edge
/// (docs/SCREENS.md, "Compact layout").
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
    let link = ui::link(change, ty::LINK_SMALL, Message::Go(Go::Node(back)));
    let line: Element<'_, Message> = if model.compact() {
        let mut lines = column![
            row![
                ui::dot(dot, 8.0),
                t(label, ty::NOTE, color::TEXT_MUTED),
                space().width(Length::Fill),
                link,
            ]
            .spacing(sp::S8)
            .align_y(Alignment::Center),
        ]
        .spacing(sp::S6);
        // An empty URL, when no node is chosen, has no line to draw.
        if !url.is_empty() {
            lines = lines
                .push(t(url, ty::MONO_TINY, color::TEXT_SECONDARY).wrapping(Wrapping::WordOrGlyph));
        }
        lines.into()
    } else {
        row![
            ui::dot(dot, 8.0),
            t(label, ty::NOTE, color::TEXT_MUTED),
            t(url, ty::MONO_TINY, color::TEXT_SECONDARY),
            space().width(Length::Fill),
            link,
        ]
        .spacing(sp::S8)
        .align_y(Alignment::Center)
        .into()
    };
    column![ui::divider(), line].spacing(sp::S16).into()
}

/// S1: create or restore (01).
fn start(model: &Model, choice: StartChoice) -> Element<'_, Message> {
    let compact = model.compact();
    column![
        heading(
            "Get started",
            format!(
                "Your keys are created and encrypted on this {}. Nothing secret ever leaves it.",
                place(model)
            ),
        ),
        column![
            radio_card(
                compact,
                choice == StartChoice::Create,
                "Create a new wallet",
                Some("Recommended"),
                "A fresh 24-word recovery phrase and your first account.",
                Message::Choose(StartChoice::Create),
            ),
            radio_card(
                compact,
                choice == StartChoice::Restore,
                "Restore from recovery phrase",
                None,
                format!(
                    "12 to 24 words you already have. The store is made from them on this {}; \
                     accounts already on the chain are found from the wallet after.",
                    place(model)
                ),
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
            if model.phone() {
                "Open the wallet on this phone"
            } else {
                "Open a wallet already on this computer"
            },
            ty::LINK,
            Message::Go(Go::Unlock),
        ),
        node_footer(model, Back::Start),
    ]
    .spacing(sp::S24)
    .into()
}

/// A radio card (5.11). It acts on release, not on press, so that a drag which
/// starts on it scrolls the page (docs/DECISIONS.md D32 item 10 (d)); a
/// desktop click is a press and a release in place, so nothing changes there.
/// On a phone the card has 16 pixels of padding, and its badge wraps under the
/// title when the two do not fit one line (docs/SCREENS.md, "Compact layout").
fn radio_card<'a>(
    compact: bool,
    selected: bool,
    title: &'a str,
    badge: Option<&'a str>,
    description: impl iced::widget::text::IntoFragment<'a>,
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
    let title_row: Element<'a, Message> = if compact {
        title_row.wrap().vertical_spacing(sp::S4).into()
    } else {
        title_row.into()
    };
    tap(container(
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
    .padding(if compact { sp::S16 } else { sp::S18 })
    .width(Length::Fill)
    .style(theme::radio_card(selected)))
    .on_tap(on_press)
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
    // On a phone the links wrap onto a second line when the width runs out.
    let links: Element<'a, Message> = if model.compact() {
        links.wrap().vertical_spacing(sp::S8).into()
    } else {
        links.into()
    };
    let mut body = column![
        heading(
            "Choose a node",
            format!(
                "There is no default node: you choose whom to trust. Every balance, key index \
                 and chain tip the wallet works from comes from this node, so use one you \
                 trust. https:// is required; http:// is accepted only to this {}.",
                place(model)
            ),
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

/// The folder and the two passwords, shared by S3 and S6. `drawn` is the
/// fields' generation (`Typed`). The folder, and the callout about a synced
/// one, are drawn only when `folder` says the person chooses it: a phone
/// keeps every store in the app's own folder (docs/DECISIONS.md D32 item 10
/// (a)).
fn password_fields(
    form: &PasswordForm,
    drawn: u64,
    submit: Option<Message>,
    folder: bool,
) -> Element<'_, Message> {
    let mut fields = column![].spacing(sp::S16);
    if folder {
        fields = fields.push(ui::field(
            "Folder",
            "",
            &form.dir,
            ty::MONO,
            false,
            Message::Dir,
            None,
        ));
        if let Some(w) = &form.synced {
            fields = fields.push(ui::warning_callout("This folder is synced", w.as_str()));
        }
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
                    move |text| Message::Password(Typed { text, drawn }),
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
            move |text| Message::Again(Typed { text, drawn }),
            submit,
        ))
        .into()
}

/// S3: the folder and the password for a new store.
fn new_wallet<'a>(model: &'a Model, form: &'a PasswordForm) -> Element<'a, Message> {
    let ready = !form.password.is_empty() && !form.again.is_empty() && !form.dir.trim().is_empty();
    let intro = if model.phone() {
        "Choose the password that seals the store. The store is kept in the app's own folder. \
         Nothing is written until you have confirmed the recovery phrase."
    } else {
        "Choose where the store is kept and the password that seals it. Nothing is written \
         until you have confirmed the recovery phrase."
    };
    let mut body = column![heading("New wallet", intro),].spacing(sp::S24);
    if let Some(note) = &form.note {
        body = body.push(ui::accent_callout(
            crate::icon::Icon::Lock,
            "Nothing was created",
            note.as_str(),
        ));
    }
    body = body.push(password_fields(
        form,
        model.drawn(),
        ready.then_some(Message::CreateWallet),
        !model.phone(),
    ));
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

/// S4: the phrase, shown once, with no way to copy it (docs/PLAN.md 4.3). Its
/// words are in three columns, or in two on a phone, whose width has no room
/// for three (docs/SCREENS.md, "Compact layout").
fn phrase(compact: bool, p: &PhraseState) -> Element<'_, Message> {
    let words: Vec<&str> = p.phrase.words().collect();
    let (columns, padding) = if compact {
        (2, Padding::from([sp::S8, sp::S10]))
    } else {
        (3, Padding::from([sp::S8, sp::S12]))
    };
    let mut grid = column![].spacing(sp::S8);
    for cells in phrase_grid(words.len(), columns) {
        let mut line = row![].spacing(sp::S8);
        for cell in cells {
            let word = cell.and_then(|n| words.get(n).map(|w| (n, *w)));
            let cell: Element<'_, Message> = match word {
                Some((n, w)) => container(
                    row![
                        t(format!("{:>2}", n + 1), ty::MONO_SMALL, color::TEXT_MUTED),
                        t(w, ty::MONO_WORD, color::TEXT_PRIMARY),
                    ]
                    .spacing(sp::S10)
                    .align_y(Alignment::Center),
                )
                .padding(padding)
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

/// Where the `len` words of a phrase go in a grid of `columns` columns: one
/// entry per row, holding the zero-based word each cell shows, or `None` for
/// a cell with no word. The phrase is read down the first column and then the
/// next, as the desktop's three columns always were, so on a phone words 1 to
/// 12 fill the first column and words 13 to 24 the second.
fn phrase_grid(len: usize, columns: usize) -> Vec<Vec<Option<usize>>> {
    let rows = len.div_ceil(columns);
    (0..rows)
        .map(|r| {
            (0..columns)
                .map(|c| {
                    let n = c * rows + r;
                    (n < len).then_some(n)
                })
                .collect()
        })
        .collect()
}

/// S5: three words, to show the phrase was written down.
fn confirm(p: &PhraseState, drawn: u64) -> Element<'_, Message> {
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
            move |text| Message::Word(i, Typed { text, drawn }),
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
fn restore<'a>(model: &Model, r: &'a RestoreForm, drawn: u64) -> Element<'a, Message> {
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
        password_fields(&r.form, drawn, None, !model.phone()),
        phrase_field(model, r, drawn, ready),
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

/// S6's recovery-phrase field, drawn as `ui::field` draws a field, with what
/// keyboard paste brings cleaned as a phrase (docs/DECISIONS.md D32 item 10
/// (h)), and in the compact layout a Paste button beside it (item 10 (g)).
fn phrase_field<'a>(
    model: &Model,
    r: &'a RestoreForm,
    drawn: u64,
    ready: bool,
) -> Element<'a, Message> {
    let field = text_input("", &r.phrase)
        .id(super::phrase_id())
        .font(ty::FIELD.font)
        .size(ty::FIELD.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .secure(true)
        .style(theme::field(radius::R12))
        .on_input(move |text| Message::PhraseText(Typed { text, drawn }))
        .on_submit_maybe(ready.then_some(Message::RestoreWallet));
    let entry = if model.compact() {
        row![
            paste::cleaned(field),
            paste::button(Some(Message::Paste(PasteInto::Phrase))),
        ]
        .spacing(sp::S8)
        .align_y(Alignment::Center)
        .into()
    } else {
        paste::cleaned(field)
    };
    column![
        t("Recovery phrase", ty::FORM_LABEL, color::TEXT_SECONDARY),
        entry
    ]
    .spacing(sp::S6)
    .into()
}

/// S7: unlock the store on this computer.
fn unlock<'a>(model: &'a Model, form: &'a UnlockForm) -> Element<'a, Message> {
    let ready = !form.password.is_empty() && !form.dir.trim().is_empty();
    let drawn = model.drawn();
    let mut body = column![heading(
        "Unlock",
        format!(
            "Open the store on this {} with its password. It stays open until you lock it, or \
             until nothing has been done for the auto-lock period.",
            place(model)
        ),
    )]
    .spacing(sp::S24);
    if let Some(note) = &form.note {
        body = body.push(ui::accent_callout(
            crate::icon::Icon::Lock,
            "The wallet locked",
            note.as_str(),
        ));
    }
    if !model.phone() {
        body = body.push(ui::field(
            "Folder",
            "",
            &form.dir,
            ty::MONO,
            false,
            Message::Dir,
            None,
        ));
    }
    body = body.push(ui::field(
        "Password",
        "",
        &form.password,
        ty::FIELD,
        true,
        move |text| Message::Password(Typed { text, drawn }),
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

/// S8: the command a store is being made or opened by, waiting its turn or
/// under way, what the worker is doing for it, and a way to stop it.
fn opening(busy: &Busy) -> Element<'_, Message> {
    let (title, what) = super::activity(busy);
    let mut body = column![heading(title, what)].spacing(sp::S20);
    if let Some(detail) = super::progress(busy) {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iced::advanced::layout;
    use iced::advanced::widget::Tree;
    use iced::{Point, Rectangle, Size};
    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::{Activity, CONFIRM_POSITIONS, PhraseForDisplay, Progress, RequestId};

    use super::*;
    use crate::app::NARROWEST;
    use crate::harness::{
        Driver, finger_lift, finger_move, finger_press, fits, mouse_move, mouse_press,
        mouse_release, texts,
    };

    /// The phone's own folder, as the shells report it (docs/DECISIONS.md D32
    /// item 7). The store is kept in a `keystore` folder inside it.
    const PHONE_DIR: &str = "/private/var/mobile/Containers/Data/Application/0A1B2C3D/Library/Application Support/Tawara";

    /// The node the screens show.
    const NODE: &str = "https://node.example";

    /// A phone of 360 logical pixels drawing `screen`, with its store in the
    /// phone's folder and a node chosen.
    fn phone_model(screen: Screen) -> Model {
        let mut model = Model::new(Preferences::default(), None);
        model.default_dir = Some(PathBuf::from(PHONE_DIR).join("keystore"));
        model.confine(PathBuf::from(PHONE_DIR));
        model.width = NARROWEST;
        model.node.url = Some(NODE.to_owned());
        model.screen = screen;
        model
    }

    /// Every first-run screen, S1 to S8, drawn on a phone, each with a name to
    /// report it by. S1 is also drawn with the restore card chosen, S7 with a
    /// note and with a refusal, and S8 at each of its three stages.
    fn phone_screens() -> Vec<(&'static str, Model)> {
        let folder = PathBuf::from(PHONE_DIR)
            .join("keystore")
            .display()
            .to_string();
        let password = "correct horse battery staple";
        let form = || PasswordForm {
            dir: folder.clone(),
            password: password.to_owned(),
            again: password.to_owned(),
            ..PasswordForm::default()
        };
        let phrase = || PhraseState {
            phrase: PhraseForDisplay::example(),
            dir: folder.clone(),
            positions: CONFIRM_POSITIONS,
            written: true,
            words: Default::default(),
            error: None,
        };
        let busy = |activity, progress| Busy {
            id: RequestId::example(),
            activity,
            progress,
        };
        let mut waiting = phone_model(Screen::Unlock(UnlockForm::default()));
        waiting.busy = Some(busy(None, None));
        let mut opening = phone_model(Screen::Unlock(UnlockForm::default()));
        opening.busy = Some(busy(Some(Activity::DerivingKey), None));
        let mut reconciling = phone_model(Screen::Unlock(UnlockForm::default()));
        reconciling.busy = Some(busy(
            Some(Activity::AskingNode),
            Some(Progress {
                account: 2,
                accounts: 3,
                position: 3_840,
                ceiling: 10_017,
            }),
        ));
        vec![
            (
                "S1",
                phone_model(Screen::Start {
                    choice: StartChoice::Create,
                }),
            ),
            (
                "S1 with restore chosen",
                phone_model(Screen::Start {
                    choice: StartChoice::Restore,
                }),
            ),
            (
                "S2",
                phone_model(Screen::Node(NodeForm {
                    url: NODE.to_owned(),
                    error: None,
                    back: Back::Start,
                })),
            ),
            ("S3", phone_model(Screen::NewWallet(form()))),
            (
                "S3 with a discarded phrase",
                phone_model(Screen::NewWallet(PasswordForm {
                    note: Some(
                        "The recovery phrase was discarded after 5 minutes with nothing done, \
                         before it was confirmed, and no store was written. Choose the password \
                         again and continue to get a new phrase."
                            .to_owned(),
                    ),
                    ..form()
                })),
            ),
            ("S4", phone_model(Screen::Phrase(phrase()))),
            (
                "S5",
                phone_model(Screen::Confirm(PhraseState {
                    words: ["abandon".into(), "abandon".into(), String::new()],
                    ..phrase()
                })),
            ),
            (
                "S6",
                phone_model(Screen::Restore(RestoreForm {
                    form: form(),
                    phrase: "abandon abandon abandon".to_owned(),
                })),
            ),
            (
                "S7",
                phone_model(Screen::Unlock(UnlockForm {
                    dir: folder.clone(),
                    ..UnlockForm::default()
                })),
            ),
            (
                "S7 with a note",
                phone_model(Screen::Unlock(UnlockForm {
                    dir: folder.clone(),
                    note: Some(
                        "Locked after 5 minutes with nothing done. The store is closed and its \
                         key is gone from memory; unlock it to go on."
                            .to_owned(),
                    ),
                    ..UnlockForm::default()
                })),
            ),
            (
                "S7 with a refusal",
                phone_model(Screen::Unlock(UnlockForm {
                    dir: folder.clone(),
                    error: Some("the password does not open the store in this folder".to_owned()),
                    ..UnlockForm::default()
                })),
            ),
            ("S8 waiting", waiting),
            ("S8 opening", opening),
            ("S8 reconciling", reconciling),
        ]
    }

    /// The first-run screen called `name`, drawn on a phone.
    fn named(name: &str) -> Model {
        phone_screens()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, model)| model)
            .unwrap_or_else(|| panic!("no first-run screen called {name}"))
    }

    /// The absolute bounds of every node `root` lays out to in a window of
    /// `width` by `height`, the root first. A node's position is its parent's
    /// plus its own, so the bounds are those drawn.
    fn drawn_bounds(root: Element<'_, Message>, width: f32, height: f32) -> Vec<Rectangle> {
        let mut element = root;
        let renderer = crate::harness::renderer();
        let mut tree = Tree::new(element.as_widget());
        let limits = layout::Limits::new(Size::ZERO, Size::new(width, height));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let mut out = vec![node.bounds()];
        collect(&node, Point::ORIGIN, &mut out);
        out
    }

    /// Appends the bounds of every node under `node`, which sits at `origin`.
    fn collect(node: &layout::Node, origin: Point, out: &mut Vec<Rectangle>) {
        for child in node.children() {
            let bounds = child.bounds();
            let at = Point::new(origin.x + bounds.x, origin.y + bounds.y);
            out.push(Rectangle::new(at, bounds.size()));
            collect(child, at, out);
        }
    }

    /// The top and the bottom of the rows of the window at `x`, between `rows`
    /// pixels from the top, on which a finger that goes down and comes up in
    /// place publishes a message `wanted` accepts: the extent of the control
    /// it taps.
    fn tapped_rows(
        driver: &mut Driver<'_, Message>,
        x: f32,
        rows: std::ops::Range<u32>,
        wanted: impl Fn(&Message) -> bool,
    ) -> Option<(f32, f32)> {
        let mut span: Option<(f32, f32)> = None;
        for y in rows.step_by(2) {
            let y = y as f32;
            let (messages, _, _) = driver.send(&[finger_press(1, x, y), finger_lift(1, x, y)]);
            if messages.iter().any(&wanted) {
                span = Some(span.map_or((y, y), |(top, _)| (top, y)));
            }
        }
        span
    }

    #[test]
    fn every_first_run_screen_fits_a_phone_of_360() {
        let mut past = Vec::new();
        for (name, model) in phone_screens() {
            if let Err(boxes) = fits(view(&model), NARROWEST) {
                past.push(format!("{name}: {boxes:?}"));
            }
        }
        assert!(
            past.is_empty(),
            "nodes past 360 pixels:\n{}",
            past.join("\n")
        );
    }

    #[test]
    fn a_phone_page_is_one_column_across_the_window() {
        // The desktop puts the brand panel beside the form, two nodes at the top
        // level, each half the width. A phone has the page alone, the width of
        // the window.
        for (name, model) in phone_screens() {
            let mut element = view(&model);
            let renderer = crate::harness::renderer();
            let mut tree = Tree::new(element.as_widget());
            let limits = layout::Limits::new(Size::ZERO, Size::new(NARROWEST, 844.0));
            let root = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits);
            let page = root.children();
            assert_eq!(page.len(), 1, "{name}: one page, with no panel beside it");
            assert_eq!(
                page[0].bounds().width,
                NARROWEST,
                "{name}: the page spans the window"
            );
        }
    }

    #[test]
    fn the_phrase_reads_down_two_columns_of_twelve_rows_on_a_phone() {
        let grid = phrase_grid(24, 2);
        assert_eq!(grid.len(), 12, "twelve rows");
        assert!(
            grid.iter().all(|cells| cells.len() == 2),
            "two cells in each row"
        );
        assert_eq!(
            grid[0],
            [Some(0), Some(12)],
            "word 1 heads the first column, word 13 the second"
        );
        assert_eq!(
            grid[11],
            [Some(11), Some(23)],
            "word 12 ends the first column, word 24 the second"
        );
        // The desktop keeps its three columns and their order.
        let desktop = phrase_grid(24, 3);
        assert_eq!(desktop.len(), 8, "eight rows");
        assert_eq!(desktop[0], [Some(0), Some(8), Some(16)]);
    }

    #[test]
    fn the_drawn_phrase_is_twelve_rows_of_two_cells_on_a_phone() {
        // Drawn in a window tall enough for the page, so that no scrollbar takes
        // width from the columns: each of the two is half of the 328 pixels a
        // phone's page leaves, less the gap between them.
        let model = named("S4");
        let cells: Vec<Rectangle> = drawn_bounds(view(&model), NARROWEST, 2000.0)
            .into_iter()
            .filter(|r| (r.width - 160.0).abs() < 0.5 && r.height > 1.0)
            .collect();
        let mut tops: Vec<f32> = cells.iter().map(|r| r.y).collect();
        tops.sort_by(f32::total_cmp);
        tops.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        assert_eq!(cells.len(), 24, "one cell per word");
        assert_eq!(tops.len(), 12, "twelve rows");
        for top in tops {
            let in_row = cells.iter().filter(|r| (r.y - top).abs() < 0.5).count();
            assert_eq!(in_row, 2, "two cells in the row at {top}");
        }
    }

    #[test]
    fn a_tap_on_the_node_footers_link_opens_the_node_screen() {
        // On the iOS simulator, before the phone layout, the footer's link drew
        // its words where its button was not, and taps on them did nothing
        // (docs/DECISIONS.md D32 item 10 (f)). On a phone the link has the room
        // of its own row, so a tap on it reaches it, whether the node answers
        // or not, on each screen that has the footer.
        let node = |m: &Message| matches!(m, Message::Go(Go::Node(_)));
        let screens = || {
            [
                (
                    "S1",
                    Screen::Start {
                        choice: StartChoice::Create,
                    },
                ),
                ("S3", Screen::NewWallet(PasswordForm::default())),
                ("S7", Screen::Unlock(UnlockForm::default())),
            ]
        };
        for not_answering in [false, true] {
            for (name, screen) in screens() {
                let mut model = phone_model(screen);
                if not_answering {
                    model.node.error = Some("the node did not answer".to_owned());
                }
                // A window tall enough for the whole page, so that the footer
                // is on it without a drag.
                let mut driver = Driver::new(view(&model), (360, 1400));
                // The link ends at the right margin, 16 pixels from the edge.
                let span = tapped_rows(&mut driver, 320.0, 0..1400, node);
                let (top, bottom) = span.unwrap_or_else(|| {
                    panic!("{name}, not answering {not_answering}: no tap reached the link")
                });
                assert!(
                    bottom - top >= 10.0,
                    "{name}, not answering {not_answering}: the link is {top}..{bottom}"
                );
            }
        }
    }

    #[test]
    fn a_drag_from_a_radio_card_scrolls_the_page_and_a_tap_chooses() {
        let restore = |m: &Message| matches!(m, Message::Choose(StartChoice::Restore));
        let model = phone_model(Screen::Start {
            choice: StartChoice::Create,
        });
        // A window shorter than the page, so that a drag has something to scroll.
        let mut driver = Driver::new(view(&model), (360, 600));
        let x = 180.0;
        let (top, bottom) =
            tapped_rows(&mut driver, x, 0..600, restore).expect("the restore card is on the page");
        let middle = (top + bottom) / 2.0;

        // The finger goes down on the card and comes up 40 pixels higher.
        let (messages, _, _) = driver.send(&[
            finger_press(2, x, middle),
            finger_move(2, x, middle - 20.0),
            finger_move(2, x, middle - 40.0),
            finger_lift(2, x, middle - 40.0),
        ]);
        assert!(
            !messages.iter().any(|m| matches!(m, Message::Choose(_))),
            "a drag chose a card: {messages:?}"
        );

        // The page scrolled under the finger, so the card is 40 pixels higher.
        let (moved_top, moved_bottom) = tapped_rows(&mut driver, x, 0..600, restore)
            .expect("the restore card, on the scrolled page");
        assert!(
            (top - 40.0 - moved_top).abs() <= 4.0,
            "the page moved the card by {}, not by 40",
            top - moved_top
        );

        // On the scrolled page, a tap chooses the card.
        let middle = (moved_top + moved_bottom) / 2.0;
        let (messages, _, _) =
            driver.send(&[finger_press(3, x, middle), finger_lift(3, x, middle)]);
        assert!(
            messages.iter().any(restore),
            "a tap chose nothing: {messages:?}"
        );
    }

    #[test]
    fn s6_has_a_paste_button_by_the_phrase_on_a_phone_alone() {
        // docs/DECISIONS.md D32 item 10 (g): the button asks for the phrase's
        // paste; the desktop pastes from the keyboard.
        let model = phone_model(Screen::Restore(RestoreForm::default()));
        let pastes: Vec<Rectangle> = texts(view(&model), (NARROWEST, 844.0))
            .into_iter()
            .filter(|(text, _)| text == "Paste")
            .map(|(_, at)| at)
            .collect();
        assert_eq!(pastes.len(), 1, "{pastes:?}");
        let at = pastes[0];
        let mut driver = Driver::new(view(&model), (360, 844));
        let (messages, _, _) = driver.send(&[
            mouse_move(at.center_x(), at.center_y()),
            mouse_press(),
            mouse_release(),
        ]);
        assert!(
            matches!(messages.as_slice(), [Message::Paste(PasteInto::Phrase)]),
            "{messages:?}"
        );

        let mut desktop = Model::new(Preferences::default(), None);
        desktop.screen = Screen::Restore(RestoreForm::default());
        assert!(
            !texts(view(&desktop), (1440.0, 900.0))
                .iter()
                .any(|(text, _)| text == "Paste")
        );
    }
}
