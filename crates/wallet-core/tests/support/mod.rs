//! A scripted node and a harness for driving the worker.
//!
//! # The node is a `Transport`, not a mock of the wallet
//!
//! The worker is handed a [`FakeNode`] in place of the HTTPS one, and the
//! real `MeshClient` and the real codec run on top of it, so every answer the
//! worker sees was parsed by the library's own parser. Only the bytes on the
//! wire are scripted here, in the shapes the library's own test chain uses
//! (its `tests/support/chain.rs`, after the live captures in
//! `fixtures/group_n_mesh_live.json`): `tag_resolve` answers with
//! `result.{address,amount}` or the middleware's code 4 "Account not found",
//! `/network/status` with `current_block_identifier`, and
//! `/construction/submit` with `transaction_identifier.hash` in bare hex.
//!
//! The submit echo is computed the way the library's own `submit` verb
//! computes the id it expects, `tx::wire::Transaction::from_wire(..).id_digest()`:
//! a node computes it from the bytes it was sent, and so does this one.
//!
//! # Where the addresses come from
//!
//! The library: an account's tag is `derive::derive_account_tag(master, n)`
//! and its address at a position is `recon::derived_address_at`, the restore
//! path's derivation. The master seed is the one the BIP39 test phrase below
//! derives through `mnemonic::master_seed_from_phrase`, which is what
//! `cli::create::create` does with a phrase. That phrase is a public test
//! vector and must never be funded.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use mochimo_crypto::account::WotsIndex;
use mochimo_crypto::addr::{Address, Tag};
use mochimo_crypto::consts::SEED_LEN;
use mochimo_crypto::mesh::Transport;
use mochimo_crypto::{Error, Secret, TransportKind, derive, mnemonic, recon};
use tawara_wallet_core::{
    Activity, Command, Config, Connect, Event, LockReason, Reply, RequestId, SecretText,
    WorkerHandle, spawn,
};

/// The BIP39 all-`abandon` 24-word test vector. Public; never fund it.
pub const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                          abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                          abandon abandon abandon abandon abandon art";

/// A password over the library's floor.
pub const PASSWORD: &str = "correct horse battery";

pub const NODE: &str = "https://node.test";

/// A second node, answered by its own chain ([`Harness::chain_b`]), so a
/// test can tell which node the worker asked.
pub const NODE_B: &str = "https://other-node.test";

pub fn master() -> Secret<SEED_LEN> {
    mnemonic::master_seed_from_phrase(PHRASE, "").expect("the test phrase parses")
}

/// Derived account `n`'s tag.
pub fn tag(n: u32) -> Tag {
    derive::derive_account_tag(&master(), n)
}

/// Derived account `n`'s address at position `i`.
pub fn address(n: u32, i: u32) -> Address {
    recon::derived_address_at(&master(), n, position(i))
}

pub fn position(i: u32) -> WotsIndex {
    let mut p = WotsIndex::ZERO;
    for _ in 0..i {
        p = p.advanced().expect("position");
    }
    p
}

pub fn destination(n: u32) -> String {
    mochimo_crypto::addr::tag_to_base58(&tag(n)).expect("renders")
}

pub fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[derive(Default)]
struct State {
    ledger: BTreeMap<Tag, (Address, u64)>,
    tip: u64,
    unreachable: bool,
    refuse_submits: bool,
    submits: Vec<Vec<u8>>,
    calls: usize,
    gate: Option<Gate>,
    delay: Duration,
    /// How many times each tag was resolved.
    resolved: BTreeMap<Tag, usize>,
    /// From this resolve of this tag on, answer "account not found".
    vanish: Option<(Tag, usize)>,
    /// The index's rows for each tag, newest first, as the endpoint spells
    /// them.
    history: BTreeMap<Tag, Vec<serde_json::Value>>,
    /// The deployment runs no index: a search answers its internal error.
    no_index: bool,
    /// How many transactions the index has recorded, for their ids.
    indexed: u64,
}

/// Holds every request at the chain until it is opened, so a test can act
/// while the worker is waiting on the node.
#[derive(Clone, Default)]
pub struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    pub fn open(&self) {
        let (open, changed) = &*self.0;
        *open.lock().expect("gate") = true;
        changed.notify_all();
    }

    fn pass(&self) {
        let (open, changed) = &*self.0;
        let mut is_open = open.lock().expect("gate");
        while !*is_open {
            is_open = changed.wait(is_open).expect("gate");
        }
    }
}

