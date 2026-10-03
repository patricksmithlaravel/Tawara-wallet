//! The Tawara application: iced state, messages, `update`, `view`,
//! subscriptions, screens and theme.
//!
//! It never calls the wallet library. Every library call goes through
//! `tawara-wallet-core`'s worker thread, and what comes back is a view model
//! that holds no secret (docs/PLAN.md section 3). The screens follow the
//! owner's renderings in `design/renderings/` (docs/PLAN.md section 5), and
//! where a rendering conflicts with the threat model in docs/PLAN.md section
//! 4, section 4 wins and the owner is asked.
//!
//! Phase 0 holds the crate's place in the workspace and puts iced, at the
//! pinned version and features, into every build and every licence check.
//! The interface itself arrives in phase 3.

/// The application's display name, as the owner gave it.
pub const DISPLAY_NAME: &str = "Tawara";
