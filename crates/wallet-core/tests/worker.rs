//! The worker, end to end, against a scripted node.
//!
//! Every case drives the worker through its handle, as the interface will,
//! and reads only its events. The store is a real store in a scratch
//! directory and the key derivation is the library's at its recommended
//! cost, so each create or unlock costs about a second in a test build
//! (the workspace optimises the two derivation crates in the dev profile).

mod support;

use std::time::Duration;

use mochimo_crypto::keystore::{self, Keystore, Unlock};
use support::*;
use tawara_wallet_core::spend::{Amount, DestinationInput, SpendInputError, SpendRequest};
use tawara_wallet_core::view::{AccountId, AccountState, ReservationState, WalletView};
use tawara_wallet_core::{
    Activity, CONFIRM_POSITIONS, Command, Config, Event, LockReason, PlanView, Refusal,
    RefusalKind, Reply, SentView,
};

const FUNDS: u64 = 5_000_000;

fn refusal(reply: Reply) -> Refusal {
    match reply {
        Reply::Refused(r) => r,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn opened(reply: Reply) -> WalletView {
    match reply {
        Reply::Created {
            opened: Ok(view), ..
        }
        | Reply::Unlocked(view)
        | Reply::Wallet(view) => view,
        other => panic!("expected an open store, got {other:?}"),
    }
}

fn planned(reply: Reply) -> PlanView {
    match reply {
        Reply::Planned(p) => p,
        other => panic!("expected a plan, got {other:?}"),
    }
}

fn sent(reply: Reply) -> SentView {
    match reply {
        Reply::Sent(s) => s,
        other => panic!("expected a sent spend, got {other:?}"),
    }
}

fn account0() -> AccountId {
    AccountId::from_tag(tag(0))
}

fn pay(to: String, amount: Amount) -> SpendRequest {
    SpendRequest {
        from: account0(),
        destinations: vec![DestinationInput {
            to,
            amount,
            reference: String::new(),
        }],
        fee: None,
        blk_to_live: 0,
    }
}

/// A harness with a node, and the test phrase's store in `scratch`, funded
/// at account 0's first key and opened. Returns the key index it opened at.
fn funded(scratch: &Scratch) -> (Harness, u32) {
    let mut h = Harness::with_node();
    h.chain.hold(tag(0), address(0, 0), FUNDS);
    let view = opened(h.create_from_phrase(&scratch.store()));
    assert!(
        view.opened,
        "the wallet should open on a funded store: {view:?}"
    );
    let row = &view.accounts[0];
    assert_eq!(row.id, account0());
    assert_eq!(row.state, AccountState::InSync { balance: FUNDS });
    assert!(row.spendable);
    (h, row.index)
}

// ------------------------------------------------------------------ create

#[test]
fn a_new_store_is_written_only_once_its_phrase_is_confirmed() {
    let scratch = Scratch::new("create");
    let dir = scratch.store();
    let mut h = Harness::new();

    let (words, positions) = match h.call(Command::CreateBegin {
        dir: dir.clone(),
        password: secret(PASSWORD),
        password_again: secret(PASSWORD),
    }) {
        Reply::CreatePhrase {
            phrase,
            confirm_positions,
        } => (
            phrase.words().map(str::to_owned).collect::<Vec<_>>(),
            confirm_positions,
        ),
        other => panic!("expected a phrase, got {other:?}"),
    };
    assert_eq!(words.len(), 24);
    assert_eq!(positions, CONFIRM_POSITIONS);
    assert_eq!(
        keystore::occupied(&dir),
        None,
        "nothing is written before the confirmation"
    );

    // The wrong words: refused, nothing written, and the phrase still waits.
    let wrong = refusal(h.call(Command::CreateConfirm {
        answer: secret("zoo zoo zoo"),
    }));
    assert_eq!(wrong.kind, RefusalKind::ConfirmationWrong);
    assert!(wrong.text.contains("Nothing was created"), "{}", wrong.text);
    assert_eq!(keystore::occupied(&dir), None);

    let answer = positions
        .iter()
        .map(|&p| words[p - 1].as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let reply = h.call(Command::CreateConfirm {
        answer: secret(&answer),
    });
    let (first, view) = match reply {
        Reply::Created {
            dir: written,
            first,
            opened: Ok(view),
        } => {
            assert_eq!(written, dir);
            (first, view)
        }
        other => panic!("expected a written store, got {other:?}"),
    };
    assert!(keystore::occupied(&dir).is_some(), "the store is written");
    assert_eq!(view.accounts.len(), 1);
    assert_eq!(view.accounts[0].id, first);
    // No node: the store is open and nothing was reconciled.
    assert!(!view.opened);
    assert_eq!(view.accounts[0].state, AccountState::NotReconciled);
    assert!(
        view.notice
            .as_deref()
            .is_some_and(|n| n.contains("No node is chosen")),
        "{:?}",
        view.notice
    );
    assert!(
        h.seen.iter().any(|e| matches!(
            e,
            Event::Busy {
                activity: Activity::DerivingKey,
                ..
            }
        )),
        "the key derivation is announced"
    );

    // The phrase was used up.
    let again = refusal(h.call(Command::CreateConfirm {
        answer: secret(&answer),
    }));
    assert_eq!(again.kind, RefusalKind::NothingToConfirm);
}

#[test]
fn creating_refuses_before_a_phrase_is_shown() {
    let scratch = Scratch::new("create-refusals");
    let dir = scratch.store();
    let mut h = Harness::new();

    let short = refusal(h.call(Command::CreateBegin {
        dir: dir.clone(),
        password: secret("short"),
        password_again: secret("short"),
    }));
    assert_eq!(short.kind, RefusalKind::PasswordTooShort);

    let differ = refusal(h.call(Command::CreateBegin {
        dir: dir.clone(),
        password: secret(PASSWORD),
        password_again: secret("correct horse battery!"),
    }));
    assert_eq!(differ.kind, RefusalKind::PasswordsDiffer);
    assert_eq!(keystore::occupied(&dir), None);

    let _ = opened(h.create_from_phrase(&dir));
    // A store is there now: refused first, before the password is looked at.
    let occupied = refusal(h.call(Command::CreateBegin {
        dir: dir.clone(),
        password: secret("short"),
        password_again: secret("other"),
    }));
    assert_eq!(occupied.kind, RefusalKind::Occupied);
    let occupied = refusal(h.create_from_phrase(&dir));
    assert_eq!(occupied.kind, RefusalKind::Occupied);
}

#[test]
fn abandoning_drops_the_pending_phrase() {
    let scratch = Scratch::new("abandon");
    let mut h = Harness::new();
    assert!(matches!(
        h.call(Command::CreateBegin {
            dir: scratch.store(),
            password: secret(PASSWORD),
            password_again: secret(PASSWORD),
        }),
        Reply::CreatePhrase { .. }
    ));
    assert!(matches!(
        h.call(Command::CreateAbandon),
        Reply::CreateAbandoned
    ));
    let none = refusal(h.call(Command::CreateConfirm {
        answer: secret("a b c"),
    }));
    assert_eq!(none.kind, RefusalKind::NothingToConfirm);
    assert_eq!(keystore::occupied(&scratch.store()), None);
}

// ------------------------------------------------------------------ unlock

#[test]
fn unlocking_refuses_a_wrong_password_a_missing_store_and_a_held_one() {
    let scratch = Scratch::new("unlock-refusals");
    let dir = scratch.store();
    let mut h = Harness::new();
    let _ = opened(h.create_from_phrase(&dir));
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    assert_eq!(h.wait_locked(Duration::from_secs(5)), LockReason::Asked);

    let wrong = refusal(h.unlock(&dir, "not the password at all"));
    assert_eq!(wrong.kind, RefusalKind::WrongPassword);

    let missing = refusal(h.unlock(&scratch.0.join("nothing-here"), PASSWORD));
    assert_eq!(missing.kind, RefusalKind::NoStore);

    // Another process holding the store: its lock is `flock`, per open
    // file, so a second open in this process is refused the same way.
    let held = Keystore::open(
        &dir,
        &Unlock {
            password: PASSWORD.as_bytes(),
            nonce_seed: [7; 32],
        },
    )
    .expect("the test opens the store");
    let in_use = refusal(h.unlock(&dir, PASSWORD));
    assert_eq!(in_use.kind, RefusalKind::StoreInUse);
    drop(held);

    let view = opened(h.unlock(&dir, PASSWORD));
    assert_eq!(view.dir, dir);
}

#[test]
fn without_a_node_or_with_a_silent_one_the_store_stays_open() {
    let scratch = Scratch::new("no-node");
    let dir = scratch.store();
    let mut h = Harness::new();
    let view = opened(h.create_from_phrase(&dir));
    assert!(!view.opened);

    // The store's accounts can be shown, and nothing can be sent.
    assert!(matches!(
        h.call(Command::Receive {
            account: account0()
        }),
        Reply::Receive(_)
    ));
    let not_open = refusal(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(1_000)),
    }));
    assert_eq!(not_open.kind, RefusalKind::WalletNotOpen);

    // A node that does not answer.
    assert!(matches!(
        h.call(Command::SetNode { url: NODE.into() }),
        Reply::NodeSet { .. }
    ));
    h.chain.set_unreachable(true);
    let view = opened(h.call(Command::Refresh));
    assert!(!view.opened);
    assert!(
        view.notice
            .as_deref()
            .is_some_and(|n| n.contains("did not answer")),
        "{:?}",
        view.notice
    );

    // It answers, and the account is paid: the wallet opens.
    h.chain.set_unreachable(false);
    h.chain
        .hold(tag(0), address(0, view.accounts[0].index), FUNDS);
    let view = opened(h.call(Command::Refresh));
    assert!(view.opened, "{view:?}");
    assert_eq!(
        view.accounts[0].state,
        AccountState::InSync { balance: FUNDS }
    );
    assert!(view.accounts[0].spendable);
    assert_eq!(view.notice, None);
}

