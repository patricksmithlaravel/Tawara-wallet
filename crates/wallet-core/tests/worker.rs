//! The worker, end to end, against a scripted node.
//!
//! Every case drives the worker through its handle, as the interface will,
//! and reads only its events. The store is a real store in a scratch
//! directory and the key derivation is the library's at its recommended
//! cost, so each create or unlock costs about a second in a test build
//! (the workspace optimises the two derivation crates in the dev profile).

mod support;

use std::time::{Duration, Instant};

use mochimo_crypto::keystore::{self, Keystore, Unlock};
use support::*;
use tawara_wallet_core::explorer::IndexState;
use tawara_wallet_core::location::{Environment, Platform, default_store_dir};
use tawara_wallet_core::spend::{Amount, DestinationInput, SpendInputError, SpendRequest};
use tawara_wallet_core::view::{
    AccountId, AccountState, DivergenceKind, NoticeKind, ReservationState, Total, WalletView,
};
use tawara_wallet_core::{
    Activity, CONFIRM_POSITIONS, Command, Config, DISCOVER_MAX_TO, Event, LockReason,
    MAX_KEY_INDEX, NetworkName, PlanView, Progress, Refusal, RefusalKind, Reply, RequestId,
    SentView, SyncState,
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
    assert_eq!(
        view.notice.as_ref().map(|n| n.kind),
        Some(NoticeKind::NoNode)
    );
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

#[cfg(unix)]
#[test]
fn a_store_the_library_will_not_write_leaves_its_phrase_waiting() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("create-unsafe");
    let dir = scratch.store();
    // An empty folder anyone may write to: the library will not put a
    // store in it.
    std::fs::create_dir_all(&dir).expect("store folder");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("mode");
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
    let answer = positions
        .iter()
        .map(|&p| words[p - 1].as_str())
        .collect::<Vec<_>>()
        .join(" ");

    // The right words, and a folder the library refuses: nothing written,
    // and the phrase still waits.
    let unsafe_folder = refusal(h.call(Command::CreateConfirm {
        answer: secret(&answer),
    }));
    assert_eq!(
        unsafe_folder.kind,
        RefusalKind::UnsafeDirectory,
        "{}",
        unsafe_folder.text
    );
    assert!(
        unsafe_folder.text.contains("Nothing was created"),
        "{}",
        unsafe_folder.text
    );
    assert_eq!(keystore::occupied(&dir), None);

    // Put right, the same words write the store.
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("mode");
    match h.call(Command::CreateConfirm {
        answer: secret(&answer),
    }) {
        Reply::Created { dir: written, .. } => assert_eq!(written, dir),
        other => panic!("expected a written store, got {other:?}"),
    }
    assert!(keystore::occupied(&dir).is_some());
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
        matches!(&view.accounts[0].state, AccountState::Diverged {
            kind: DivergenceKind::NotFound,
            report,
            advance_to: None,
        } if report.contains("account not found")),
        "{:?}",
        view.accounts[0].state
    );
    let notice = view.notice.expect("a store that is not whole says so");
    assert_eq!(notice.kind, NoticeKind::NotWhole);
    assert!(notice.starts_with("THIS STORE IS NOT WHOLE"), "{notice}");
    assert!(!notice.ends_with("---"), "{notice:?}");

    h.chain
        .hold(tag(0), address(0, view.accounts[0].index), FUNDS);
    let view = opened(h.call(Command::Refresh));
    assert!(view.opened, "{view:?}");
    assert_eq!(view.total(), Total::Whole(u128::from(FUNDS)));
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
    // A tag the store does not hold: the command line's page, which echoes
    // it in hex so it cannot be taken for a destination.
    let other = refusal(h.call(Command::Receive {
        account: AccountId::from_tag(tag(5)),
    }));
    assert!(other.text.contains(&hex_of(&tag(5))), "{}", other.text);
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
    // The library's page for a spend not yet signed, word for word.
    assert!(plan.text.starts_with("NOT SIGNED"), "{}", plan.text);
    assert!(plan.text.contains(&destination(1)), "{}", plan.text);
    assert!(
        plan.text.contains("No key has been reserved or used"),
        "{}",
        plan.text
    );
    assert!(
        !plan.text.contains("THIS EMPTIES THE ACCOUNT"),
        "{}",
        plan.text
    );
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
    // The library's own warning, before anything is signed.
    assert!(plan.text.starts_with("NOT SIGNED"), "{}", plan.text);
    assert!(
        plan.text.contains("THIS EMPTIES THE ACCOUNT"),
        "{}",
        plan.text
    );
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
    // from it. Every account diverged, so the library would refuse the
    // wallet and drop the store; the store stays open with its page.
    h.chain.hold(tag(0), address(0, start + 3), FUNDS);
    assert!(matches!(
        h.call(Command::SetNode { url: NODE.into() }),
        Reply::NodeSet { .. }
    ));
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    assert_eq!(h.wait_locked(Duration::from_secs(5)), LockReason::Asked);
    let view = opened(h.unlock(&dir, PASSWORD));
    assert!(!view.opened, "{view:?}");
    let notice = view.notice.clone().expect("the refusal page");
    assert_eq!(notice.kind, NoticeKind::WillNotStart);
    assert!(notice.starts_with("WALLET WILL NOT START"), "{notice}");
    // Asking again keeps it open: nothing needs the password twice.
    let again = opened(h.call(Command::Refresh));
    assert!(!again.opened);
    assert_eq!(again.notice, view.notice);
    let advance_to = match &view.accounts[0].state {
        AccountState::Diverged {
            advance_to: Some(to),
            kind,
            ..
        } => {
            // The chain is ahead of the store, by the keys spent elsewhere.
            assert_eq!(*kind, DivergenceKind::Ahead { gap: 3 });
            *to
        }
        other => panic!("expected a divergence with an advance, got {other:?}"),
    };
    assert_eq!(advance_to, start + 3);

    let refused = refusal(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(1_000)),
    }));
    assert_eq!(refused.kind, RefusalKind::WalletNotOpen);

    // An index the report does not name: the library refuses it, and the
    // store is still open afterwards.
    match h.call(Command::Reconcile {
        account: account0(),
        advance_to: advance_to - 1,
    }) {
        Reply::Reconciled {
            ok,
            advanced_to,
            text,
            opened: Ok(view),
        } => {
            assert!(!ok, "{text}");
            assert_eq!(advanced_to, None, "{text}");
            assert!(!view.opened);
            assert_eq!(view.accounts[0].index, start);
        }
        other => panic!("expected the refused advance, got {other:?}"),
    }
    assert!(
        !h.seen.iter().any(|e| matches!(e, Event::Locked { .. })),
        "the store was never closed: {:?}",
        h.seen
    );

    let id = h
        .handle
        .send(Command::Reconcile {
            account: account0(),
            advance_to,
        })
        .expect("worker running");
    match h.wait_for(id) {
        Reply::Reconciled {
            ok,
            advanced_to,
            text,
            opened: Ok(view),
        } => {
            assert!(ok, "{text}");
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
    // The advance reported its account before reviewing it and again
    // before the re-check that comes before its write, and the wallet
    // opening after it reported it once more. None walked far enough to
    // report a position.
    let advancing = progress_of(&h, id);
    assert_eq!(advancing.len(), 3, "{advancing:?}");
    assert!(
        advancing
            .iter()
            .all(|p| p.account == 0 && p.accounts == 1 && p.position == 0),
        "{advancing:?}"
    );
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
            ok,
            text,
            opened: Ok(view),
        } => {
            assert!(ok, "{text}");
            assert!(view.opened, "{text}");
            assert_eq!(view.accounts.len(), 2, "{text}");
            let row = view
                .accounts
                .iter()
                .find(|r| r.id == AccountId::from_tag(tag(2)))
                .expect("the restored account");
            assert_eq!(row.index, 4, "{text}");
            assert_eq!(row.state, AccountState::InSync { balance: 7_000 });
            assert_eq!(view.total(), Total::Whole(u128::from(FUNDS) + 7_000));
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
fn a_shorter_idle_period_takes_effect_at_once() {
    let scratch = Scratch::new("idle-period");
    let mut h = Harness::with(Config {
        idle_lock: Duration::from_secs(3_600),
    });
    let _ = opened(h.create_from_phrase(&scratch.store()));
    // The worker is waiting out the hour it started with; the new period
    // wakes it, and counts from the person's last input.
    h.handle.set_idle_lock(Duration::from_millis(300));
    let started = Instant::now();
    assert_eq!(h.wait_locked(Duration::from_secs(10)), LockReason::Idle);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn an_idle_lock_drops_a_waiting_phrase_and_says_so() {
    let scratch = Scratch::new("idle-phrase");
    let mut h = Harness::with(Config {
        idle_lock: Duration::from_millis(300),
    });
    assert!(matches!(
        h.call(Command::CreateBegin {
            dir: scratch.store(),
            password: secret(PASSWORD),
            password_again: secret(PASSWORD),
        }),
        Reply::CreatePhrase { .. }
    ));
    assert_eq!(h.wait_locked(Duration::from_secs(10)), LockReason::Idle);
    let none = refusal(h.call(Command::CreateConfirm {
        answer: secret("a b c"),
    }));
    assert_eq!(none.kind, RefusalKind::NothingToConfirm);
    assert_eq!(keystore::occupied(&scratch.store()), None);
}

#[test]
fn touching_keeps_the_store_open_and_polling_does_not() {
    let scratch = Scratch::new("polling");
    let mut h = Harness::with(Config {
        idle_lock: Duration::from_secs(1),
    });
    assert!(matches!(
        h.call(Command::SetNode { url: NODE.into() }),
        Reply::NodeSet { .. }
    ));
    let _ = opened(h.create_from_phrase(&scratch.store()));

    let until = Instant::now() + Duration::from_millis(1_500);
    while Instant::now() < until {
        h.handle.touch();
        std::thread::sleep(Duration::from_millis(100));
    }
    while let Ok(event) = h.events.try_recv() {
        assert!(
            !matches!(event, Event::Locked { .. }),
            "locked while touched"
        );
    }

    // Asking the node over and over is not the person doing anything.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !h.seen.iter().any(|e| matches!(e, Event::Locked { .. })) {
        assert!(
            Instant::now() < deadline,
            "the store never locked while it was polled"
        );
        assert!(matches!(
            h.call(Command::NetworkStatus),
            Reply::Network { .. }
        ));
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(h.wait_locked(Duration::ZERO), LockReason::Idle);
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
            ok: true,
            opened: Ok(view),
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
    assert_eq!(view.total(), Total::Whole(u128::from(FUNDS) + 1_000));

    // A Lock sent before a cancel still locks.
    let gate = h.chain.close_gate();
    let id = h.handle.send(Command::Refresh).expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::AskingNode);
    let lock = h.handle.send(Command::Lock).expect("worker running");
    h.handle.cancel();
    gate.open();
    assert_eq!(refusal(h.wait_for(id)).kind, RefusalKind::Cancelled);
    assert!(matches!(h.wait_for(lock), Reply::Locked));
    assert_eq!(h.wait_locked(Duration::from_secs(5)), LockReason::Asked);
}

#[test]
fn moving_to_the_background_stops_a_queued_spend_before_it_signs() {
    let scratch = Scratch::new("background-spend");
    let dir = scratch.store();
    let (mut h, start) = funded(&scratch);
    let plan = planned(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(100_000)),
    }));

    // A refresh waits on the node, and the confirmation is queued behind
    // it when the app goes to the background.
    let gate = h.chain.close_gate();
    let refresh = h.handle.send(Command::Refresh).expect("worker running");
    assert_eq!(h.wait_busy(refresh), Activity::AskingNode);
    let confirm = h
        .handle
        .send(Command::ConfirmSend { plan: plan.plan })
        .expect("worker running");
    h.handle.background();
    gate.open();

    assert_eq!(refusal(h.wait_for(refresh)).kind, RefusalKind::Cancelled);
    assert_eq!(refusal(h.wait_for(confirm)).kind, RefusalKind::Cancelled);
    assert_eq!(
        h.wait_locked(Duration::from_secs(10)),
        LockReason::Background
    );
    assert!(h.chain.submits().is_empty(), "nothing was sent");

    // Nothing was reserved either: the account is as it was.
    let view = opened(h.unlock(&dir, PASSWORD));
    assert_eq!(
        view.accounts[0].state,
        AccountState::InSync { balance: FUNDS }
    );
    assert_eq!(view.accounts[0].index, start);
    assert!(view.accounts[0].spendable);
}