/// The scripted chain, shared between the test and every transport the
/// worker builds from it.
#[derive(Clone, Default)]
pub struct Chain(Arc<Mutex<State>>);

impl Chain {
    pub fn new() -> Chain {
        let chain = Chain::default();
        chain.0.lock().expect("chain").tip = 1_000;
        chain
    }

    /// The ledger holds `tag` at `address` with `balance`.
    pub fn hold(&self, tag: Tag, address: Address, balance: u64) {
        self.0
            .lock()
            .expect("chain")
            .ledger
            .insert(tag, (address, balance));
    }

    /// The ledger has no entry for `tag`.
    pub fn forget(&self, tag: Tag) {
        self.0.lock().expect("chain").ledger.remove(&tag);
    }

    pub fn set_unreachable(&self, unreachable: bool) {
        self.0.lock().expect("chain").unreachable = unreachable;
    }

    pub fn refuse_submits(&self, refuse: bool) {
        self.0.lock().expect("chain").refuse_submits = refuse;
    }

    /// Every wire image posted to `/construction/submit`.
    pub fn submits(&self) -> Vec<Vec<u8>> {
        self.0.lock().expect("chain").submits.clone()
    }

    pub fn calls(&self) -> usize {
        self.0.lock().expect("chain").calls
    }

    /// How many times `tag` has been resolved.
    pub fn resolves(&self, tag: Tag) -> usize {
        self.0
            .lock()
            .expect("chain")
            .resolved
            .get(&tag)
            .copied()
            .unwrap_or(0)
    }

    /// Answer "account not found" for `tag` from its `nth` resolve on
    /// (counted as [`Chain::resolves`] counts), whatever the ledger holds:
    /// the node changing its answer partway through a command.
    pub fn vanish_from(&self, tag: Tag, nth: usize) {
        self.0.lock().expect("chain").vanish = Some((tag, nth));
    }

    /// Take `delay` to answer every request from now on, as a slow node does.
    pub fn slow(&self, delay: Duration) {
        self.0.lock().expect("chain").delay = delay;
    }

    /// The index records a transfer of `amount` from `from` to `to` in
    /// `block`, as the index spells one: the source debited gross, the
    /// change back to it as a destination of its own, and the fee.
    pub fn index_transfer(&self, from: Tag, to: Tag, amount: u64, change: u64, block: u64) {
        let mut s = self.0.lock().expect("chain");
        s.indexed += 1;
        let fee = 500;
        let op = |i: u64, kind: &str, tag: &Tag, value: i128| {
            serde_json::json!({
                "operation_identifier": { "index": i },
                "type": kind,
                "account": { "address": format!("0x{}", hex_of(tag)) },
                "amount": { "value": value.to_string() },
            })
        };
        let gross = i128::from(amount + change + fee);
        let row = serde_json::json!({
            "transaction_identifier": { "hash": format!("0x{:064x}", s.indexed) },
            "block_identifier": { "index": block, "hash": format!("0x{:064x}", block) },
            "timestamp": block * 60_000,
            "operations": [
                op(0, "SOURCE_TRANSFER", &from, -gross),
                op(1, "DESTINATION_TRANSFER", &to, i128::from(amount)),
                op(2, "DESTINATION_TRANSFER", &from, i128::from(change)),
                {
                    "operation_identifier": { "index": 3 },
                    "type": "FEE",
                    "account": { "address": "" },
                    "amount": { "value": fee.to_string() },
                },
            ],
        });
        for tag in [from, to] {
            s.history.entry(tag).or_default().insert(0, row.clone());
        }
    }

    pub fn set_no_index(&self, no_index: bool) {
        self.0.lock().expect("chain").no_index = no_index;
    }

    /// Hold every request from now on until the returned gate is opened.
    pub fn close_gate(&self) -> Gate {
        let gate = Gate::default();
        self.0.lock().expect("chain").gate = Some(gate.clone());
        gate
    }
}

fn mesh(what: &'static str) -> Error {
    Error::MeshResponse { what }
}

