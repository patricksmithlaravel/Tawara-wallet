//! The pieces every screen is built from: text in the renderings' type
//! scale (`design/TOKENS.md` section 2.4), buttons (5.1), fields (5.5),
//! cards, chips and the library's pages (docs/DECISIONS.md D27, item 13).

use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{Button, Text, button, column, container, row, rule, space, text, text_input};
use iced::{Alignment, Color, Element, Font, Length, Padding, Pixels};

use crate::fonts;
use crate::icon::{self, Icon};
use crate::theme::{self, color, radius, space as sp};

/// A role in the type scale: a face and a size. The line height follows
/// from the face, as the renderings' "normal" does (`design/TOKENS.md`
/// 2.3): Poppins 1.5, Montserrat 1.219, IBM Plex Mono 1.3.
#[derive(Clone, Copy, Debug)]
pub struct Type {
    pub font: Font,
    pub size: f32,
}

impl Type {
    const fn new(font: Font, size: f32) -> Type {
        Type { font, size }
    }

    fn line_height(self) -> f32 {
        match self.font.family {
            iced::font::Family::Name("Poppins") => 1.5,
            iced::font::Family::Name("Montserrat") => 1.219,
            _ => 1.3,
        }
    }
}

/// The type scale (`design/TOKENS.md` 2.4), by role.
pub mod ty {
    use super::Type;
    use crate::fonts;

    pub const HERO: Type = Type::new(fonts::DISPLAY_BOLD, 52.0);
    pub const HERO_ITALIC: Type = Type::new(fonts::DISPLAY_BOLD_ITALIC, 52.0);
    pub const ONBOARDING_TITLE: Type = Type::new(fonts::DISPLAY_BOLD, 28.0);
    pub const PAGE_TITLE: Type = Type::new(fonts::DISPLAY_BOLD, 26.0);
    pub const WORDMARK: Type = Type::new(fonts::DISPLAY_BOLD, 22.0);
    pub const WORDMARK_LARGE: Type = Type::new(fonts::DISPLAY_BOLD, 30.0);
    pub const SUBTITLE: Type = Type::new(fonts::BODY, 13.0);
    pub const INTRO: Type = Type::new(fonts::BODY, 14.0);
    pub const CARD_TITLE: Type = Type::new(fonts::DISPLAY_BOLD, 16.0);
    pub const SECTION_LABEL: Type = Type::new(fonts::DISPLAY_SEMIBOLD, 12.0);
    pub const TABLE_HEADER: Type = Type::new(fonts::BODY_MEDIUM, 12.0);
    pub const TABLE_BODY: Type = Type::new(fonts::BODY, 13.0);
    pub const TABLE_NAME: Type = Type::new(fonts::BODY_MEDIUM, 13.0);
    pub const BODY: Type = Type::new(fonts::BODY, 14.0);
    pub const BODY_SMALL: Type = Type::new(fonts::BODY, 13.0);
    pub const ROW_TITLE: Type = Type::new(fonts::BODY_MEDIUM, 14.0);
    pub const NOTE: Type = Type::new(fonts::BODY, 12.0);
    pub const TINY: Type = Type::new(fonts::BODY, 11.0);
    pub const FORM_LABEL: Type = Type::new(fonts::BODY_MEDIUM, 12.0);
    pub const RADIO_TITLE: Type = Type::new(fonts::DISPLAY_BOLD, 15.0);
    pub const RADIO_TEXT: Type = Type::new(fonts::BODY, 13.0);
    pub const MONO: Type = Type::new(fonts::MONO, 13.0);
    pub const MONO_SMALL: Type = Type::new(fonts::MONO, 12.0);
    pub const MONO_TINY: Type = Type::new(fonts::MONO, 11.0);
    pub const MONO_WORD: Type = Type::new(fonts::MONO_MEDIUM, 15.0);
    pub const TABLE_AMOUNT: Type = Type::new(fonts::DISPLAY_SEMIBOLD, 13.0);
    pub const STAT_VALUE: Type = Type::new(fonts::DISPLAY_BOLD, 20.0);
    pub const BALANCE: Type = Type::new(fonts::DISPLAY_BOLD, 56.0);
    pub const BALANCE_DECIMALS: Type = Type::new(fonts::DISPLAY_BOLD, 26.0);
    pub const BALANCE_UNIT: Type = Type::new(fonts::DISPLAY_BOLD, 18.0);
    pub const BUTTON_XL: Type = Type::new(fonts::DISPLAY_BOLD, 15.0);
    pub const BUTTON_HEADER: Type = Type::new(fonts::DISPLAY_BOLD, 14.0);
    pub const BUTTON_SECONDARY: Type = Type::new(fonts::BODY_MEDIUM, 13.0);
    pub const BUTTON_SMALL: Type = Type::new(fonts::BODY_MEDIUM, 12.0);
    pub const LINK: Type = Type::new(fonts::BODY_MEDIUM, 13.0);
    pub const LINK_SMALL: Type = Type::new(fonts::BODY_MEDIUM, 12.0);
    pub const NAV: Type = Type::new(fonts::BODY_MEDIUM, 14.0);
    pub const NAV_ACTIVE: Type = Type::new(fonts::BODY_SEMIBOLD, 14.0);
    pub const CHIP: Type = Type::new(fonts::BODY_MEDIUM, 12.0);
    pub const BADGE: Type = Type::new(fonts::BODY_SEMIBOLD, 11.0);
    pub const PILL: Type = Type::new(fonts::BODY_MEDIUM, 13.0);
    pub const CALLOUT_TITLE: Type = Type::new(fonts::DISPLAY_SEMIBOLD, 14.0);
    pub const CALLOUT_BODY: Type = Type::new(fonts::BODY, 12.0);
    pub const FIELD: Type = Type::new(fonts::BODY, 15.0);
    pub const PAGE: Type = Type::new(fonts::MONO, 12.0);
}