#[test]
fn a_store_nobody_has_paid_stays_a_store_until_it_is() {
    let scratch = Scratch::new("unpaid");
    let mut h = Harness::with_node();
    let view = opened(h.create_from_phrase(&scratch.store()));
    assert!(
        !view.opened,
        "nothing on the ledger: the wallet cannot open: {view:?}"
    );
    assert_eq!(view.accounts.len(), 1);
    // The library's reading of "account not found", word for word: it
    // cannot tell a new account from an emptied one or a failed lookup.
    assert!(
        matches!(&view.accounts[0].state, AccountState::Diverged { report, advance_to: None }
            if report.contains("account not found")),
        "{:?}",
        view.accounts[0].state
    );
    let notice = view.notice.expect("a store that is not whole says so");
    assert!(notice.starts_with("THIS STORE IS NOT WHOLE"), "{notice}");
    assert!(!notice.ends_with("---"), "{notice:?}");

    h.chain
        .hold(tag(0), address(0, view.accounts[0].index), FUNDS);
    let view = opened(h.call(Command::Refresh));
    assert!(view.opened, "{view:?}");
    assert_eq!(view.total(), u128::from(FUNDS));
}

// ----------------------------------------------------------------- receive

#[test]
fn receiving_shows_the_destination_and_the_ledger_entry() {
    let scratch = Scratch::new("receive");
    let mut h = Harness::new();
    let _ = opened(h.create_from_phrase(&scratch.store()));
    match h.call(Command::Receive {
        account: account0(),
    }) {
        Reply::Receive(r) => {
            assert_eq!(r.destination.as_deref(), Some(destination(0).as_str()));
            assert_eq!(r.ledger_address, hex_of(&address(0, r.index)));
            assert!(!r.text.is_empty());
        }
        other => panic!("expected a destination, got {other:?}"),
    }
}