#[test]
fn a_cancelled_status_is_not_a_report() {
    let scratch = Scratch::new("cancel-status");
    let (mut h, _) = funded(&scratch);
    h.chain.hold(tag(1), address(1, 0), 1_000);
    match h.call(Command::Restore {
        account_index: 1,
        scan_to: None,
    }) {
        Reply::Restored { ok: true, .. } => {}
        other => panic!("expected a restore, got {other:?}"),
    }
    // Another wallet spends from account 1: the chain is three keys ahead,
    // so reading it walks the key positions, which a cancel stops.
    h.chain.hold(tag(1), address(1, 3), 900);
    let account1 = AccountId::from_tag(tag(1));

    let gate = h.chain.close_gate();
    let id = h
        .handle
        .send(Command::Status {
            account: account1,
            scan_to: None,
        })
        .expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::AskingNode);
    h.handle.cancel();
    gate.open();
    assert_eq!(refusal(h.wait_for(id)).kind, RefusalKind::Cancelled);

    match h.call(Command::Status {
        account: account1,
        scan_to: None,
    }) {
        Reply::Status {
            state:
                AccountState::Diverged {
                    advance_to: Some(3),
                    ..
                },
            ..
        } => {}
        other => panic!("expected the divergence and its advance, got {other:?}"),
    }
}