/// `content` in role `ty` and `color`.
pub fn t<'a>(content: impl text::IntoFragment<'a>, ty: Type, color: Color) -> Text<'a> {
    text(content)
        .font(ty.font)
        .size(ty.size)
        .line_height(LineHeight::Relative(ty.line_height()))
        .color(color)
}

/// `content` in role `ty`, in the colour of whatever it sits in (a
/// button's label takes the button's text colour).
pub fn label<'a>(content: impl text::IntoFragment<'a>, ty: Type) -> Text<'a> {
    text(content)
        .font(ty.font)
        .size(ty.size)
        .line_height(LineHeight::Relative(ty.line_height()))
}

/// A section label: Montserrat 12/600, upper case, muted (2.4). The
/// renderings track it out by 0.12 em; iced sets no letter spacing, so it
/// is set solid (docs/DECISIONS.md D27).
pub fn section_label<'a>(content: &str) -> Text<'a> {
    t(content.to_uppercase(), ty::SECTION_LABEL, color::TEXT_MUTED)
}

/// A page title over its subtitle (5.21).
pub fn page_header<'a, M: 'a>(
    title: &str,
    subtitle: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    column![
        t(title.to_uppercase(), ty::PAGE_TITLE, color::TEXT_PRIMARY),
        t(subtitle, ty::SUBTITLE, color::TEXT_MUTED),
    ]
    .spacing(sp::S2)
    .into()
}

/// The sizes of `design/TOKENS.md` 5.1 that the screens use.
#[derive(Clone, Copy, Debug)]
pub enum Size {
    /// 56 px, full width (01 "Continue").
    Xl,
    /// 44 px (02 header buttons).
    Header,
    /// 40 px.
    Medium,
    /// 36 px.
    Small,
}

impl Size {
    fn height(self) -> f32 {
        match self {
            Size::Xl => 56.0,
            Size::Header => 44.0,
            Size::Medium => 40.0,
            Size::Small => 36.0,
        }
    }
}

