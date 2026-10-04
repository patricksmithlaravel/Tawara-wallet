//! Everything in Tawara that touches the wallet library, `mochimo-crypto`.
//!
//! This crate has no interface dependency (docs/PLAN.md section 3, D3): it is
//! the part of the application that survives unchanged if a mobile shell is
//! replaced. The `app` crate talks to it and never to the library.
//!
//! What it owns:
//!
//! - **One worker thread** that owns the `Keystore` and the `Wallet`
//!   ([`worker`]). Nothing calls the library on the interface thread: the
//!   transport is synchronous, `Wallet::open` reconciles over the network,
//!   and unlocking at `Kdf::RECOMMENDED` costs 64 MiB over three passes.
//! - **Commands in, events out** ([`Command`], [`Event`]). Events carry view
//!   models ([`view`]): plain data that holds no secret. The one exception
//!   is the recovery phrase, shown once when a store is created
//!   ([`PhraseForDisplay`]).
//! - **Cancellation** through the library's `recon::Cancel`, where the
//!   library takes one.
//! - **Store location, entropy and secret input** ([`location`],
//!   [`entropy`], [`SecretText`]; docs/PLAN.md section 4).
//! - **Lifecycle.** An idle timer, an explicit Lock, and a move to the
//!   background on mobile each drop the `Keystore`, which drops the secret
//!   and releases the store's lock in one move.
//! - **The library's own words** for every report and refusal that protects
//!   the person using the wallet: the worker builds the command line's
//!   own outcomes and has the library's renderer write them.
//!
//! `tests/policy.rs` holds the repository's standing rules as checks: the
//! library pinned by full commit hash and never patched, the release profile
//! that keeps the library's scrubs running, the test-only key-derivation
//! cost kept out of every shipped path, and the boundary between this crate
//! and the interface.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod amount;
mod command;
pub mod entropy;
mod event;
pub mod location;
pub mod node;
mod secret;
pub mod spend;
mod text;
pub mod view;
pub mod worker;

pub use command::{Command, PlanId, RequestId};
pub use event::{
    Activity, Discovered, Event, LockReason, PlanView, PlannedDestination, ReceiveView, Refusal,
    RefusalKind, Reply, SentView,
};
pub use node::{Connect, HttpsNode, NodeRefused};
pub use secret::{PhraseForDisplay, SecretText};
pub use worker::{Config, DEFAULT_IDLE_LOCK, WorkerHandle, WorkerStopped, save_artifact, spawn};

/// The shortest password a new store takes, in characters: the library's
/// floor (`cli::create::MIN_PASSWORD_LEN`), for the interface to say before
/// anything is typed. The library refuses below it whatever the interface
/// does.
pub const MIN_PASSWORD_LEN: usize = mochimo_crypto::cli::create::MIN_PASSWORD_LEN;

/// The positions (one-based) of the words the confirmation asks for: the
/// library's (`cli::create::CONFIRM_POSITIONS`).
pub const CONFIRM_POSITIONS: [usize; 3] = mochimo_crypto::cli::create::CONFIRM_POSITIONS;

/// What to show before a recovery phrase is typed to make a store from it:
/// the library's warning (`cli::create::SCHEME_WARNING`) that Mochimo
/// wallets do not agree on how a phrase becomes a seed.
pub const SCHEME_WARNING: &str = mochimo_crypto::cli::create::SCHEME_WARNING;

/// How far a discovery sweep goes when the person does not say: the
/// library's default (`cli::args::DISCOVER_DEFAULT_TO`).
pub const DISCOVER_DEFAULT_TO: u32 = mochimo_crypto::cli::args::DISCOVER_DEFAULT_TO;

/// The furthest a discovery sweep goes, one node request per account: the
/// library's ceiling (`cli::args::DISCOVER_MAX_TO`).
pub const DISCOVER_MAX_TO: u32 = mochimo_crypto::cli::args::DISCOVER_MAX_TO;
