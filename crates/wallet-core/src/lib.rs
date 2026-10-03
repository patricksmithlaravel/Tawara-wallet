//! Everything in Tawara that touches the wallet library, `mochimo-crypto`.
//!
//! This crate has no interface dependency (docs/PLAN.md section 3, D3): it is
//! the part of the application that survives unchanged if a mobile shell is
//! replaced. The `app` crate talks to it and never to the library.
//!
//! What it will own, from phase 2 on:
//!
//! - **One worker thread** that owns the `Keystore` and the `Wallet`. Nothing
//!   calls the library on the interface thread: the transport is
//!   synchronous, `Wallet::open` reconciles over the network, and unlocking
//!   at `Kdf::RECOMMENDED` costs 64 MiB over three passes.
//! - **Commands in, events out.** Events carry view models: plain data that
//!   holds no secret.
//! - **Cancellation and progress** through the library's `recon::Cancel`.
//! - **Store-location policy, entropy, and secret input** (docs/PLAN.md
//!   section 4).
//! - **Lifecycle.** An idle timer, an explicit Lock, and a move to the
//!   background on mobile each drop the `Keystore`, which drops the secret
//!   and releases the store's lock in one move.
//!
//! Phase 0 holds the crate's place in the workspace, and the repository's
//! standing rules as checks in `tests/policy.rs`: the library pinned by full
//! commit hash and never patched, the release profile that keeps the
//! library's scrubs running, the test-only key-derivation cost kept out of
//! every shipped path, and the boundary between this crate and the
//! interface.