impl Transport for Chain {
    fn post(&self, path: &str, body: &[u8]) -> mochimo_crypto::Result<Vec<u8>> {
        // Waited at with the chain's lock released, so the test can script
        // the chain meanwhile.
        let (gate, delay) = {
            let s = self.0.lock().map_err(|_| mesh("test: chain lock"))?;
            (s.gate.clone(), s.delay)
        };
        if let Some(gate) = gate {
            gate.pass();
        }
        std::thread::sleep(delay);
        let mut s = self.0.lock().map_err(|_| mesh("test: chain lock"))?;
        s.calls += 1;
        if s.unreachable {
            return Err(Error::Transport {
                op: "connect",
                kind: TransportKind::Io(std::io::ErrorKind::ConnectionRefused),
            });
        }
        let req: serde_json::Value =
            serde_json::from_slice(body).map_err(|_| mesh("test: request json"))?;
        match path {
            "/network/status" => Ok(format!(
                r#"{{"current_block_identifier":{{"index":{},"hash":"0x{}"}},"sync_status":{{"stage":"synchronized","synced":true}}}}"#,
                s.tip,
                "ab".repeat(32)
            )
            .into_bytes()),
            "/call" => {
                let asked = req["parameters"]["tag"].as_str().ok_or(mesh("test: parameters.tag"))?;
                let raw = asked
                    .strip_prefix("0x")
                    .and_then(unhex)
                    .ok_or(mesh("test: tag hex"))?;
                let tag: Tag = raw.try_into().map_err(|_| mesh("test: tag length"))?;
                let nth = {
                    let count = s.resolved.entry(tag).or_default();
                    *count += 1;
                    *count
                };
                let vanished = matches!(s.vanish, Some((t, from)) if t == tag && nth >= from);
                match s.ledger.get(&tag).filter(|_| !vanished) {
                    Some((address, balance)) => Ok(format!(
                        r#"{{"result":{{"address":"0x{}","amount":{balance}}},"idempotent":true}}"#,
                        hex_of(address)
                    )
                    .into_bytes()),
                    None => Ok(br#"{"code":4,"message":"Account not found","retriable":false}"#.to_vec()),
                }
            }
            "/search/transactions" => {
                if s.no_index {
                    return Ok(br#"{"code":1,"message":"Internal error","retriable":false}"#.to_vec());
                }
                let asked = req["account_identifier"]["address"].as_str().ok_or(mesh("test: account_identifier"))?;
                let limit = req["limit"].as_u64().filter(|l| (1..=100).contains(l)).unwrap_or(10);
                let raw = asked.strip_prefix("0x").and_then(unhex).ok_or(mesh("test: tag hex"))?;
                let tag: Tag = raw.try_into().map_err(|_| mesh("test: tag length"))?;
                let rows = s.history.get(&tag).cloned().unwrap_or_default();
                let total = rows.len();
                let page: Vec<_> = rows.into_iter().take(usize::try_from(limit).unwrap_or(10)).collect();
                Ok(serde_json::json!({ "transactions": page, "total_count": total }).to_string().into_bytes())
            }
            "/block" => {
                let index = req["block_identifier"]["index"].as_u64().ok_or(mesh("test: block index"))?;
                // Index 0 is the tip to this endpoint, as the library notes.
                let index = if index == 0 { s.tip } else { index };
                if index > s.tip {
                    return Ok(br#"{"code":2,"message":"Block not found","retriable":false}"#.to_vec());
                }
                Ok(serde_json::json!({ "block": {
                    "block_identifier": { "index": index, "hash": format!("0x{index:064x}") },
                    "parent_block_identifier": { "index": index - 1, "hash": format!("0x{:064x}", index - 1) },
                    "timestamp": index * 60_000,
                    "transactions": [],
                }})
                .to_string()
                .into_bytes())
            }
            "/construction/submit" => {
                let text = req["signed_transaction"].as_str().ok_or(mesh("test: signed_transaction"))?;
                let wire = unhex(text).ok_or(mesh("test: wire hex"))?;
                s.submits.push(wire.clone());
                if s.refuse_submits {
                    return Err(Error::Transport {
                        op: "write",
                        kind: TransportKind::Io(std::io::ErrorKind::BrokenPipe),
                    });
                }
                let tx = mochimo_crypto::tx::wire::Transaction::from_wire(&wire)?;
                Ok(format!(r#"{{"transaction_identifier":{{"hash":"{}"}}}}"#, hex_of(&tx.id_digest())).into_bytes())
            }
            _ => Err(mesh("test: a path this node does not script")),
        }
    }
}

/// The worker's [`Connect`] for the scripted chain.
/// The worker's [`Connect`]: [`NODE_B`] is answered by the second chain,
/// any other URL by the first.
pub struct FakeNode {
    pub chain: Chain,
    pub chain_b: Chain,
}

impl Connect for FakeNode {
    type Transport = Chain;

    fn connect(&self, url: &str) -> Result<Chain, Error> {
        Ok(if url == NODE_B {
            self.chain_b.clone()
        } else {
            self.chain.clone()
        })
    }
}

/// A directory to put stores under, removed on drop. It is under cargo's
/// per-target scratch directory, as the library's own tests keep theirs, and
/// the store's folder inside it is left for the library to make, with the
/// owner and access it requires.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Scratch {
        // Unique per run as well as per test: on Windows a store the worker
        // has not yet released cannot be removed, and must not be found by
        // the next run.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("wallet-core-{}-{name}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        Scratch(dir)
    }

    pub fn store(&self) -> PathBuf {
        self.0.join("keystore")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn secret(text: &str) -> SecretText {
    let mut field = text.to_owned();
    SecretText::take(&mut field)
}

/// The worker, its events, and a scripted chain.
pub struct Harness {
    pub handle: WorkerHandle,
    pub events: Receiver<Event>,
    pub chain: Chain,
    /// The chain behind [`NODE_B`].
    pub chain_b: Chain,
    /// Every event that was not the answer being waited for.
    pub seen: Vec<Event>,
}

const WAIT: Duration = Duration::from_secs(120);

impl Harness {
    pub fn new() -> Harness {
        Harness::with(Config::default())
    }

    pub fn with(config: Config) -> Harness {
        let chain = Chain::new();
        let chain_b = Chain::new();
        let node = FakeNode {
            chain: chain.clone(),
            chain_b: chain_b.clone(),
        };
        let (handle, events) = spawn(config, node).expect("worker starts");
        Harness {
            handle,
            events,
            chain,
            chain_b,
            seen: Vec::new(),
        }
    }

    /// A harness with the node chosen.
    pub fn with_node() -> Harness {
        let mut h = Harness::new();
        match h.call(Command::SetNode { url: NODE.into() }) {
            Reply::NodeSet { .. } => {}
            other => panic!("node not set: {other:?}"),
        }
        h
    }

    /// Send a command and wait for its answer.
    pub fn call(&mut self, command: Command) -> Reply {
        let id = self.handle.send(command).expect("worker running");
        self.wait_for(id)
    }

    /// Wait for the answer to a command sent with `handle.send`.
    pub fn wait_for(&mut self, id: RequestId) -> Reply {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(Event::Done { id: got, reply }) if got == id => return reply,
                Ok(other) => self.seen.push(other),
                Err(e) => panic!("no answer to {id:?}: {e}"),
            }
        }
    }

    /// Wait until command `id` reports what it is doing.
    pub fn wait_busy(&mut self, id: RequestId) -> Activity {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(Event::Busy { id: got, activity }) if got == id => return activity,
                Ok(other) => self.seen.push(other),
                Err(e) => panic!("{id:?} never reported busy: {e}"),
            }
        }
    }

    /// Wait for the next `Locked` event, keeping the rest.
    pub fn wait_locked(&mut self, within: Duration) -> LockReason {
        if let Some(i) = self
            .seen
            .iter()
            .position(|e| matches!(e, Event::Locked { .. }))
            && let Event::Locked { reason } = self.seen.remove(i)
        {
            return reason;
        }
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(Event::Locked { reason }) => return reason,
                Ok(other) => self.seen.push(other),
                Err(e) => panic!("no Locked event: {e}"),
            }
        }
    }

    /// Make a store from the test phrase in `dir` and open it.
    pub fn create_from_phrase(&mut self, dir: &Path) -> Reply {
        self.call(Command::CreateFromPhrase {
            dir: dir.to_path_buf(),
            password: secret(PASSWORD),
            password_again: secret(PASSWORD),
            phrase: secret(PHRASE),
        })
    }

    pub fn unlock(&mut self, dir: &Path, password: &str) -> Reply {
        self.call(Command::Unlock {
            dir: dir.to_path_buf(),
            password: secret(password),
        })
    }
}