#[test]
fn scans_advances_and_sweeps_are_bounded() {
    let scratch = Scratch::new("bounds");
    let mut h = Harness::with_node();
    let _ = opened(h.create_from_phrase(&scratch.store()));
    let calls = h.chain.calls();
    for command in [
        Command::Status {
            account: account0(),
            scan_to: Some(MAX_KEY_INDEX + 1),
        },
        Command::Restore {
            account_index: 1,
            scan_to: Some(MAX_KEY_INDEX + 1),
        },
        Command::Reconcile {
            account: account0(),
            advance_to: u32::MAX,
        },
        Command::Discover { to: 0 },
        Command::Discover {
            to: DISCOVER_MAX_TO + 1,
        },
    ] {
        let name = format!("{command:?}");
        let r = refusal(h.call(command));
        assert_eq!(r.kind, RefusalKind::OutOfRange, "{name}");
        assert!(r.text.contains("Nothing was done"), "{name}: {}", r.text);
    }
    assert_eq!(h.chain.calls(), calls, "nothing reached the node");
    assert!(
        matches!(
            h.call(Command::Receive {
                account: account0()
            }),
            Reply::Receive(_)
        ),
        "the store is still open"
    );
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
    let status = |h: &mut Harness| match h.call(Command::Status {
        account: account0(),
        scan_to: None,
    }) {
        Reply::Status {
            account,
            state,
            spendable,
            text,
        } => {
            assert_eq!(account, account0());
            assert!(!text.is_empty());
            (state, spendable)
        }
        other => panic!("expected a status, got {other:?}"),
    };
    assert_eq!(
        status(&mut h),
        (AccountState::InSync { balance: FUNDS }, true)
    );

    // A check the node did not answer is a report, and the account does
    // not spend; once it answers, the account the wallet held spends again.
    h.chain.set_unreachable(true);
    let (state, spendable) = status(&mut h);
    assert!(
        matches!(
            state,
            AccountState::Diverged {
                kind: DivergenceKind::Unreachable,
                ..
            }
        ),
        "{state:?}"
    );
    assert!(!spendable);
    h.chain.set_unreachable(false);
    assert_eq!(
        status(&mut h),
        (AccountState::InSync { balance: FUNDS }, true)
    );
    let view = opened(h.call(Command::Refresh));
    assert!(view.accounts[0].spendable, "{view:?}");
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
        Reply::NodeSet { url, view: None } if url == NODE
    ));
    match h.call(Command::NetworkStatus) {
        Reply::Network {
            tip_index,
            tip_hash,
            tip_time_ms,
            sync,
        } => {
            assert_eq!(tip_index, 1_000);
            assert_eq!(tip_hash, "ab".repeat(32));
            assert_eq!(tip_time_ms, 1_000 * 60_000);
            assert_eq!(
                sync,
                Some(SyncState {
                    stage: "synchronized".into(),
                    synced: true
                })
            );
        }
        other => panic!("expected the tip, got {other:?}"),
    }
    // The middleware's own word for a refresh that failed, as it sent it.
    h.chain.sync("latest block error", false);
    match h.call(Command::NetworkStatus) {
        Reply::Network { sync, .. } => assert_eq!(
            sync,
            Some(SyncState {
                stage: "latest block error".into(),
                synced: false
            })
        ),
        other => panic!("expected the tip, got {other:?}"),
    }
    match h.call(Command::Networks) {
        Reply::Networks(names) => assert_eq!(
            names,
            vec![NetworkName {
                blockchain: "mochimo".into(),
                network: "mainnet".into()
            }]
        ),
        other => panic!("expected the networks, got {other:?}"),
    }
    assert!(matches!(
        h.call(Command::ClearNode),
        Reply::NodeCleared { view: None }
    ));
    assert_eq!(
        refusal(h.call(Command::NetworkStatus)).kind,
        RefusalKind::NoNode
    );
    assert_eq!(refusal(h.call(Command::Networks)).kind, RefusalKind::NoNode);
}

// ------------------------------------------------------- node and idle edges