// ------------------------------------------------------------------- spend

#[test]
fn a_spend_is_planned_signed_submitted_and_settled() {
    let scratch = Scratch::new("spend");
    let (mut h, start) = funded(&scratch);
    let calls = h.chain.calls();

    // A plan that is replaced cannot be confirmed.
    let stale = planned(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(100_000)),
    }));
    let plan = planned(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(100_000)),
    }));
    assert!(h.chain.calls() > calls, "planning reads the ledger");
    assert_ne!(stale.plan, plan.plan);
    assert_eq!(plan.send_total, 100_000);
    assert_eq!(plan.fee_total, 500);
    assert_eq!(plan.balance, FUNDS);
    assert_eq!(plan.change_total, FUNDS - 100_500);
    assert!(!plan.empties_account);
    assert_eq!(plan.destinations.len(), 1);
    assert_eq!(plan.destinations[0].destination, destination(1));
    assert_eq!(plan.destinations[0].amount, 100_000);
    assert!(
        h.chain.submits().is_empty(),
        "nothing is signed by planning"
    );

    let wrong = refusal(h.call(Command::ConfirmSend { plan: stale.plan }));
    assert_eq!(wrong.kind, RefusalKind::NoSuchPlan);
    assert!(h.chain.submits().is_empty());

    let plan = planned(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(100_000)),
    }));
    let s = sent(h.call(Command::ConfirmSend { plan: plan.plan }));
    let submits = h.chain.submits();
    assert_eq!(submits.len(), 1);
    assert!(s.submitted);
    assert!(s.tx_id.is_some());
    assert_eq!(
        s.artifact_hex,
        hex_of(&submits[0]),
        "the artifact is the bytes the node was sent"
    );
    assert!(!s.text.is_empty());
    let row = &s.view.accounts[0];
    assert!(!row.spendable, "a reserved account cannot spend again");
    assert!(
        matches!(
            row.state,
            AccountState::SpendOutstanding {
                spent_index,
                reservation: ReservationState::Live,
                ..
            } if spent_index == start
        ),
        "{:?}",
        row.state
    );

    // Nothing landed yet: settling changes nothing.
    match h.call(Command::Settle {
        account: account0(),
    }) {
        Reply::Settled { view, .. } => {
            assert!(matches!(
                view.accounts[0].state,
                AccountState::SpendOutstanding { .. }
            ));
        }
        other => panic!("expected a settlement report, got {other:?}"),
    }

    // The spend lands: the change is at the next key.
    h.chain
        .hold(tag(0), address(0, start + 1), plan.change_total);
    h.chain.hold(tag(1), address(1, 0), 100_000);
    let view = opened(h.call(Command::Refresh));
    assert!(
        matches!(view.accounts[0].state, AccountState::SpendLanded { spent_index, .. } if spent_index == start),
        "{:?}",
        view.accounts[0].state
    );

    match h.call(Command::Settle {
        account: account0(),
    }) {
        Reply::Settled { view, text } => {
            let row = &view.accounts[0];
            assert_eq!(
                row.state,
                AccountState::InSync {
                    balance: plan.change_total
                },
                "{text}"
            );
            assert_eq!(row.index, start + 1);
            assert!(row.spendable);
        }
        other => panic!("expected a settlement, got {other:?}"),
    }
}