/// A button of `variant` and `size`, labelled `text`, with an optional
/// leading icon. `on_press` of `None` draws it disabled.
pub fn button_with<'a, M: Clone + 'a>(
    content: &'a str,
    variant: theme::Button,
    size: Size,
    lead: Option<Icon>,
    on_press: Option<M>,
) -> Button<'a, M> {
    let ink = button_ink(variant, on_press.is_some());
    let face = match (variant, size) {
        (theme::Button::Primary | theme::Button::WarningPrimary, Size::Xl) => ty::BUTTON_XL,
        (theme::Button::Primary | theme::Button::WarningPrimary, _) => ty::BUTTON_HEADER,
        (theme::Button::OutlineStrong, _) => Type::new(fonts::DISPLAY_SEMIBOLD, 14.0),
        (_, Size::Small) if matches!(variant, theme::Button::WarningTonal) => ty::BUTTON_SMALL,
        _ => ty::BUTTON_SECONDARY,
    };
    let mut inner = row![].spacing(sp::S8).align_y(Alignment::Center);
    if let Some(i) = lead {
        inner = inner.push(icon::icon(i, 16.0, 2.0, ink));
    }
    inner = inner.push(label(content, face));
    let padding = match size {
        Size::Xl => Padding::from([0.0, sp::S20]),
        Size::Header => Padding::from([0.0, sp::S20]),
        _ => Padding::from([0.0, sp::S14]),
    };
    let width = if matches!(size, Size::Xl) {
        Length::Fill
    } else {
        Length::Shrink
    };
    button(
        container(inner)
            .center_y(Length::Fill)
            .align_x(if matches!(size, Size::Xl) {
                Alignment::Center
            } else {
                Alignment::Start
            })
            .width(width),
    )
    .height(Length::Fixed(size.height()))
    .width(width)
    .padding(padding)
    .style(theme::button(variant))
    .on_press_maybe(on_press)
}

/// The text colour a button of `variant` draws its icon in, as
/// [`theme::button`] draws its label.
fn button_ink(variant: theme::Button, enabled: bool) -> Color {
    if !enabled {
        return color::TEXT_MUTED;
    }
    match variant {
        theme::Button::Primary | theme::Button::WarningPrimary => color::TEXT_ON_ACCENT,
        theme::Button::WarningTonal => color::WARNING,
        theme::Button::WarningOutline => color::TEXT_ON_WARNING_SOFT,
        theme::Button::Ghost => color::TEXT_MUTED,
        theme::Button::Link | theme::Button::Dashed => color::ACCENT,
        theme::Button::Nav { active } => {
            if active {
                color::ACCENT
            } else {
                color::TEXT_SECONDARY
            }
        }
        theme::Button::Segment { selected } => {
            if selected {
                color::TEXT_ON_ACCENT
            } else {
                color::TEXT_SECONDARY
            }
        }
        theme::Button::OutlineStrong
        | theme::Button::Secondary
        | theme::Button::Tonal
        | theme::Button::Bare => color::TEXT_PRIMARY,
    }
}