#[test]
fn clearing_or_changing_the_node_stops_the_open_wallet_using_the_old_one() {
    let scratch = Scratch::new("node-change");
    let (mut h, start) = funded(&scratch);
    let plan = planned(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(100_000)),
    }));

    // Forget the node: the spend laid out against it does not go to it,
    // and the store stays open without it.
    match h.call(Command::ClearNode) {
        Reply::NodeCleared { view: Some(view) } => {
            assert!(!view.opened, "{view:?}");
            assert_eq!(view.accounts[0].state, AccountState::NotReconciled);
            assert!(
                view.notice
                    .as_deref()
                    .is_some_and(|n| n.contains("No node is chosen")),
                "{:?}",
                view.notice
            );
        }
        other => panic!("expected the store detached, got {other:?}"),
    }
    let r = refusal(h.call(Command::ConfirmSend { plan: plan.plan }));
    assert_eq!(r.kind, RefusalKind::WalletNotOpen, "{}", r.text);
    assert!(
        h.chain.submits().is_empty(),
        "nothing went to the node that was cleared"
    );
    let calls = h.chain.calls();
    assert_eq!(
        refusal(h.call(Command::Status {
            account: account0(),
            scan_to: None,
        }))
        .kind,
        RefusalKind::NoNode
    );
    assert_eq!(
        refusal(h.call(Command::Discover { to: 3 })).kind,
        RefusalKind::NoNode
    );
    assert_eq!(h.chain.calls(), calls, "the cleared node is asked nothing");

    // Another node: every command that asks a node asks it from now on.
    h.chain_b.hold(tag(0), address(0, start), FUNDS);
    match h.call(Command::SetNode { url: NODE_B.into() }) {
        Reply::NodeSet {
            url,
            view: Some(view),
        } => {
            assert_eq!(url, NODE_B);
            assert!(!view.opened, "{view:?}");
        }
        other => panic!("expected the node set, got {other:?}"),
    }
    let r = refusal(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(100_000)),
    }));
    assert_eq!(r.kind, RefusalKind::WalletNotOpen, "{}", r.text);
    match h.call(Command::Status {
        account: account0(),
        scan_to: None,
    }) {
        // The store is open on its own, not as a wallet: nothing spends
        // until a refresh opens it.
        Reply::Status {
            state: AccountState::InSync { balance },
            spendable: false,
            ..
        } => assert_eq!(balance, FUNDS),
        other => panic!("expected the new node's answer, got {other:?}"),
    }
    assert!(h.chain_b.calls() > 0, "the new node was asked");
    let view = opened(h.call(Command::Refresh));
    assert!(view.opened, "{view:?}");
    assert_eq!(
        view.accounts[0].state,
        AccountState::InSync { balance: FUNDS }
    );
    assert_eq!(
        h.chain.calls(),
        calls,
        "the first node is asked nothing after the change"
    );
    assert!(h.chain.submits().is_empty() && h.chain_b.submits().is_empty());

    // Choosing the node already chosen changes nothing.
    match h.call(Command::SetNode { url: NODE_B.into() }) {
        Reply::NodeSet {
            view: Some(view), ..
        } => assert!(view.opened, "{view:?}"),
        other => panic!("expected the node set, got {other:?}"),
    }
}

#[test]
fn queued_polls_do_not_keep_the_store_open() {
    let scratch = Scratch::new("poll-backlog");
    let mut h = Harness::with(Config {
        idle_lock: Duration::from_secs(1),
    });
    assert!(matches!(
        h.call(Command::SetNode { url: NODE.into() }),
        Reply::NodeSet { .. }
    ));
    let _ = opened(h.create_from_phrase(&scratch.store()));

    // The interface polls faster than the node answers, so polls queue up:
    // a three-second backlog against a one-second idle period.
    h.chain.slow(Duration::from_millis(250));
    let ids: Vec<_> = (0..12)
        .map(|_| {
            h.handle
                .send(Command::NetworkStatus)
                .expect("worker running")
        })
        .collect();
    let last = ids[ids.len() - 1];
    let mut locked = None;
    loop {
        match h
            .events
            .recv_timeout(Duration::from_secs(60))
            .expect("an event")
        {
            Event::Locked { reason } => locked = Some(reason),
            Event::Done { id, reply } => {
                assert!(matches!(reply, Reply::Network { .. }), "{reply:?}");
                if id == last {
                    break;
                }
            }
            _ => {}
        }
    }
    assert_eq!(
        locked,
        Some(LockReason::Idle),
        "the store stayed open while polls were queued"
    );
}

// ------------------------------------------------- default folder, recovery

