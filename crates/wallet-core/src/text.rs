//! The library's own words, for the reports and refusals that protect the
//! person using the wallet (docs/PLAN.md section 3: "Library text that
//! protects the user is reused, not rewritten").
//!
//! The library separates what a command decided (`cli::outcome::Outcome`,
//! plain types) from the page that announces it (`cli::render::render`).
//! The worker makes the decisions through the `Wallet` and `Keystore`
//! methods, builds the same `Outcome` the command line would, and has the
//! library's renderer write the text. So a divergence report, a startup
//! refusal, the three residues of a send, a settlement and a reconcile read
//! here exactly as they read at the command line.
//!
//! The rendered pages name command-line verbs in places (`settle`,
//! `reconcile ... --advance-to N`). How the interface presents them, and any
//! paraphrase, is phase 3's, with the owner's approval.

use mochimo_crypto::Error;
use mochimo_crypto::cli::outcome::{Decided, Outcome};
use mochimo_crypto::cli::render;
use mochimo_crypto::recon::Divergence;

/// The page the command line would print for `outcome`, with `standing`
/// divergences reported in front of it.
pub(crate) fn page(standing: &[Divergence], outcome: Outcome) -> String {
    render::render(&Decided {
        standing: standing.to_vec(),
        outcome,
    })
    .text
}

/// The library's notice that a store is not whole, for these diverged
/// accounts, or `None` for a whole store.
///
/// `render` writes `notice` then the outcome's page, so the notice is the
/// difference between a page rendered with the divergences and the same
/// page rendered without them. That takes the library's words without
/// copying them, and a change to the notice in the library arrives here
/// with the next `rev`. The rule the command line draws under the notice,
/// to part it from the page below, is left off: here the notice stands
/// alone.
pub(crate) fn standing_notice(diverged: &[Divergence]) -> Option<String> {
    if diverged.is_empty() {
        return None;
    }
    let with = page(diverged, Outcome::HandledBeforeTheWallet);
    let without = page(&[], Outcome::HandledBeforeTheWallet);
    let notice = with.strip_suffix(&without)?.trim_end();
    let notice = notice.strip_suffix(PAGE_RULE).unwrap_or(notice).trim_end();
    Some(notice.to_owned())
}

/// The rule the command line prints between a standing notice and the page.
const PAGE_RULE: &str = "---";

/// A refusal in the library's words.
pub(crate) fn refusal(e: Error) -> String {
    page(&[], Outcome::Failed(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_store_has_no_notice() {
        assert_eq!(standing_notice(&[]), None);
    }

    #[test]
    fn the_notice_is_the_librarys_and_names_every_account() {
        let d = Divergence::TagUnresolved {
            tag: [0x11; 20],
            local: mochimo_crypto::account::WotsIndex::ZERO,
        };
        let e = Divergence::ChainUnreachable {
            tag: [0x22; 20],
            cause: Error::Locked,
        };
        let notice = standing_notice(&[d.clone(), e.clone()]).expect("a notice");
        assert!(
            notice.starts_with("THIS STORE IS NOT WHOLE: 2 account(s)"),
            "{notice}"
        );
        assert!(notice.contains(&d.to_string()), "{notice}");
        assert!(notice.contains(&e.to_string()), "{notice}");
        assert!(
            !notice.contains("handled before the wallet"),
            "the probe page is cut off: {notice}"
        );
        assert!(
            !notice.ends_with(PAGE_RULE),
            "the rule under the notice is left off: {notice:?}"
        );
        assert_eq!(notice, notice.trim_end());
    }

    #[test]
    fn a_refusal_is_the_errors_display() {
        assert_eq!(refusal(Error::Locked), Error::Locked.to_string());
    }
}