#[test]
fn resigning_reproduces_the_reserved_bytes_and_nothing_else() {
    let scratch = Scratch::new("resign");
    let (mut h, _) = funded(&scratch);
    let request = pay(destination(1), Amount::Nano(250_000));

    // The node refuses the write: the bytes are still handed back.
    h.chain.refuse_submits(true);
    let plan = planned(h.call(Command::PlanSend {
        spend: request.clone(),
    }));
    let first = sent(h.call(Command::ConfirmSend { plan: plan.plan }));
    assert!(!first.submitted);
    assert_eq!(first.tx_id, None);
    assert!(!first.artifact_hex.is_empty());

    h.chain.refuse_submits(false);
    let again = sent(h.call(Command::Resign {
        spend: request.clone(),
    }));
    assert!(again.submitted);
    assert_eq!(
        again.artifact_hex, first.artifact_hex,
        "the same spend, byte for byte"
    );
    let submits = h.chain.submits();
    assert_eq!(submits.len(), 2);
    assert_eq!(submits[0], submits[1]);

    // A different amount is another spend, and is refused.
    let other = refusal(h.call(Command::Resign {
        spend: pay(destination(1), Amount::Nano(250_001)),
    }));
    assert_eq!(other.kind, RefusalKind::Library);
    assert_eq!(h.chain.submits().len(), 2, "nothing more was sent");

    // The saved bytes go out again with no store open.
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    match h.call(Command::SubmitArtifact {
        artifact_hex: format!("{}\n", first.artifact_hex),
    }) {
        Reply::Submitted { accepted, text } => assert!(accepted, "{text}"),
        other => panic!("expected a submission, got {other:?}"),
    }
    assert_eq!(h.chain.submits().len(), 3);
    match h.call(Command::SubmitArtifact {
        artifact_hex: "not hex".into(),
    }) {
        Reply::Submitted { accepted, .. } => assert!(!accepted),
        other => panic!("expected a refused submission, got {other:?}"),
    }
}