#[test]
fn creating_at_the_default_location_makes_its_application_folder() {
    let scratch = Scratch::new("default-dir");
    let root = scratch.0.to_str().expect("a UTF-8 scratch path");
    let data = scratch.0.join("data");
    let local = scratch.0.join("Local");
    // A fresh profile: none of the folders the default path runs through
    // exist yet below the home folder.
    let env = Environment::of(&[
        ("HOME", root),
        ("XDG_DATA_HOME", data.to_str().expect("UTF-8")),
        ("LOCALAPPDATA", local.to_str().expect("UTF-8")),
    ]);
    let Ok(dir) = default_store_dir(Platform::current(), &env) else {
        return; // Android and iOS: the shell supplies the folder.
    };
    let app = dir.parent().expect("the application folder").to_path_buf();
    assert!(!app.exists(), "{} should not exist yet", app.display());

    let mut h = Harness::new();
    let view = opened(h.create_from_phrase(&dir));
    assert_eq!(view.dir, dir);
    assert!(keystore::occupied(&dir).is_some());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&app)
            .expect("app folder")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o077,
            0,
            "the application folder is private: {mode:o}"
        );
    }

    // The confirmed path makes it too, and only once the phrase is
    // confirmed.
    let other = scratch.0.join("elsewhere").join("Tawara").join("keystore");
    let (words, positions) = match h.call(Command::CreateBegin {
        dir: other.clone(),
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
    assert!(
        !scratch.0.join("elsewhere").exists(),
        "nothing is made before the confirmation"
    );
    let answer = positions
        .iter()
        .map(|&p| words[p - 1].as_str())
        .collect::<Vec<_>>()
        .join(" ");
    match h.call(Command::CreateConfirm {
        answer: secret(&answer),
    }) {
        Reply::Created {
            opened: Ok(view), ..
        } => assert_eq!(view.dir, other),
        other => panic!("expected a written store, got {other:?}"),
    }
}

#[test]
fn an_account_set_aside_at_open_settles_and_rejoins_once_it_reconciles() {
    let scratch = Scratch::new("recover");
    let dir = scratch.store();
    let (mut h, _) = funded(&scratch);
    let account1 = AccountId::from_tag(tag(1));
    h.chain.hold(tag(1), address(1, 0), 1_000_000);
    match h.call(Command::Restore {
        account_index: 1,
        scan_to: None,
    }) {
        Reply::Restored { ok: true, .. } => {}
        other => panic!("expected a restore, got {other:?}"),
    }
    let from1 = |amount| SpendRequest {
        from: account1,
        ..pay(destination(2), amount)
    };
    let plan = planned(h.call(Command::PlanSend {
        spend: from1(Amount::Nano(100_000)),
    }));
    let _ = sent(h.call(Command::ConfirmSend { plan: plan.plan }));

    // Opened again while the node answers "account not found" for account
    // 1: the wallet opens with account 0 and sets account 1 aside.
    h.chain.forget(tag(1));
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    let view = opened(h.unlock(&dir, PASSWORD));
    assert!(view.opened, "{view:?}");
    let row = |v: &WalletView| {
        v.accounts
            .iter()
            .find(|r| r.id == account1)
            .cloned()
            .expect("account 1")
    };
    assert!(
        matches!(row(&view).state, AccountState::Diverged { .. }),
        "{:?}",
        row(&view)
    );

    // The spend landed: the node now holds account 1 at its change key.
    // Settling asks the node afresh, as the library's settle does, rather
    // than refusing on what it found when the wallet opened.
    h.chain.hold(tag(1), address(1, 1), plan.change_total);
    match h.call(Command::Settle { account: account1 }) {
        Reply::Settled { view, text } => {
            assert_eq!(
                row(&view).state,
                AccountState::InSync {
                    balance: plan.change_total
                },
                "{text}"
            );
            assert_eq!(row(&view).index, 1);
        }
        other => panic!("expected the settlement, got {other:?}"),
    }

    // A refresh opens the wallet again so the account can spend.
    let view = opened(h.call(Command::Refresh));
    assert!(view.opened);
    assert!(row(&view).spendable, "{:?}", row(&view));
    let _ = planned(h.call(Command::PlanSend {
        spend: from1(Amount::Nano(1_000)),
    }));
}

// ------------------------------------------------- teardown, failed reopen

#[test]
fn dropping_the_last_handle_stops_a_queued_spend_before_it_signs() {
    let scratch = Scratch::new("drop-queued");
    let (mut h, _) = funded(&scratch);
    let plan = planned(h.call(Command::PlanSend {
        spend: pay(destination(1), Amount::Nano(100_000)),
    }));

    // A refresh waits on the node, the confirmation is queued behind it,
    // and the interface goes away.
    let gate = h.chain.close_gate();
    let refresh = h.handle.send(Command::Refresh).expect("worker running");
    assert_eq!(h.wait_busy(refresh), Activity::AskingNode);
    let confirm = h
        .handle
        .send(Command::ConfirmSend { plan: plan.plan })
        .expect("worker running");
    let Harness {
        handle,
        events,
        chain,
        ..
    } = h;
    drop(handle);
    gate.open();

    let mut answers = Vec::new();
    let mut locked = None;
    loop {
        match events
            .recv_timeout(Duration::from_secs(60))
            .expect("an event")
        {
            Event::Done { id, reply } => answers.push((id, reply)),
            Event::Locked { reason } => locked = Some(reason),
            Event::Stopped { panicked } => {
                assert!(!panicked);
                break;
            }
            Event::Busy { .. } | Event::Progress { .. } => {}
        }
    }
    assert!(chain.submits().is_empty(), "nothing was signed or sent");
    assert_eq!(locked, Some(LockReason::Shutdown));
    for (id, reply) in &answers {
        if *id == refresh || *id == confirm {
            assert!(
                matches!(reply, Reply::Refused(r) if r.kind == RefusalKind::Cancelled),
                "{reply:?}"
            );
        }
    }
}

/// A single-account store whose account diverged three keys ahead, open in
/// the Store session against [`NODE`], and the index the advance goes to.
fn diverged_store(scratch: &Scratch) -> (Harness, u32) {
    let mut h = Harness::new();
    let view = opened(h.create_from_phrase(&scratch.store()));
    let advance_to = view.accounts[0].index + 3;
    h.chain.hold(tag(0), address(0, advance_to), FUNDS);
    let _ = h.call(Command::SetNode { url: NODE.into() });
    let view = opened(h.call(Command::Refresh));
    assert!(!view.opened, "{view:?}");
    (h, advance_to)
}

#[test]
fn a_refused_reopen_after_an_advance_keeps_the_store_open() {
    // How many times the advance and the reopen after it resolve the tag.
    let dry = Scratch::new("reopen-dry");
    let (mut h, advance_to) = diverged_store(&dry);
    let before = h.chain.resolves(tag(0));
    match h.call(Command::Reconcile {
        account: account0(),
        advance_to,
    }) {
        Reply::Reconciled { ok: true, .. } => {}
        other => panic!("expected the advance, got {other:?}"),
    }
    let per_command = h.chain.resolves(tag(0)) - before;
    drop(h);

    // The same again, with the node answering "account not found" from the
    // last of those on: when the wallet is opened again after the advance.
    // The library refuses it, and hands the store back: it stays open on
    // its own, with the library's report, and nothing asks for the
    // password again.
    let scratch = Scratch::new("reopen");
    let (mut h, advance_to) = diverged_store(&scratch);
    let from = h.chain.resolves(tag(0)) + per_command;
    h.chain.vanish_from(tag(0), from);
    match h.call(Command::Reconcile {
        account: account0(),
        advance_to,
    }) {
        Reply::Reconciled {
            ok,
            advanced_to: Some(index),
            text,
            opened: Ok(view),
        } => {
            assert!(ok, "{text}");
            assert_eq!(index, advance_to, "the advance stands");
            assert!(!view.opened, "{view:?}");
            assert_eq!(view.accounts[0].index, advance_to);
            let notice = view.notice.expect("the library's report");
            assert!(notice.starts_with("THIS STORE IS NOT WHOLE"), "{notice}");
            assert!(
                notice.contains("account not found"),
                "with the account's own report: {notice}"
            );
        }
        other => panic!("expected the advance and the refused reopen, got {other:?}"),
    }
    assert!(
        !h.seen.iter().any(|e| matches!(e, Event::Locked { .. })),
        "the store was never closed: {:?}",
        h.seen
    );

    // The node answers again: the same store opens, as it is.
    h.chain.vanish_from(tag(0), usize::MAX);
    let view = opened(h.call(Command::Refresh));
    assert!(view.opened, "{view:?}");
    assert_eq!(view.accounts[0].index, advance_to);
}

// ------------------------------------------- the long operations: cancel

/// A funded store with a second account, restored, both in sync.
fn two_accounts(scratch: &Scratch) -> Harness {
    let (mut h, _) = funded(scratch);
    h.chain.hold(tag(1), address(1, 0), 1_000);
    match h.call(Command::Restore {
        account_index: 1,
        scan_to: None,
    }) {
        Reply::Restored {
            ok: true,
            opened: Ok(view),
            text,
        } => assert_eq!(view.accounts.len(), 2, "{text}"),
        other => panic!("expected a restore, got {other:?}"),
    }
    h
}

#[test]
fn a_cancelled_unlock_leaves_the_store_open_and_unreconciled() {
    let scratch = Scratch::new("cancel-unlock");
    let dir = scratch.store();
    let mut h = two_accounts(&scratch);
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    assert_eq!(h.wait_locked(Duration::from_secs(5)), LockReason::Asked);

    // The node holds the first account's lookup while the person cancels:
    // the wallet stops opening before it asks about the second.
    let gate = h.chain.close_gate();
    let calls = h.chain.calls();
    let id = h
        .handle
        .send(Command::Unlock {
            dir: dir.clone(),
            password: secret(PASSWORD),
        })
        .expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::DerivingKey);
    assert_eq!(h.wait_busy(id), Activity::AskingNode);
    h.handle.cancel();
    gate.open();
    let view = match h.wait_for(id) {
        Reply::Unlocked(view) => view,
        other => panic!("expected the store open on its own, got {other:?}"),
    };
    assert!(!view.opened, "{view:?}");
    assert!(
        view.notice
            .as_deref()
            .is_some_and(|n| n.contains("cancelled")),
        "{:?}",
        view.notice
    );
    assert_eq!(view.accounts.len(), 2);
    assert!(
        view.accounts
            .iter()
            .all(|r| r.state == AccountState::NotReconciled && !r.spendable),
        "{view:?}"
    );
    assert!(
        h.chain.calls() - calls <= 1,
        "nothing was asked after the cancel"
    );
    assert!(
        !h.seen.iter().any(|e| matches!(e, Event::Locked { .. })),
        "the store stayed open"
    );

    // Nothing needs the password again: a refresh opens the wallet.
    let view = opened(h.call(Command::Refresh));
    assert!(view.opened, "{view:?}");
    assert_eq!(view.total(), Total::Whole(u128::from(FUNDS) + 1_000));
}