/// A text link (5.2).
pub fn link<'a, M: Clone + 'a>(content: &'a str, ty: Type, on_press: M) -> Button<'a, M> {
    button(label(content, ty))
        .padding(0)
        .style(theme::button(theme::Button::Link))
        .on_press(on_press)
}

/// A labelled text field (5.5): the label over the field, 6 px apart.
pub fn field<'a, M: Clone + 'a>(
    field_label: impl text::IntoFragment<'a>,
    placeholder: &'a str,
    value: &'a str,
    face: Type,
    secure: bool,
    on_input: impl Fn(String) -> M + 'a,
    on_submit: Option<M>,
) -> Element<'a, M> {
    let mut input = text_input(placeholder, value)
        .font(face.font)
        .size(face.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .secure(secure)
        .style(theme::field(radius::R12))
        .on_input(on_input);
    if let Some(m) = on_submit {
        input = input.on_submit(m);
    }
    column![t(field_label, ty::FORM_LABEL, color::TEXT_SECONDARY), input]
        .spacing(sp::S6)
        .into()
}

/// Library text broken for a terminal, joined again within each
/// paragraph: a single line break becomes a space, a blank line stays. No
/// word is changed.
#[must_use]
pub fn reflow(text: &str) -> String {
    text.trim()
        .split("\n\n")
        .map(|p| p.split('\n').map(str::trim).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Helper text under a field: 12 px, muted (5.5).
pub fn helper<'a>(content: impl text::IntoFragment<'a>) -> Text<'a> {
    t(content, ty::NOTE, color::TEXT_MUTED)
}

/// A refusal under a form, in the library's or the worker's words, whole.
pub fn refusal<'a, M: 'a>(content: &'a str) -> Element<'a, M> {
    container(
        row![
            icon::icon(Icon::Warning, 18.0, 2.0, color::WARNING),
            t(content, ty::CALLOUT_BODY, color::TEXT_ON_WARNING_SOFT)
                .line_height(LineHeight::Relative(1.5))
                .width(Length::Fill),
        ]
        .spacing(sp::S12)
        .align_y(Alignment::Start),
    )
    .padding(sp::S14)
    .width(Length::Fill)
    .style(theme::callout_warning)
    .into()
}

/// An amber callout with a title (5.15).
pub fn warning_callout<'a, M: 'a>(
    title: &'a str,
    body: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    container(
        row![
            container(icon::icon(Icon::Warning, 20.0, 2.0, color::WARNING))
                .center(Length::Fixed(40.0))
                .style(theme::icon_tile(color::WARNING_SOFT, color::WARNING)),
            column![
                t(title, ty::CALLOUT_TITLE, color::TEXT_PRIMARY),
                t(body, ty::CALLOUT_BODY, color::TEXT_ON_WARNING_SOFT)
                    .line_height(LineHeight::Relative(1.5)),
            ]
            .spacing(sp::S4)
            .width(Length::Fill),
        ]
        .spacing(sp::S16)
        .align_y(Alignment::Start),
    )
    .padding(sp::S18)
    .width(Length::Fill)
    .style(theme::callout_warning)
    .into()
}

/// A green callout with an icon and a title (5.15).
pub fn accent_callout<'a, M: 'a>(
    lead: Icon,
    title: impl text::IntoFragment<'a>,
    body: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    container(
        row![
            container(icon::icon(lead, 20.0, 2.2, color::TEXT_ON_ACCENT))
                .center(Length::Fixed(40.0))
                .style(theme::icon_tile(color::ACCENT, color::TEXT_ON_ACCENT)),
            column![
                t(title, ty::CALLOUT_TITLE, color::TEXT_PRIMARY),
                t(body, ty::CALLOUT_BODY, color::TEXT_ON_ACCENT_SOFT)
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
    .style(theme::callout_accent)
    .into()
}

/// The command-line verbs a library page can name, and the control in
/// this application that does the same (docs/DECISIONS.md D25, D27 item
/// 13). Each is found by the verb as the page quotes it, in backticks.
const VERBS: [(&str, &str); 9] = [
    (
        "--advance-to",
        "`reconcile ... --advance-to N` is the account-recovery screen, which shows every \
         account's report first and asks you to type the index.",
    ),
    ("`settle", "`settle` is Settle, on the account's page."),
    ("`resign", "`resign` is Re-sign, on the account's page."),
    ("`submit", "`submit` is Submit saved artifact."),
    ("`restore", "`restore --account N` is Add account."),
    ("`discover", "`discover` is Discover accounts."),
    ("`status", "`status` is Check now, on the account's page."),
    // With its argument: the receive page itself quotes `address` for the
    // ledger line it shows, and that is not the verb.
    ("`address ", "`address` is Receive."),
    ("`create", "`create` is making a new wallet."),
];

/// The notes for the command-line verbs `page` names, each naming the
/// control in Tawara that does the same.
#[must_use]
pub fn verb_notes(page: &str) -> Vec<&'static str> {
    VERBS
        .iter()
        .filter(|(needle, _)| page.contains(needle))
        .map(|(_, note)| *note)
        .collect()
}

/// One of the library's pages, word for word (docs/DECISIONS.md D27, item
/// 13): IBM Plex Mono, its line breaks kept, never cut short; then, where
/// it names a command-line verb, which control does the same.
pub fn library_page<'a, M: 'a>(page: &'a str) -> Element<'a, M> {
    let mut body = column![
        t(page.trim_end(), ty::PAGE, color::TEXT_SECONDARY)
            .wrapping(Wrapping::WordOrGlyph)
            .width(Length::Fill)
    ]
    .spacing(sp::S14);
    let notes = verb_notes(page);
    if !notes.is_empty() {
        let mut list = column![t(
            "This page is the wallet library's, word for word. Where it names a command, \
             the control in Tawara that does the same:",
            ty::NOTE,
            color::TEXT_MUTED,
        )]
        .spacing(sp::S4);
        for note in notes {
            list = list.push(t(note, ty::NOTE, color::TEXT_SECONDARY));
        }
        body = body.push(divider()).push(list);
    }
    container(body)
        .padding(sp::S20)
        .width(Length::Fill)
        .style(theme::card)
        .into()
}

/// A pill (5.4): `content` on `fill` in `ink`, `height` tall.
pub fn pill<'a, M: 'a>(
    content: impl text::IntoFragment<'a>,
    face: Type,
    fill: Color,
    ink: Color,
    height: f32,
) -> Element<'a, M> {
    container(label(content, face))
        .height(Length::Fixed(height))
        .padding(Padding::from([0.0, height / 2.6]))
        .center_y(Length::Fixed(height))
        .style(theme::pill(fill, ink))
        .into()
}

/// A status dot, `size` pixels across (1.7).
pub fn dot<'a, M: 'a>(fill: Color, size: f32) -> Element<'a, M> {
    container(space())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(theme::pill(fill, fill))
        .into()
}

/// A one-pixel divider.
pub fn divider<'a, M: 'a>() -> Element<'a, M> {
    rule::horizontal(1).style(theme::divider).into()
}

/// A card: surface, border, radius 20, padding 24 (3.3).
pub fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> container::Container<'a, M> {
    container(content)
        .padding(sp::S24)
        .width(Length::Fill)
        .style(theme::card)
}

/// The amount `nano` laid out as the renderings lay one out: MCM with
/// thousands grouped and all nine places (`12,480.537214906`), or nanoMCM
/// grouped (`12,480,537,214,906`).
#[must_use]
pub fn amount(nano: impl Into<u128>, unit: tawara_wallet_core::preferences::AmountUnit) -> String {
    use tawara_wallet_core::preferences::AmountUnit;
    let nano: u128 = nano.into();
    match unit {
        AmountUnit::Mcm => {
            let mcm = tawara_wallet_core::amount::format_mcm(nano);
            let (whole, frac) = mcm.split_once('.').unwrap_or((&mcm, ""));
            format!("{}.{frac}", group(whole))
        }
        AmountUnit::NanoMcm => group(&nano.to_string()),
    }
}

/// `digits` with a comma every three from the right.
#[must_use]
pub fn group(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The unit's own name, as the renderings set it beside an amount.
#[must_use]
pub fn unit_name(unit: tawara_wallet_core::preferences::AmountUnit) -> &'static str {
    match unit {
        tawara_wallet_core::preferences::AmountUnit::Mcm => "MCM",
        tawara_wallet_core::preferences::AmountUnit::NanoMcm => "nanoMCM",
    }
}

/// Fixed pixels, for the sizes the renderings state.
#[must_use]
pub fn px(n: f32) -> Length {
    Length::Fixed(n)
}

/// A size in pixels, for iced's text sizes.
#[must_use]
pub fn pixels(n: f32) -> Pixels {
    Pixels(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tawara_wallet_core::preferences::AmountUnit;

    #[test]
    fn verbs_the_library_names_get_their_control() {
        let page = "after reading the report above, `reconcile 0x5d --advance-to 33` takes \
                    the acknowledged path; `settle` it afterwards";
        assert_eq!(verb_notes(page).len(), 2);
        assert!(verb_notes("nothing to say").is_empty());
    }

    #[test]
    fn reflow_joins_lines_and_keeps_paragraphs() {
        assert_eq!(reflow("a b\nc d\n\ne\nf\n"), "a b c d\n\ne f");
    }

    #[test]
    fn amounts_are_grouped_and_exact() {
        assert_eq!(
            amount(12_480_537_214_906_u64, AmountUnit::Mcm),
            "12,480.537214906"
        );
        assert_eq!(amount(500_u64, AmountUnit::Mcm), "0.000000500");
        assert_eq!(
            amount(12_480_537_214_906_u64, AmountUnit::NanoMcm),
            "12,480,537,214,906"
        );
        assert_eq!(amount(0_u64, AmountUnit::NanoMcm), "0");
        // A total past u64 is written exactly, not clamped.
        assert_eq!(
            amount(u128::from(u64::MAX) + 5, AmountUnit::Mcm),
            "18,446,744,073.709551620"
        );
        assert_eq!(group("100"), "100");
        assert_eq!(group("1000"), "1,000");
    }
}