#[test]
fn everything_empties_the_account_from_one_ledger_read() {
    let scratch = Scratch::new("everything");
    let (mut h, _) = funded(&scratch);
    let plan = planned(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Everything),
    }));
    assert_eq!(plan.send_total, FUNDS - 500);
    assert_eq!(plan.change_total, 0);
    assert!(plan.empties_account);
    assert_eq!(plan.destinations[0].amount, FUNDS - 500);
}

#[test]
fn spend_input_is_refused_before_the_node_is_asked() {
    let scratch = Scratch::new("spend-input");
    let (mut h, _) = funded(&scratch);
    let calls = h.chain.calls();

    let mut twice = pay(destination(1), Amount::Nano(1_000));
    twice.destinations.push(twice.destinations[0].clone());
    let dup = refusal(h.call(Command::PlanSend { spend: twice }));
    assert_eq!(
        dup.kind,
        RefusalKind::Spend(SpendInputError::Duplicate {
            first: 1,
            second: 2
        })
    );

    let bare = refusal(h.call(Command::PlanSend {
        spend: pay(hex_of(&tag(1)), Amount::Nano(1_000)),
    }));
    assert!(matches!(
        bare.kind,
        RefusalKind::Spend(SpendInputError::BareHex { index: 1, .. })
    ));

    let none = refusal(h.call(Command::PlanSend {
        spend: SpendRequest {
            destinations: Vec::new(),
            ..pay(String::new(), Amount::Nano(0))
        },
    }));
    assert!(matches!(
        none.kind,
        RefusalKind::Spend(SpendInputError::Count { given: 0 })
    ));

    assert_eq!(h.chain.calls(), calls, "no refused input reached the node");
}

// ------------------------------------------------------ divergence, restore

#[test]
fn a_diverged_store_is_reopened_and_advanced_on_acknowledgement() {
    let scratch = Scratch::new("diverged");
    let dir = scratch.store();
    let mut h = Harness::new();
    let view = opened(h.create_from_phrase(&dir));
    let start = view.accounts[0].index;

    // The chain holds the account three keys ahead: another wallet spent
    // from it. Every account diverged, so the library refuses the wallet;
    // the worker reopens the store with the password and says why.
    h.chain.hold(tag(0), address(0, start + 3), FUNDS);
    assert!(matches!(
        h.call(Command::SetNode { url: NODE.into() }),
        Reply::NodeSet { .. }
    ));
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    let view = opened(h.unlock(&dir, PASSWORD));
    assert!(!view.opened, "{view:?}");
    assert!(view.notice.is_some());
    let advance_to = match &view.accounts[0].state {
        AccountState::Diverged {
            advance_to: Some(to),
            ..
        } => *to,
        other => panic!("expected a divergence with an advance, got {other:?}"),
    };
    assert_eq!(advance_to, start + 3);

    let refused = refusal(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(1_000)),
    }));
    assert_eq!(refused.kind, RefusalKind::WalletNotOpen);

    match h.call(Command::Reconcile {
        account: account0(),
        advance_to,
    }) {
        Reply::Reconciled {
            advanced_to,
            text,
            view: Some(view),
        } => {
            assert_eq!(advanced_to, Some(advance_to), "{text}");
            assert!(view.opened, "{view:?}");
            assert_eq!(view.accounts[0].index, advance_to);
            assert_eq!(
                view.accounts[0].state,
                AccountState::InSync { balance: FUNDS }
            );
        }
        other => panic!("expected the advance, got {other:?}"),
    }
}