#[test]
fn a_cancelled_restore_writes_nothing() {
    let scratch = Scratch::new("cancel-restore");
    let (mut h, _) = funded(&scratch);
    h.chain.hold(tag(1), address(1, 0), 1_000);

    let gate = h.chain.close_gate();
    let calls = h.chain.calls();
    let id = h
        .handle
        .send(Command::Restore {
            account_index: 1,
            scan_to: None,
        })
        .expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::AskingNode);
    h.handle.cancel();
    gate.open();
    match h.wait_for(id) {
        Reply::Restored {
            ok,
            text,
            opened: Ok(view),
        } => {
            assert!(!ok, "{text}");
            assert!(text.contains("cancelled"), "{text}");
            assert_eq!(view.accounts.len(), 1, "nothing was added: {view:?}");
            assert!(!view.opened, "{view:?}");
            assert!(
                view.notice
                    .as_deref()
                    .is_some_and(|n| n.contains("cancelled")),
                "{:?}",
                view.notice
            );
        }
        other => panic!("expected the cancelled restore, got {other:?}"),
    }
    // At most the restore's own lookup: the wallet opening after it heard
    // the same cancel before it asked anything.
    assert!(h.chain.calls() - calls <= 1);
    assert!(
        !h.seen.iter().any(|e| matches!(e, Event::Locked { .. })),
        "the store stayed open"
    );

    let view = opened(h.call(Command::Refresh));
    assert!(view.opened, "{view:?}");
    assert_eq!(view.accounts.len(), 1, "{view:?}");
}

#[test]
fn a_cancelled_advance_moves_no_key_index() {
    let scratch = Scratch::new("cancel-advance");
    let (mut h, advance_to) = diverged_store(&scratch);

    let gate = h.chain.close_gate();
    let id = h
        .handle
        .send(Command::Reconcile {
            account: account0(),
            advance_to,
        })
        .expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::AskingNode);
    h.handle.cancel();
    gate.open();
    match h.wait_for(id) {
        Reply::Reconciled {
            ok,
            advanced_to,
            text,
            opened: Ok(view),
        } => {
            assert!(!ok, "{text}");
            assert_eq!(advanced_to, None, "{text}");
            assert!(text.contains("cancelled"), "{text}");
            assert!(!view.opened, "{view:?}");
            assert_eq!(view.accounts[0].index, advance_to - 3, "{view:?}");
        }
        other => panic!("expected the cancelled advance, got {other:?}"),
    }

    // Still diverged where it was: the library names the same advance.
    let view = opened(h.call(Command::Refresh));
    assert!(!view.opened, "{view:?}");
    assert!(
        matches!(
            &view.accounts[0].state,
            AccountState::Diverged { advance_to: Some(to), .. } if *to == advance_to
        ),
        "{:?}",
        view.accounts[0].state
    );
}

#[test]
fn a_cancelled_sweep_reports_nothing_found() {
    let scratch = Scratch::new("cancel-sweep");
    let (mut h, _) = funded(&scratch);

    let gate = h.chain.close_gate();
    let calls = h.chain.calls();
    let id = h
        .handle
        .send(Command::Discover { to: 50 })
        .expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::AskingNode);
    h.handle.cancel();
    gate.open();
    assert_eq!(refusal(h.wait_for(id)).kind, RefusalKind::Cancelled);
    assert!(
        h.chain.calls() - calls <= 1,
        "it stopped before the next request"
    );
    assert!(opened(h.call(Command::Refresh)).opened);
}

#[test]
fn the_idle_period_stops_a_walk_to_a_far_index_and_locks() {
    let scratch = Scratch::new("idle-walk");
    let mut h = Harness::with(Config {
        idle_lock: Duration::from_millis(1_500),
    });
    assert!(matches!(
        h.call(Command::SetNode { url: NODE.into() }),
        Reply::NodeSet { .. }
    ));
    let view = opened(h.create_from_phrase(&scratch.store()));
    assert!(!view.opened, "nothing is on the ledger yet: {view:?}");

    // The node holds the account under its own tag at a key none of its
    // positions derives (another account's), so the search runs to the
    // index named, the last there is: far longer than the idle period,
    // which stops it and locks.
    let mut elsewhere = address(0, 0);
    elsewhere[20..].copy_from_slice(&address(5, 0)[20..]);
    h.chain.hold(tag(0), elsewhere, FUNDS);
    let id = h
        .handle
        .send(Command::Status {
            account: account0(),
            scan_to: Some(MAX_KEY_INDEX),
        })
        .expect("worker running");
    assert_eq!(refusal(h.wait_for(id)).kind, RefusalKind::Cancelled);
    assert_eq!(h.wait_locked(Duration::from_secs(10)), LockReason::Idle);
}

// ----------------------------------------- the long operations: progress

/// The progress reports seen for request `id`, in the order they came.
fn progress_of(h: &Harness, id: RequestId) -> Vec<Progress> {
    h.seen
        .iter()
        .filter_map(|e| match e {
            Event::Progress { id: got, progress } if *got == id => Some(*progress),
            _ => None,
        })
        .collect()
}

