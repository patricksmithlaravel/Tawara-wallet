//! W12, account recovery: the acknowledged advance, on the owner's terms
//! (docs/DECISIONS.md D19; docs/SCREENS.md W12).
//!
//! Reached on purpose, from a paused account's page or from Settings, and
//! never offered as a button on a refusal. It shows the report for every
//! account in the store before anything else, each opened to its summary,
//! and offers the advance only once every account's full report has been
//! opened, because the evidence that one account's advance is wrong is
//! most often in another's report (D29, item 5). The person types the
//! index the report names (nothing fills it in) and confirms that no other
//! wallet uses this recovery phrase; then the library advances, only to
//! the index its live report names, or refuses. An account the chain was
//! not found for among the keys searched can be searched for further
//! instead.
//!
//! On a phone (docs/SCREENS.md, "Compact layout") each account's head is a
//! column, each index field sits under its label at the full width, and the
//! Advance button sits over its note.

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
    let compact = model.compact();
    let mut body = vec![ui::warning_callout(
        "Nothing here moves a key index until you say so",
        "Read every account's report first. An advance moves an account forward to the key the \
         chain shows, once you have typed that index and confirmed that no other wallet uses \
         this recovery phrase. The library warns that advancing can destroy keys when a second \
         wallet is live on the same phrase.",
    )];
    match &r.reports {
        None => body.push(
            ui::card_for(
                compact,
                ui::helper(if model.busy.is_some() {
                    "Reading every account from the chain…"
                } else {
                    "Not read yet: Read again asks the chain about every account."
                }),
            )
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
                    ui::card_for(
                        compact,
                        ui::helper(
                            "No account's report names an index to advance to, and none was \
                             missed among the keys searched: there is nothing to recover here.",
                        ),
                    )
                    .into(),
                );
            }
        }
    }
    if let Some((ok, text)) = &r.result {
        body.push(report::show_in(
            compact,
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
    let compact = model.compact();
    let (word, ink) = status(&a.state);
    let kind_word = row_of(model, a.account).map_or_else(String::new, kind);
    let name_and_kind = column![
        t(name(a.account), ty::ROW_TITLE, color::TEXT_PRIMARY),
        t(kind_word, ty::TINY, color::TEXT_MUTED),
    ]
    .spacing(sp::S2)
    .width(Length::Fill);
    let state = row![ui::dot(ink, 6.0), t(word, ty::TABLE_BODY, ink)]
        .spacing(sp::S6)
        .align_y(Alignment::Center);
    let read: iced::widget::Row<'a, Message> = if r.read.contains(&a.account) {
        row![
            crate::icon::icon(crate::icon::Icon::Check, 14.0, 2.0, color::ACCENT),
            t("Full report read", ty::NOTE, color::ACCENT),
        ]
        .spacing(sp::S4)
        .align_y(Alignment::Center)
    } else {
        row![t("Full report not opened yet", ty::NOTE, color::TEXT_MUTED)]
    };
    let remedy_action: Option<Element<'a, Message>> = remedy(&a.state).map(|m| {
        if r.target == Some(a.account) {
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
        }
    });
    // On a phone the head is a column: the name, the state and the read
    // marker beneath it (wrapping if they do not fit), then the action.
    let head: Element<'a, Message> = if compact {
        let mut head = column![
            name_and_kind,
            row![state, read]
                .spacing(sp::S12)
                .align_y(Alignment::Center)
                .wrap()
                .vertical_spacing(sp::S6),
        ]
        .spacing(sp::S10);
        if let Some(remedy_action) = remedy_action {
            head = head.push(remedy_action);
        }
        head.into()
    } else {
        let mut head = row![name_and_kind, state]
            .spacing(sp::S12)
            .align_y(Alignment::Center);
        head = head.push(read);
        if let Some(remedy_action) = remedy_action {
            head = head.push(remedy_action);
        }
        head.into()
    };
    let key = ReportKey::Review(u16::try_from(n).unwrap_or(u16::MAX));
    let body =
        column![head, report::show_in(compact, page, key, report::review(a))].spacing(sp::S12);
    if compact {
        ui::card_for(true, body).into()
    } else {
        ui::card(body).padding(sp::S18).into()
    }
}