#[test]
fn restoring_and_discovering_derived_accounts() {
    let scratch = Scratch::new("restore");
    let (mut h, _) = funded(&scratch);
    h.chain.hold(tag(2), address(2, 4), 7_000);

    match h.call(Command::Discover { to: 3 }) {
        Reply::Discovered { accounts, text } => {
            assert_eq!(accounts.len(), 4, "{text}");
            assert_eq!(accounts[0].ledger_balance, Some(FUNDS));
            assert!(accounts[0].held);
            assert_eq!(accounts[1].ledger_balance, None);
            assert_eq!(accounts[2].ledger_balance, Some(7_000));
            assert!(!accounts[2].held);
            assert_eq!(accounts[2].id, AccountId::from_tag(tag(2)));
        }
        other => panic!("expected a sweep, got {other:?}"),
    }

    match h.call(Command::Restore {
        account_index: 2,
        scan_to: None,
    }) {
        Reply::Restored {
            text,
            view: Some(view),
        } => {
            assert!(view.opened, "{text}");
            assert_eq!(view.accounts.len(), 2, "{text}");
            let row = view
                .accounts
                .iter()
                .find(|r| r.id == AccountId::from_tag(tag(2)))
                .expect("the restored account");
            assert_eq!(row.index, 4, "{text}");
            assert_eq!(row.state, AccountState::InSync { balance: 7_000 });
            assert_eq!(view.total(), u128::from(FUNDS) + 7_000);
        }
        other => panic!("expected a restore, got {other:?}"),
    }
}

// --------------------------------------------------------------- lifecycle

#[test]
fn an_idle_store_locks_itself() {
    let scratch = Scratch::new("idle");
    let mut h = Harness::with(Config {
        idle_lock: Duration::from_millis(300),
    });
    let _ = opened(h.create_from_phrase(&scratch.store()));
    assert_eq!(h.wait_locked(Duration::from_secs(10)), LockReason::Idle);
    let locked = refusal(h.call(Command::Refresh));
    assert_eq!(locked.kind, RefusalKind::NotUnlocked);
}

#[test]
fn the_background_and_an_explicit_lock_close_the_store() {
    let scratch = Scratch::new("background");
    let dir = scratch.store();
    let mut h = Harness::new();
    let _ = opened(h.create_from_phrase(&dir));

    h.handle.background();
    assert_eq!(
        h.wait_locked(Duration::from_secs(10)),
        LockReason::Background
    );
    assert_eq!(
        refusal(h.call(Command::Refresh)).kind,
        RefusalKind::NotUnlocked
    );

    let _ = opened(h.unlock(&dir, PASSWORD));
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    assert_eq!(h.wait_locked(Duration::from_secs(5)), LockReason::Asked);
    // The lock was released: the store opens from outside the worker.
    let reopened = Keystore::open(
        &dir,
        &Unlock {
            password: PASSWORD.as_bytes(),
            nonce_seed: [9; 32],
        },
    );
    assert!(reopened.is_ok(), "the store's lock is released on Lock");

    // Locking with nothing open answers, and reports no closure.
    drop(reopened);
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    assert!(!h.seen.iter().any(|e| matches!(e, Event::Locked { .. })));
}

#[test]
fn unlocking_another_store_replaces_the_open_one() {
    let a = Scratch::new("replace-a");
    let b = Scratch::new("replace-b");
    let mut h = Harness::new();
    let _ = opened(h.create_from_phrase(&a.store()));
    let _ = opened(h.create_from_phrase(&b.store()));
    assert_eq!(h.wait_locked(Duration::from_secs(5)), LockReason::Replaced);
}

#[test]
fn shutting_down_locks_and_stops() {
    let scratch = Scratch::new("shutdown");
    let mut h = Harness::new();
    let _ = opened(h.create_from_phrase(&scratch.store()));
    h.handle.shutdown();
    assert_eq!(h.wait_locked(Duration::from_secs(10)), LockReason::Shutdown);
    match h.events.recv_timeout(Duration::from_secs(10)) {
        Ok(Event::Stopped { panicked }) => assert!(!panicked),
        other => panic!("expected the worker to stop, got {other:?}"),
    }
}

#[test]
fn dropping_the_last_handle_locks_and_stops() {
    let scratch = Scratch::new("drop");
    let mut h = Harness::new();
    let _ = opened(h.create_from_phrase(&scratch.store()));
    let Harness { handle, events, .. } = h;
    let clone = handle.clone();
    drop(handle);
    assert!(
        events.recv_timeout(Duration::from_millis(300)).is_err(),
        "a clone keeps the worker running"
    );
    drop(clone);
    match events.recv_timeout(Duration::from_secs(10)) {
        Ok(Event::Locked { reason }) => assert_eq!(reason, LockReason::Shutdown),
        other => panic!("expected the store to lock, got {other:?}"),
    }
    assert!(matches!(
        events.recv_timeout(Duration::from_secs(10)),
        Ok(Event::Stopped { panicked: false })
    ));
}