#[test]
fn long_operations_report_how_far_they_have_got() {
    let scratch = Scratch::new("progress");
    let dir = scratch.store();
    let (mut h, _) = funded(&scratch);

    // A sweep reports each account it is about to ask about, and walks
    // nothing.
    let id = h
        .handle
        .send(Command::Discover { to: 3 })
        .expect("worker running");
    assert!(matches!(h.wait_for(id), Reply::Discovered { .. }));
    let swept = progress_of(&h, id);
    assert_eq!(
        swept.iter().map(|p| p.account).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert!(
        swept
            .iter()
            .all(|p| p.accounts == 4 && p.position == 0 && p.ceiling == 0),
        "{swept:?}"
    );

    // A restore that walks three hundred key positions reports along the
    // way, and the wallet opening after it reports each account.
    h.chain.hold(tag(2), address(2, 300), 7_000);
    let id = h
        .handle
        .send(Command::Restore {
            account_index: 2,
            scan_to: None,
        })
        .expect("worker running");
    match h.wait_for(id) {
        Reply::Restored {
            ok: true,
            opened: Ok(view),
            text,
        } => assert!(view.opened, "{text}"),
        other => panic!("expected a restore, got {other:?}"),
    }
    let restored = progress_of(&h, id);
    assert!(
        restored
            .iter()
            .any(|p| p.accounts == 1 && p.position == 256 && p.ceiling >= 300),
        "{restored:?}"
    );
    assert_eq!(
        restored
            .iter()
            .filter(|p| p.accounts == 2)
            .map(|p| p.account)
            .collect::<Vec<_>>(),
        [0, 1],
        "{restored:?}"
    );

    // Unlocking reports each account as the wallet opens.
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    let id = h
        .handle
        .send(Command::Unlock {
            dir,
            password: secret(PASSWORD),
        })
        .expect("worker running");
    assert!(opened(h.wait_for(id)).opened);
    let opening = progress_of(&h, id);
    assert_eq!(
        opening.iter().map(|p| p.account).collect::<Vec<_>>(),
        [0, 1],
        "{opening:?}"
    );
    assert!(opening.iter().all(|p| p.accounts == 2), "{opening:?}");
}

// ---------------------------------------------------------------- explorer

#[test]
fn activity_reads_every_account_from_the_index_and_says_when_there_is_none() {
    use tawara_wallet_core::explorer::Direction;
    let scratch = Scratch::new("activity");
    let (mut h, _) = funded(&scratch);
    let payee = AccountId::from_tag(tag(5));
    h.chain
        .index_transfer(tag(0), tag(5), 400_000, 1_000, 990, "INV-7");

    let id = h.handle.send(Command::Activity).expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::ReadingIndex);
    let histories = match h.wait_for(id) {
        Reply::Activity(Ok(histories)) => histories,
        other => panic!("expected the activity, got {other:?}"),
    };
    assert_eq!(histories.len(), 1, "one per account in the store");
    let mine = &histories[0];
    assert_eq!(mine.account, account0());
    assert_eq!(mine.total, 1);
    assert!(!mine.more());
    assert!(mine.text.contains("recent transactions"), "{}", mine.text);
    let tx = &mine.transactions[0];
    assert_eq!(tx.block, Some(990));
    assert_eq!(tx.time_ms, Some(990 * 60_000));
    assert_eq!(tx.direction(account0()), Direction::Both);
    assert_eq!(tx.net(account0()), -400_500, "what left, net of the change");
    assert_eq!(tx.paid_out(account0()).count(), 1);
    assert_eq!(tx.fee(), 500);
    assert_eq!(tx.net(payee), 400_000);
    let paid: Vec<_> = tx.paid_out(account0()).collect();
    assert_eq!(
        paid[0].memo, "INV-7",
        "the index's row carries the reference, its padding left off"
    );

    // A node that runs no index does not serve the search, for any account.
    h.chain.set_no_index(true);
    match h.call(Command::Activity) {
        Reply::Activity(Err(refused)) => {
            assert_eq!(refused.index, Some(IndexState::Absent));
            assert!(refused.text.contains("HTTP 404"), "{}", refused.text);
        }
        other => panic!("expected the index refused, got {other:?}"),
    }
    // One that runs an index that does not answer answers code 2.
    h.chain.set_no_index(false);
    h.chain.set_index_down(true);
    match h.call(Command::OlderActivity(vec![(account0(), 0)])) {
        Reply::Activity(Err(refused)) => {
            assert_eq!(refused.index, Some(IndexState::Unavailable));
            assert!(refused.text.contains("code 2"), "{}", refused.text);
        }
        other => panic!("expected the index refused, got {other:?}"),
    }

    // A cancel stops it before it starts, and nothing is reported.
    h.chain.set_index_down(false);
    let gate = h.chain.close_gate();
    let id = h.handle.send(Command::Activity).expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::ReadingIndex);
    h.handle.cancel();
    gate.open();
    assert_eq!(refusal(h.wait_for(id)).kind, RefusalKind::Cancelled);
}

#[test]
fn older_activity_reads_on_from_where_the_last_page_ended() {
    let scratch = Scratch::new("older-activity");
    let (mut h, _) = funded(&scratch);
    for n in 0..105 {
        h.chain
            .index_transfer(tag(0), tag(5), 1_000 + n, 1_000, 900 + n, "");
    }
    let mut mine = match h.call(Command::Activity) {
        Reply::Activity(Ok(mut histories)) => histories.remove(0),
        other => panic!("expected the activity, got {other:?}"),
    };
    assert_eq!(mine.transactions.len(), 100);
    assert_eq!((mine.total, mine.next, mine.unread()), (105, 100, 5));
    assert!(mine.more());

    // A row the index gains at the top pushes the rest down a place: the
    // older page repeats one row, which is listed once.
    h.chain
        .index_transfer(tag(0), tag(5), 9_999, 1_000, 1_100, "");
    let older = match h.call(Command::OlderActivity(vec![
        (account0(), mine.next),
        (AccountId::from_tag(tag(9)), 0),
    ])) {
        Reply::Activity(Ok(histories)) => histories,
        other => panic!("expected the older page, got {other:?}"),
    };
    assert_eq!(
        older.len(),
        1,
        "an account the store does not hold is not read"
    );
    assert_eq!(older[0].transactions.len(), 6);
    let first_text = mine.text.clone();
    mine.extend(older.into_iter().next().expect("one page"));
    assert_eq!(mine.transactions.len(), 105, "the repeated row once");
    assert_eq!((mine.total, mine.next), (106, 106));
    assert!(!mine.more());
    assert!(mine.text.starts_with(&first_text));
    assert!(
        mine.text.len() > first_text.len(),
        "both pages, one after the other"
    );
    let blocks: Vec<_> = mine.transactions.iter().map(|t| t.block).collect();
    assert!(
        blocks.windows(2).all(|w| w[0] > w[1]),
        "newest first, each once: {blocks:?}"
    );

    // Locked, nothing is read.
    assert!(matches!(h.call(Command::Lock), Reply::Locked));
    assert_eq!(
        refusal(h.call(Command::OlderActivity(vec![(account0(), 100)]))).kind,
        RefusalKind::NotUnlocked
    );
}