/// The index field under its label: beside the label on the desktop, and
/// beneath it at the full width on a phone (docs/SCREENS.md, W12).
fn index_field<'a>(
    compact: bool,
    label: &'static str,
    field: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if compact {
        return column![
            t(label, ty::FORM_LABEL, color::TEXT_SECONDARY),
            field.into()
        ]
        .spacing(sp::S6)
        .width(Length::Fill)
        .into();
    }
    row![
        t(label, ty::FORM_LABEL, color::TEXT_SECONDARY),
        field.into()
    ]
    .spacing(sp::S12)
    .align_y(Alignment::Center)
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
    let compact = model.compact();
    let idle = model.busy.is_none();
    let mut field = text_input("", &r.index)
        .font(ty::MONO.font)
        .size(ty::MONO.size)
        .padding(Padding::from([sp::S12, sp::S14]))
        .width(if compact {
            Length::Fill
        } else {
            Length::Fixed(160.0)
        })
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
                index_field(compact, "Advance to key index", field),
                confirm,
                gate(model, r),
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
            index_field(compact, "Search to key index", field),
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
        .padding(if compact { sp::S16 } else { sp::S24 })
        .width(Length::Fill)
        .style(theme::callout_warning)
        .into()
}

/// The Advance button, offered once every account's full report has been
/// opened since the reports were read (docs/DECISIONS.md D19, D29), with
/// how many are still to open until then. On a phone the note sits beneath
/// the button.
fn gate<'a>(model: &'a Model, r: &'a RecoveryPage) -> Element<'a, Message> {
    let unread = r.unread().unwrap_or(usize::MAX);
    let total = r.reports.as_ref().map_or(0, Vec::len);
    let button = ui::button_with(
        "Advance",
        theme::Button::WarningPrimary,
        Size::Medium,
        None,
        (model.busy.is_none() && unread == 0).then(|| WalletMsg::Advance.into()),
    );
    if unread == 0 {
        return button.into();
    }
    let note = t(
        format!(
            "Open every account's full report above first: {} of {total} opened. The \
             evidence that an advance is wrong is most often in another account's report.",
            total.saturating_sub(unread)
        ),
        ty::NOTE,
        color::TEXT_SECONDARY,
    );
    if model.compact() {
        return column![button, note.width(Length::Fill)]
            .spacing(sp::S8)
            .into();
    }
    row![button, note.width(Length::Fill)]
        .spacing(sp::S16)
        .align_y(Alignment::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use tawara_wallet_core::preferences::Preferences;
    use tawara_wallet_core::sample;
    use tawara_wallet_core::{NetworkName, SyncState};

    use super::*;
    use crate::app::{Level, NARROWEST, NodeState, Page, Screen, WalletPage};
    use crate::harness::fits;
    use crate::screens;
    use crate::ui::Zone;

    /// The node and the folder the sample store is kept in, as the screenshot
    /// example names them.
    const NODE: &str = "https://node.example";
    const DIR: &str = "/home/you/.local/share/tawara/keystore";

    /// The sample store on `page`, drawn at the narrowest phone width, built as
    /// the screenshot example builds it.
    fn sample_model(page: Page) -> Model {
        let mut model = Model::new(Preferences::default(), Some(PathBuf::from(DIR)));
        model.node = NodeState {
            url: Some(NODE.to_owned()),
            tip: Some((871_173, Duration::from_millis(42))),
            solved_ms: Some(sample::TIP_MS),
            sync: Some(SyncState {
                stage: "synchronized".to_owned(),
                synced: true,
            }),
            network: Some(NetworkName {
                blockchain: "mochimo".to_owned(),
                network: "mainnet".to_owned(),
            }),
            error: None,
        };
        model.clock_ms = sample::TIP_MS + 48_000;
        model.zone = Zone::Fixed(0);
        model.width = NARROWEST;
        model.wallet = Some(sample::wallet_view());
        model.screen = Screen::Wallet(WalletPage {
            page,
            ..WalletPage::default()
        });
        model
    }

    /// The recovery screen with every account's report, as the screenshot
    /// example builds it: the paused account chosen with its index typed and
    /// the second wallet ruled out, the other two reports at `others` and the
    /// paused one at `paused`, and the first `read` accounts' full reports read.
    fn recovery(others: Level, paused: Level, read: usize) -> Model {
        let reports = sample::review();
        let mut model = sample_model(Page::Recovery(RecoveryPage {
            target: Some(reports[2].account),
            index: "33".to_owned(),
            confirmed: true,
            read: reports.iter().rev().take(read).map(|a| a.account).collect(),
            reports: Some(reports),
            result: None,
        }));
        if let Screen::Wallet(page) = &mut model.screen {
            for n in 0..2u16 {
                page.open.insert(ReportKey::Review(n), others);
            }
            page.open.insert(ReportKey::Review(2), paused);
        }
        model
    }

    #[test]
    fn the_recovery_screens_fit_a_phone_of_the_narrowest_width() {
        let screens_to_draw = [
            ("w12-recovery", recovery(Level::Summary, Level::Summary, 0)),
            (
                "w12-recovery-waiting",
                recovery(Level::Closed, Level::Summary, 1),
            ),
            (
                "w12-recovery-advance",
                recovery(Level::Closed, Level::Summary, 3),
            ),
        ];
        for (screen, model) in screens_to_draw {
            assert_eq!(fits(screens::view(&model), NARROWEST), Ok(()), "{screen}");
        }
    }

    /// What a window draws as text, and where each text lies.
    #[derive(Default)]
    struct Texts(Vec<(String, iced::Rectangle)>);

    impl iced::advanced::widget::Operation for Texts {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation),
        ) {
            operate(self);
        }

        fn text(
            &mut self,
            _: Option<&iced::advanced::widget::Id>,
            bounds: iced::Rectangle,
            text: &str,
        ) {
            self.0.push((text.to_owned(), bounds));
        }
    }

    /// Where the one text that says `text` lies in a phone's window.
    fn lies_on_a_phone(model: &Model, text: &str) -> iced::Rectangle {
        let mut renderer = crate::harness::renderer();
        let mut ui = iced_runtime::user_interface::UserInterface::build(
            screens::view(model),
            iced::Size::new(NARROWEST, 844.0),
            iced_runtime::user_interface::Cache::default(),
            &mut renderer,
        );
        let mut texts = Texts::default();
        ui.operate(&renderer, &mut texts);
        let found: Vec<iced::Rectangle> = texts
            .0
            .iter()
            .filter(|(drawn, _)| drawn == text)
            .map(|(_, at)| *at)
            .collect();
        assert_eq!(found.len(), 1, "`{text}` is drawn {} times", found.len());
        found[0]
    }

    #[test]
    fn on_a_phone_each_head_puts_its_state_beneath_its_name() {
        // On the desktop the state sits beside the name; on a phone it starts
        // at the name's left edge, below the name's text. Its word follows a
        // dot of 6 pixels and a gap of 6, so it starts within 16 pixels of it.
        let model = recovery(Level::Summary, Level::Summary, 0);
        let reports = sample::review();
        let name_at = lies_on_a_phone(&model, &name(reports[0].account));
        let state_at = lies_on_a_phone(&model, "Reconciled");
        assert!(
            state_at.x < name_at.x + 16.0 && state_at.y > name_at.y,
            "the state is not beneath the name: {state_at:?} and {name_at:?}"
        );
    }
}