#[test]
fn a_cancel_stops_what_was_sent_before_it_and_nothing_after() {
    let scratch = Scratch::new("cancel");
    let (mut h, _) = funded(&scratch);
    // A second account, so a refresh has two to read.
    h.chain.hold(tag(1), address(1, 0), 1_000);
    match h.call(Command::Restore {
        account_index: 1,
        scan_to: None,
    }) {
        Reply::Restored {
            view: Some(view),
            text,
        } => assert_eq!(view.accounts.len(), 2, "{text}"),
        other => panic!("expected a restore, got {other:?}"),
    }

    // Raised with nothing running, it stops nothing sent afterwards.
    h.handle.cancel();
    assert!(opened(h.call(Command::Refresh)).opened);

    // Raised while a refresh waits on the node, it stops that refresh, and
    // the rows are left as they were.
    let gate = h.chain.close_gate();
    let id = h.handle.send(Command::Refresh).expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::AskingNode);
    h.handle.cancel();
    gate.open();
    assert_eq!(refusal(h.wait_for(id)).kind, RefusalKind::Cancelled);

    let view = opened(h.call(Command::Refresh));
    assert!(view.opened);
    assert_eq!(view.total(), u128::from(FUNDS) + 1_000);
}

#[test]
fn commands_that_need_a_store_are_refused_while_locked() {
    let mut h = Harness::with_node();
    for command in [
        Command::Refresh,
        Command::Receive {
            account: account0(),
        },
        Command::PlanSend {
            spend: pay(destination(1), Amount::Nano(1)),
        },
        Command::Settle {
            account: account0(),
        },
        Command::Status {
            account: account0(),
            scan_to: None,
        },
        Command::Discover { to: 3 },
        Command::Reconcile {
            account: account0(),
            advance_to: 1,
        },
        Command::Restore {
            account_index: 1,
            scan_to: None,
        },
    ] {
        let name = format!("{command:?}");
        let r = refusal(h.call(command));
        assert_eq!(r.kind, RefusalKind::NotUnlocked, "{name}");
    }
    assert_eq!(h.chain.calls(), 0, "a locked worker asks the node nothing");
}

// --------------------------------------------------------------------- node

#[test]
fn status_reports_an_account_without_refusing() {
    let scratch = Scratch::new("status");
    let (mut h, _) = funded(&scratch);
    match h.call(Command::Status {
        account: account0(),
        scan_to: None,
    }) {
        Reply::Status {
            account,
            state,
            text,
        } => {
            assert_eq!(account, account0());
            assert_eq!(state, AccountState::InSync { balance: FUNDS });
            assert!(!text.is_empty());
        }
        other => panic!("expected a status, got {other:?}"),
    }
}

#[test]
fn the_node_is_chosen_https_or_loopback_only() {
    let mut h = Harness::new();
    let none = refusal(h.call(Command::NetworkStatus));
    assert_eq!(none.kind, RefusalKind::NoNode);

    let plain = refusal(h.call(Command::SetNode {
        url: "http://node.example:8080".into(),
    }));
    assert_eq!(plain.kind, RefusalKind::NodeRefused);
    assert!(plain.text.contains("https"), "{}", plain.text);

    assert!(matches!(
        h.call(Command::SetNode {
            url: "http://127.0.0.1:8080".into()
        }),
        Reply::NodeSet { .. }
    ));
    assert!(matches!(
        h.call(Command::SetNode { url: format!(" {NODE} ") }),
        Reply::NodeSet { url } if url == NODE
    ));
    match h.call(Command::NetworkStatus) {
        Reply::Network {
            tip_index,
            tip_hash,
        } => {
            assert_eq!(tip_index, 1_000);
            assert_eq!(tip_hash, "ab".repeat(32));
        }
        other => panic!("expected the tip, got {other:?}"),
    }
    assert!(matches!(h.call(Command::ClearNode), Reply::NodeCleared));
    assert_eq!(
        refusal(h.call(Command::NetworkStatus)).kind,
        RefusalKind::NoNode
    );
}