#[test]
fn the_newest_blocks_are_read_down_from_the_tip_without_a_store() {
    use tawara_wallet_core::explorer::BlockKind;
    let mut h = Harness::with_node();
    h.chain.pseudo(997);
    match h.call(Command::Blocks) {
        Reply::Blocks(Ok(view)) => {
            assert_eq!(view.tip, 1_000);
            let indexes: Vec<u64> = view.blocks.iter().map(|b| b.index).collect();
            assert_eq!(indexes, vec![1_000, 999, 998, 997, 996, 995]);
            assert_eq!(view.blocks[0].time_ms, 1_000 * 60_000);
            assert_eq!(view.blocks[0].difficulty, Some(30));
            assert_eq!(view.blocks[1].difficulty, Some(34));
            let kinds: Vec<_> = view.blocks.iter().map(|b| b.kind).collect();
            let normal = Some(BlockKind::Normal);
            assert_eq!(
                kinds,
                vec![
                    normal,
                    normal,
                    normal,
                    Some(BlockKind::Pseudo),
                    normal,
                    normal
                ],
                "a block whose figures count no transactions is a pseudo-block"
            );
            assert!(view.text.contains("pseudo"), "{}", view.text);
        }
        other => panic!("expected the blocks, got {other:?}"),
    }
    // A block numbered at a multiple of 256 is a neogenesis block, whatever
    // its figures count.
    h.chain.set_tip(1_026);
    h.chain.pseudo(1_024);
    match h.call(Command::Blocks) {
        Reply::Blocks(Ok(view)) => {
            assert_eq!(view.blocks[2].index, 1_024);
            assert_eq!(view.blocks[2].kind, Some(BlockKind::Neogenesis));
        }
        other => panic!("expected the blocks, got {other:?}"),
    }
    assert_eq!(
        refusal(Harness::new().call(Command::Blocks)).kind,
        RefusalKind::NoNode
    );
}

#[test]
fn the_queue_is_counted_without_a_store() {
    let mut h = Harness::with_node();
    match h.call(Command::Mempool) {
        Reply::Mempool(Ok(view)) => {
            assert_eq!(view.waiting, 0, "Go's null is an empty queue");
            assert!(!view.text.is_empty());
        }
        other => panic!("expected the queue, got {other:?}"),
    }
    h.chain.queue(&[[1; 32], [2; 32], [3; 32]]);
    let calls = h.chain.calls();
    match h.call(Command::Mempool) {
        Reply::Mempool(Ok(view)) => assert_eq!(view.waiting, 3),
        other => panic!("expected the queue, got {other:?}"),
    }
    assert_eq!(
        h.chain.calls() - calls,
        1,
        "the ids are counted, and none is read whole"
    );
    h.chain.set_unreachable(true);
    match h.call(Command::Mempool) {
        Reply::Mempool(Err(refused)) => {
            assert_eq!(refused.index, None, "the queue is not the index")
        }
        other => panic!("expected the queue refused, got {other:?}"),
    }
    assert_eq!(
        refusal(Harness::new().call(Command::Mempool)).kind,
        RefusalKind::NoNode
    );
}

#[test]
fn a_review_reports_every_account_and_applies_it() {
    let scratch = Scratch::new("review");
    let (mut h, start) = funded(&scratch);
    h.chain.hold(tag(1), address(1, 0), 900);
    assert!(matches!(
        h.call(Command::Restore {
            account_index: 1,
            scan_to: None,
        }),
        Reply::Restored { ok: true, .. }
    ));
    let order: Vec<AccountId> = opened(h.call(Command::Refresh))
        .accounts
        .iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(order.len(), 2);

    // The chain moves account 0 three keys ahead: another wallet spent.
    h.chain.hold(tag(0), address(0, start + 3), FUNDS);
    let id = h.handle.send(Command::Review).expect("worker running");
    assert_eq!(h.wait_busy(id), Activity::AskingNode);
    let reports = match h.wait_for(id) {
        Reply::Reviewed(reports) => reports,
        other => panic!("expected the review, got {other:?}"),
    };
    let reported: Vec<AccountId> = reports.iter().map(|r| r.account).collect();
    assert_eq!(reported, order, "every account, in the store's order");
    let report = |id: AccountId| reports.iter().find(|r| r.account == id).expect("reported");
    let first = report(account0());
    assert!(
        matches!(
            first.state,
            AccountState::Diverged {
                kind: DivergenceKind::Ahead { gap: 3 },
                ..
            }
        ),
        "{:?}",
        first.state
    );
    assert!(!first.spendable);
    assert!(!first.text.is_empty());
    assert_eq!(
        report(AccountId::from_tag(tag(1))).state,
        AccountState::InSync { balance: 900 }
    );
    let view = opened(h.call(Command::Refresh));
    let row = view
        .accounts
        .iter()
        .find(|a| a.id == account0())
        .expect("held");
    assert!(matches!(row.state, AccountState::Diverged { .. }));

    let locked = refusal(Harness::new().call(Command::Review));
    assert_eq!(locked.kind, RefusalKind::NotUnlocked);
}

#[test]
fn derived_accounts_carry_the_number_the_command_line_names_them_by() {
    let scratch = Scratch::new("numbers");
    let (mut h, _) = funded(&scratch);
    h.chain.hold(tag(3), address(3, 0), 900);
    assert!(matches!(
        h.call(Command::Restore {
            account_index: 3,
            scan_to: None,
        }),
        Reply::Restored { ok: true, .. }
    ));
    let view = opened(h.call(Command::Refresh));
    let number = |id: AccountId| {
        view.accounts
            .iter()
            .find(|a| a.id == id)
            .expect("held")
            .number
    };
    assert_eq!(number(account0()), Some(0));
    assert_eq!(number(AccountId::from_tag(tag(3))), Some(3));
}
