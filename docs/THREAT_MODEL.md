# Threat model

docs/PLAN.md section 4 states the threat model in nine rules, and section 3
names this file as its full write-up, required before any release (phase 5).
This is that write-up. Decided under the owner's delegation on 2026-10-12
(docs/DECISIONS.md D34), for the owner to revisit.

It was written against these commits, and its line numbers are theirs:

| what | commit |
|---|---|
| this repository, `main` | `389f7317578dd1b28a0127d3e7b2bb87e2725d16` |
| Tawara-mobile, `main` | `587329c7bf774129d59e617cef0f6b0a7e91c51e` |
| the wallet library, `mochimo-crypto` (Rep-1) | `613e8bd7d3379a3d1cb88803319931f817a48255` (D7) |
| the iced fork, release `v0.14.1-mobile.2` | `d4ab3a057db96162b087a27dd0752a3b15abfa6b` (D32 item 9 (a)) |
| this repository's PR #26 (`work/decisions-2026-10-13`, merged as `79cae88`); D44 as cited here | `0c05b9ef8faf6ed8786005a5d194df565ff3bc5b` |

*Note (2026-10-17, D32 item 9 (a)): the iced pin has moved since these commits, to the fork's release `v0.14.1-mobile.4`, `7325ccd5823a118d46f023e91010d27f07b6dd90`. The table above and the line numbers below are as they were at these commits, and are not updated. At the new pin mundy and zbus have left `Cargo.lock`; the pin is still one fork tag, one winit and `default-features = false` (T39); and the two `deny.toml` ignores still apply (T37).*

References: "D*n*" is an entry of docs/DECISIONS.md; "PLAN 4.*n*" is
docs/PLAN.md section 4, rule *n*; a path without a repository is in this
one; `Tawara-mobile:` paths are in that repository at the commit above; the
library's invariants (I1, I2, I4 and so on) are those of its
`docs/specification.md` at its pin; `fork:` paths are in the iced fork at its
pin.

**Nothing here is a new control.** Every protection named below is in the
source or the records, with its place. A threat with no protection, or a
protection with no check, is a gap, listed in section 5 with who closes it.

**Status**, in section 3 and after:

- **verified**: a named test, or a CI check, exercises the control. Where
  that check runs only on the Android emulator, the iOS simulator or a
  desktop host, the row says so. No check of any kind has yet been run on a
  physical phone.
- **source only**: the control is in the source and was read, but nothing
  exercises it.
- **owed**: the control, or the check it needs, does not exist yet.

The **residual** column says what is left when every control holds, and
whether the owner has accepted it. "For the owner" means nobody has, and
section 6 asks for it.

## 1. Scope and assets

What Tawara protects, in order of what its loss costs:

| asset | where it lives | what its loss or misuse costs |
|---|---|---|
| **The recovery phrase and the master seed** | The phrase exists only while a new store is made (S4, S5) or restored (S6): in the worker's `PendingCreate` for a new store (`crates/wallet-core/src/worker.rs:1442-1461`), in `PhraseForDisplay` for display (`crates/wallet-core/src/secret.rs:81-127`), and, for a restore, only as the `SecretText` argument of `create_from_phrase` (`crates/wallet-core/src/worker.rs:1537-1544`). The master seed is in the open store and in the worker's session copy (D19). | Everything in every account derived from it, for good. |
| **The one-time keys and the key index** | WOTS+ keys are derived from the seed, one position at a time. The index of the next unused key is in the store. | A key that signs twice leaks enough to forge (I1). The index's integrity is therefore an asset in itself: whatever rolls it back, or lets a second copy of it run, leads to key reuse (PLAN 4.5). |
| **The store** | `keystore/` in the default or chosen folder (D21; D32 items 7 and 10 (a) on phones): the header, encrypted records, the lock file. | Encrypted under the password (Argon2id, then ChaCha20-Poly1305). A copy can be attacked offline by guessing the password, and an old copy is a rollback (above). |
| **The store password** | Typed on S3, S6 and S7, held by the worker only for the command that brought it (D19). Where the device unlock is offered (iOS, D45), a copy is held by the interface from a password unlock until the offer is answered, or the store locks, or the application leaves the foreground. A kept password is in the system's secure storage only once the person accepts, and is released only by the device's check. That check is asked on S7 once when the application comes to the foreground, and again only when its button is pressed; a Face ID prompt makes the application inactive, which asks nothing. Offering the unlock again in Settings keeps no password. Anyone who knows the phone's passcode can open the wallet with the kept password (D45 items 2 and 4). | With a copy of the store, the seed. |
| **Typed secrets** | The password, the phrase and the confirmation words in the interface's form fields while they are typed (`crates/app/src/app.rs:234-238, 305-328`), then `SecretText` (`secret.rs:32-42`). A phrase pasted is also on the system clipboard. | As the password or the seed. |
| **The signed transaction (the retry artifact)** | Returned by the worker once a spend is signed and submitted, kept in the interface for the run (`Model::signed`, docs/SCREENS.md W6), saved by the person to a file or a share sheet, or copied as hex. The library keeps no copy of it. | Not secret (D27 item 5): it authorises only the spend already made, and it is what the node was sent. Losing it can strand funds after a reverted settle (T9). Anyone who holds it can submit that same spend, and it shows its payees and amounts before the chain does. |
| **Addresses, balances and history** | On screen; asked of the node; the node's index. The preferences file holds the node's URL, the store's folder, and whether the device unlock is on or was offered (`crates/wallet-core/src/preferences.rs`). The store identifier, `store-id` in the same folder, is 32 random bytes that name no secret and open nothing (`crates/wallet-core/src/store_identifier.rs`; D45 item 4). | Privacy: who pays whom, how much is held, and that the accounts of one store belong together. |
| **The node connection** | TLS to the node the person chose (D22). | The node's answers drive reconciliation, the amounts a spend is built from, settling and every balance shown. |
| **The build** | The library and iced fork pins, the lockfile, the toolchain pin, CI. | A changed dependency runs with the seed in reach. |

Out of scope: the library's cryptography and its own proofs (its
`docs/specification.md`; this file relies on its invariants and says where
it does); the node's own security and the Mochimo network's consensus; and a
person who gives their phrase or password away knowingly. Tawara-mobile is
in scope: it holds the phone shells and their platform controls, and its
records live here (D33 item 4).

## 2. Adversaries and trust boundaries

### Trust boundaries

1. **The worker and the interface.** One thread, `tawara-wallet-core`
   (`crates/wallet-core/src/worker.rs:383-385`), owns the `Keystore` and the
   `Wallet`. The interface sends commands and receives events; events carry
   no secret except a new phrase (D19). `crates/app` and `crates/desktop`
   never name the library (`crates/wallet-core/tests/policy.rs:2413`
   `wallet_core_and_the_interface_stay_apart`).
2. **The process and its platform.** The operating system, other users,
   other apps, the clipboard, the input method, accessibility services,
   screen capture, backups and sync. The store's file checks are the
   library's (PLAN 4.6).
3. **The wallet and the node.** TLS authenticates the node's name, not what
   it says. The node is trusted for chain facts: balances, the tip, which
   address an account is at, whether a spend landed (I4: "That agreement
   depends on trusting the configured node").
4. **The application and the library.** The library is trusted, pinned by
   full hash and never patched (D1, D7, D15). Its protective text is its
   own (PLAN section 3).
5. **The source and the build.** Dependencies, the iced fork, the toolchain,
   the CI runners, and (from phase 5) packaging and signing.
6. **Tawara and the person.** The person decides whom to trust as a node,
   where to keep the store, what to type, and whether to read a refusal.
7. **The camera and the codes it reads.** A code is text that anyone can print
   or place over another one, so what the camera reads is untrusted until it is
   checked as a destination (D42 item 4). The Android preview holds the latest
   frame only while a scan draws it, and the frame goes with the scan.

### Adversaries

| # | adversary | can | wants |
|---|---|---|---|
| A1 | **A thief with an unlocked device**, or anyone at an unattended desktop | use the open wallet until it locks; read the screen | to spend; to see the phrase |
| A2 | **A thief with a locked device**, a lost laptop, a disposed disk | read the disk; on a phone, only what the platform's data protection leaves readable | to copy the store and guess the password |
| A3 | **Malware on the desktop**, running as the same user | read the store's files, the clipboard and the screen; log keys; read process memory | the password, the phrase, a swapped destination |
| A4 | **Another app on a phone** | read the clipboard within the platform's limits; use accessibility services if granted; receive shared text | the phrase, the password, the artifact, addresses |
| A5 | **Shoulder surfing and screen capture**: a person nearby, a recording, screen sharing, the app switcher's snapshot, a phone's recents thumbnail | see what is drawn | the phrase; balances and addresses |
| A6 | **Backups and cloud sync** (not an adversary, a mechanism) | keep and restore older copies of the store | (it rolls back the key index) |
| A7 | **A malicious or broken node**, or a network attacker in front of one | answer anything; refuse; delay; learn every tag asked about and the asker's address | to mislead the person, freeze accounts, link accounts, hide a rollback |
| A8 | **A reorg** | revert a block a settle relied on | (it leaves a store that recorded a settle the chain no longer shows) |
| A9 | **The supply chain**: a crate, the iced fork, the library's repository, the toolchain, CI runners, packaging | put code in the build | the seed, the password, a changed destination |
| A10 | **The person's own mistakes** | lose the phrase, run two wallets on one phrase, paste the wrong destination, press on past a warning | (loss of funds by accident) |
| A11 | **Whoever prints or places the code the camera reads** | make a code hold any text, a destination or not, and put it where the person scans | a payment to an account of its own, in place of the one the payee gave |

## 3. Threats, protections and status

Grouped by what is at stake. Each row: the threat; what protects against it,
with its source; what checks that protection; what is left; and the status.

### 3.1 Key reuse: the hazard everything else serves

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T1 | **A key signs twice within one store**: a bug, a repeated click, a spend queued behind a lock, a plan used twice. | The library: only two routes produce a signature; `sign_spend` consumes a receipt that exists only after the advanced index is durable (I1, I2); its raw signer is crate-private and `raw-backend` is denied in the graph (`deny.toml:259-268`) and kept off by the feature pin (D15). The worker: one pending plan, taken by `confirm_send` and checked by id (`worker.rs:1930-1994`); a spend is never stopped between its reservation and its submission, and a spend still queued behind a cancel, a move to the background or the last handle's drop does not sign (D19, D24). Re-signing reproduces the reserved bytes and nothing else (I1; D19). | Library suites at `7cdc2e9`, run on macOS (Darwin 27.0.0, Rust 1.98.0) on 2026-10-13 with `cargo test -p mochimo-crypto --locked --test invariants --test keystore --test recon --test signing --test spend --test kat`: invariants 70, keystore 38, recon 45, signing 17, spend 20, kat 18, all passed (`signature_is_not_released_before_the_index_is_durable` among them). The library's record at `2f57e5a` (`RELEASE.md:302-312`) is an earlier commit, with recon 38 and spend 19. `tests/worker.rs:568` `resigning_reproduces_the_reserved_bytes_and_nothing_else`; `:1112`, `:1159`, `:1738` (cancel, background and drop stop a queued spend); `policy.rs:1724` `library_is_pinned_by_full_commit_hash_and_never_patched`; `cargo deny` in CI. | I2's crash model is a kill at a syscall boundary. Power loss, an `fsync` that reports success without flushing, and page loss are outside it (I2, "Limit, stated"). A phone whose battery dies mid-commit is in that class. | verified (library and desktop hosts); on a phone, owed (G15) |
| T2 | **A restored older copy of the store** rolls back the index and any open reservation (desktop backups: Time Machine, File History, shadow copies). | Every account is reconciled against the chain before anything signs; an account the chain has moved past is set aside, and nothing signs from it until the acknowledged advance (I4; D19; D21). | `tests/worker.rs:682` `a_diverged_store_is_reopened_and_advanced_on_acknowledgement`; `:1667` `an_account_set_aside_at_open_settles_and_rejoins_once_it_reconciles`. | A copy taken before a spend was reserved, restored while that spend has not landed, reconciles as in sync, and a new spend signs with the key the lost reservation used. **Accepted by the owner, 2026-10-04 (D21).** No user-facing note about backup tools exists (G20). | verified |
| T3 | **Backups and transfers on a phone** copy the store to another device or to the cloud. | Android: the store is in `no_backup/` (D21), with `android:allowBackup="false"` and `dataExtractionRules` excluding every domain from cloud backup and device transfer (`Tawara-mobile:platform/android/AndroidManifest.xml:26-27`; `platform/android/res/xml/data_extraction_rules.xml:8-29`). iOS: `NSURLIsExcludedFromBackupKey` on the store's folder (`Tawara-mobile:crates/mobile/src/ios.rs:121-132`; D32 item 3). The store can live nowhere else on a phone (T24). | Tawara-mobile CI: the manifest's `allowBackup` and that the attribute names the rules (`platform/android/tawara.sh:256-261`), `no_backup` mode `drwx------` (`:296-305`), the folder's backup xattr on the simulator (`platform/ios/tawara.sh:175-176`); all passed on 587329c. | The rules' contents are checked by no script. Whether iCloud and device backups actually skip the folder is not observed. | verified (emulator and simulator, attributes only); effect on a device, owed (G16; V11) |
| T4 | **The store in a cloud-sync folder on the desktop**, which copies and may restore it. | The default folders are outside sync folders: `%LOCALAPPDATA%` (never Roaming), `~/Library/Application Support`, `$XDG_DATA_HOME` (D21; `crates/wallet-core/src/location.rs:190-217`). A chosen folder inside iCloud Drive, a macOS `CloudStorage` provider, OneDrive, Dropbox, Google Drive, Box, pCloud, Nextcloud, ownCloud, MEGA, Synology Drive, Seafile or a roaming profile raises a warning, on the path as given and with its links resolved (`location.rs:392-404`). It warns and does not refuse (D21). | `location.rs:585`, `:606`, `:615` (defaults); `:715`, `:738`, `:765`, `:863` (warnings). | A person may keep the folder after the warning. A provider not on the list is not seen. The named tests do not cover every provider on the list. | verified (in part) |
| T5 | **Two live wallets on one phrase**: a phone and a desktop restored from the same phrase, or a copied store folder. Each signs the same position before either spend lands. | None across stores: the library's I1 is bounded to one store and says so ("Two stores over one seed ... nothing in this invariant sees it"). Before a divergence, the application warns only on the account-recovery screen (`crates/app/src/screens/wallet/recovery.rs:36-41`), and asks there that the person confirm no other wallet uses the phrase (D19). After one, its divergence summaries name a second wallet on the same phrase as a cause (`crates/app/src/screens/wallet/report.rs:191-192`, `:240`, `:243`). | `crates/app/src/app.rs:4849` (the confirmation on W12). | Key reuse. Nothing at S4 or S6, and nothing before a divergence outside W12, says that one phrase must not be live in two wallets. | owed (G1) |
| T6 | **The acknowledged advance misused**: pressed to make a refusal go away, or made while a second wallet is live, which destroys the keys it skips. | PLAN 4.9; the owner's terms (D19): its own screen (W12), reached on purpose and never from a refusal; every account's report first, every full report opened before the advance is offered (D29 item 5); the index typed, nothing filled in; a confirmation that no other wallet uses the phrase. The library advances only to the index its live report names, never backwards (I4). The renderings' "Advance to #33" and "Reconcile" are replaced by links to W12 (D27 item 3); "Show recovery phrase" and "Export keystore" are not built (D27 item 3). | `app.rs:4849` `recovery_shows_every_report_whole_and_advances_only_as_typed_and_confirmed`; `tests/worker.rs:1994` `a_cancelled_advance_moves_no_key_index`; `:1805`. | The rule that no button anywhere moves an index rests on design and review; no automated check finds a new one. | verified (W12); the rule as a whole, source only |
| T7 | **An interrupted send**: the app ended, killed or out of background time between the reservation and the submission. | The index is durable before a signature exists (I2); the reservation stays open, and the spend can be re-signed byte for byte (W8) or its saved bytes submitted (W9). The iOS shell ends the process rather than be suspended holding the key (D32 item 6). The sent page is kept for the run however it is left (docs/SCREENS.md W6). | Library: `index_is_durable_before_the_receipt_under_syscall_kill_not_power_loss`; `tests/worker.rs:568`; `app.rs:4000` `a_signed_spend_left_any_way_is_kept_offered_and_resigned_from`. | Power loss, as T1. The records disagree on what an interrupted send can leave (section 5.3, C1). | verified (kill at a syscall boundary); a kill by the system on a phone, owed (V13) |
| T8 | **Settling too early or on a lying node's word.** | Settling takes one observation and no confirmation depth, by the library's design: too early costs one skipped key and creates no reuse; too late freezes the account (library specification, "Settling"). | Library recon and spend suites. | A skipped key; an account frozen until the chain shows the spend. | verified (library) |
| T9 | **A reorg reverts a block a settle relied on.** | The library reports the account "behind", with the retained block and every cause that fits; the account is refused (library specification, "The reverted-settle report"). The saved artifact can be sent again from W7's Submit saved artifact, which opens W9 in its resubmit mode (`Command::ResubmitSaved`), only while the source balance has not moved and block-to-live has not passed. The library's `recon::resubmit_reverted_settle` sends a copy only if its signed part hashes to the retained digest, its signature recovers to the source key, the chain shows that settle reverted and the balance and block-to-live still hold; it signs nothing and moves no key index. The button is drawn only for an account one key behind with its settle retained, in an open wallet (D38 item 5 (b)). | Library: `a_reverted_settle_is_reported_behind_with_the_retained_block_and_every_cause` (a scripted chain, not a live reorg). Wallet: `tests/worker.rs` `a_saved_copy_of_a_reverted_settle_is_sent_again_and_the_account_is_read_again` and the seven refusal tests beside it; `crates/app/src/screens/wallet/account.rs` `a_reverted_settle_offers_submit_saved_artifact_and_no_remedy`; `crates/app/src/app/wallet.rs` `w9_sends_a_saved_copy_of_a_reverted_settle_as_resubmit_and_the_rest_as_before`; `crates/app/src/screens/wallet/account.rs` `a_store_that_did_not_open_offers_no_saved_copy`; `crates/app/src/app.rs` `a_saved_copy_the_library_refuses_is_read_again_quietly`. | No command turns the retained block back into bytes: both re-signers read the open reservation, which is clear. Without the saved artifact, funds at that key wait on the chain. A refused copy's account is read again at once, quietly, and the row changes only when the chain shows the spend (D38 item 5 (b)). A store made from the same phrase elsewhere could advance to the reverted key's index and sign with that key, and neither store sees the other (T5; D38 item 5 (b)). **For the owner.** | verified (library and wallet, scripted; not a live reorg) |

### 3.2 Secrets in memory, in the interface and in logs

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T10 | **An unlocked wallet left unattended** (A1). | Idle lock after five minutes (`worker.rs:199` `DEFAULT_IDLE_LOCK`), configurable to 1, 2, 5, 10 or 15 minutes and never "never" (D24; D27 item 10), taking effect at once (D29 item 7). The period counts from the person's input only: key presses, mouse buttons, wheel scrolls and finger presses (`app.rs:2183-2207`); polls and timers never keep the store open, and the period is checked before each queued command (D24). Lock drops the `Keystore`, the session's seed copy, a waiting phrase and a pending plan (`worker.rs:1205-1215`). Explicit Lock in the sidebar and the compact bar (PLAN 4.1). | `tests/worker.rs:838` `an_idle_store_locks_itself`; `:850`; `:887` `touching_keeps_the_store_open_and_polling_does_not`; `:1545`; `:2061` `the_idle_period_stops_a_walk_to_a_far_index_and_locks`; `:1030`; `app.rs:4831`. | Up to the idle period, anyone at the device can spend: no password is asked before signing, since the library cannot check one against an open store; the confirmation is the library's "NOT SIGNED" page and a separate step (D27 item 3). The desktop does not lock when the window is minimised or loses focus (G9). The owner has not recorded acceptance of either. **For the owner.** The lock also waits for a node request in flight, up to the transport's timeout (D32 item 6). | verified |
| T11 | **The key kept in memory while a phone is in the background** (A1, A2). | Every move to the background locks the worker and wipes typed fields (D32 item 6; `WorkerHandle::background`). iOS: the shell holds a UIKit background task until the interface has taken the worker's answer to the move, and ends the process if the time runs out, if UIKit grants none, or if no thread can wait (`Tawara-mobile:crates/mobile/src/ios.rs:301-363`). Android: `Suspended` asks for the lock and does not wait; the bound was declined for now (D32 item 6; `Tawara-mobile:crates/mobile/src/android.rs:74-84`). | `app.rs:2614` `a_shell_hears_the_move_done_only_once_the_interface_has_wiped`; `:2666`; `:2681`; `tests/worker.rs:1030`, `:1159`. Tawara-mobile CI: `background.locks` (simulator), `home.locks` and `back.locks` (emulator), passed on 587329c; each waits for a log line. | Android may suspend the process before the lock has finished, with the key in memory; the records do not say what follows (G23). After `Destroy` the process does not wait for the lock. At 587329c it called `exit(0)` (`Tawara-mobile:crates/mobile/src/android.rs:46-50`); the 2026-10-13 amendment to D32 item 9 (j) replaced that with `libc::_exit`, after a SIGABRT was reproduced, and makes `exit.lock` a reported count, not a gate (G6). The phone check is owed (V4). No exit path on iOS is exercised by any check. | verified (simulator and emulator, log lines); on a device, owed |
| T12 | **Typed secrets left in the interface**: across a lock, a move to the background, a screen left, or the worker stopping. | `SecretText::take` moves a field's buffer into `Zeroizing<String>` and empties the field (`secret.rs:32-42`; D20). `wipe` takes and drops, so the bytes are zeroed (`app.rs:743`). `wipe_typed` lists every screen that holds a secret (`app.rs:759-778`) and runs when a screen is left and on every move to the background. Text from a field drawn before the latest move is refused and wiped (`Typed::drawn`, `app.rs:1315-1343`; D32 item 6). The stopped screen zeroes what the screen before it held (docs/SCREENS.md, "The wallet stopped"); the worker's queue is dropped before `Stopped` is sent (`worker.rs:429-450`). The copy held for the device unlock's offer is dropped by the same lock, move to the background and stop (`app.rs`, `release_device`; T43). | `secret.rs:134` `take_moves_the_buffer_and_empties_the_field`; `app.rs:2478`, `:2541`, `:2614`, `:2681`, `:3257`, `:3658` `a_stopped_worker_takes_the_secrets_on_the_screen_with_it`. | While a secret is typed it is a plain `String` in the form (`app.rs:234-238`, `:364`, `:372`, `:379`), and each keystroke delivers the whole value in a new `String` inside a message (`Typed`, `app.rs:407`, which is `Clone`). Only the final take is a move. D20 says the secret "is never copied on the way in" (`docs/DECISIONS.md:645-647`), which holds only for that take; PLAN 4.2 says the contents move into `Zeroizing` buffers "at once" (G4, C7). The forms, `Screen` and `Model` derive `Debug` over plaintext fields (`app.rs:46`, `:188`, `:234`, `:356`, `:369`, `:376`); nothing prints them today (G5). iced's field copies its value into a list of graphemes each time it is drawn, freed without zeroing (`fork:widget/src/text_input.rs:135-139`; `fork:widget/src/text_input/value.rs:8-19`), and the input method keeps what it keeps; neither is reached (`secret.rs:16-22`). | verified (take and wipe); typing-time copies, owed |
| T13 | **Memory read from outside**: swap, a core dump, a hibernation image, a debugger. | None. The library leaves memory unlocked and core dumps on, as a decision: the mitigations need `unsafe`, and an attacker who can read process memory can also read the store and wait for the password (library specification, "The process's memory is not locked"). Tawara adds nothing. | — | As stated. On a phone the platform's sandbox is the guard. **For the owner.** | owed (acceptance) |
| T14 | **Secrets in logs, panic text or on disk.** | `SecretText`, `PhraseForDisplay`, `Typed`, the paste read (`Clip`) and the worker's event wrapper print nothing of their text in `Debug` (`secret.rs:69-74`, `:123-127`; `app.rs:412-416` for `Typed`, `:433-437` for the worker's event wrapper, `:525-528` for `Clip`). No crate in the workspace uses a logging crate (`spikes/tawara-spike` does, outside the workspace); the desktop binary prints only a start-up error (`crates/desktop/src/main.rs:4-6`). The graph still has `log` and `tracing` (`Cargo.lock:1790`, `:3467`), whose output G13 lists. Preferences hold no secret, beside `keystore/`, never inside it (D27 item 10). Release builds unwind, so `Drop` scrubs still run on a panic (D11). | `secret.rs:146` `debug_shows_neither_text_nor_length`; `app.rs:5046` `a_secret_typed_is_never_in_a_message_log`; `:3292` `a_pasted_read_is_never_in_a_message_log`; `preferences.rs:206`, `:221`, `:244`; `policy.rs:2250` `release_profile_keeps_the_library_scrubs`. | The derived `Debug` of T12. Panic messages from the library or iced go to stderr, and their content is not controlled here. The phone shells log lifecycle lines to logcat and the system log (D32 item 9 (j)); no list of what is logged exists (G13). | verified (the types named) |
| T15 | **Weak or partial randomness.** | `getrandom` 0.2 reads the operating system's generator: `BCryptGenRandom`, the `getrandom` call with `/dev/urandom` behind it, `getentropy`, `CCRandomGenerateBytes` (D20). Creating takes three draws, opening one; a failed draw refuses with nothing created or opened (`crates/wallet-core/src/entropy.rs:51-70`). | `entropy.rs:77`, `:88`, `:96`. | None known. | verified |

### 3.3 The recovery phrase and the password on screen and at the keyboard

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T16 | **The phrase seen or captured while shown** (A5): a person nearby, a screenshot, a recording, screen sharing, the app switcher. | Shown once, at creation, behind a confirmation of three words (PLAN 4.3; docs/SCREENS.md S4, S5); never shown again, and no control exists to show it (D27 item 3). Android: `FLAG_SECURE` on the whole app, set before the first frame (D32 item 4; `Tawara-mobile:crates/mobile/src/android.rs:38`, in `main`). iOS: a cover with no wallet content on `WillResignActive`, over the key window, removed on `DidBecomeActive` (D32 item 6; `Tawara-mobile:crates/mobile/src/ios.rs`: `observe` registers them at `:333` and `:338`; `cover` is at `:369`, `uncover` at `:403`). With no key window no cover is added, and the log says so (`cover`, `ios.rs:384`). | Tawara-mobile CI: `flag_secure` reads `SECURE` from `dumpsys` (`platform/android/tawara.sh:711-715`); `background.covered` reads `cover added` in the log (`platform/ios/tawara.sh:256-257`), and `scene.window` reads the key window in a scene (`:262-263`). Passed on 587329c. | The desktop has no protection: screenshots, recording and screen sharing see S4 (G2). No check shows that an Android screenshot, recording or recents thumbnail is blank, or that the iOS switcher's snapshot is (G7, G8). The `scene.window` check (`platform/ios/tawara.sh:262-263`) catches a missing key window on the simulator only; nothing checks it on a phone. On an iPhone, while the app is active, the app does not stop a screenshot, a screen recording or mirroring of S4 (G27): the cover comes off on `DidBecomeActive` (`Tawara-mobile:crates/mobile/src/ios.rs:338`, `uncover` at `:403-409`), the iced fork's iOS `set_content_protected` is empty (`fork:vendor/winit/src/platform_impl/ios/window.rs:459`), and no code pinned before D44 item 3 reads `UIScreen.isCaptured`. V6 checks the switcher's snapshot only. The control is D44 item 3, merged with PR #26, which is a recommendation: on 2026-10-13 the owner decided only D44's frame order (item 1). Its app side is built (`crates/app/src/host.rs`, `crates/app/src/screens/first_run.rs`): the words are blanked while the shell reports the screen captured, as far as iOS reports it, and a warning shows after a screenshot taken while the words are drawn, which cannot block it; on a phone the warning takes the words' place until "Show the phrase" is pressed (D44 item 1, owner, 2026-10-16). Its images are `design/screenshots/phone-s4-phrase-photographed.png` and `design/screenshots/phone-s4-phrase-captured.png`. The iOS calls that report both are the shell's, in Tawara-mobile, and the control is not active on a phone until that pull request lands. D44 item 3 also records developers' reports that the capture state can read inactive while a recording runs, so the blanking is best effort, and V18 is the test. On Android, D44 item 4 records that Android's guidance says `FLAG_SECURE` "does not correctly predict if screen recording is active", and that on Android 11 (API 30) and lower it helps around 70% of devices reliably, so no text may say it stops every recording. | Android and iOS: verified (flag set, cover added); effect on a device, owed. Desktop: owed. iOS while active: owed (G27, V18) |
| T17 | **The input method learns or suggests a password or phrase** (A4). | iced marks a secure field's input purpose as secure (`fork:widget/src/text_input.rs:435-437`), which reaches winit as `ImePurpose::Password` (`fork:winit/src/conversion.rs:1440`). On Android winit drops it (`fork:vendor/winit/src/platform_impl/android/mod.rs:946`), and on iOS it is ignored with a warning (`fork:vendor/winit/src/platform_impl/ios/window.rs:386-387`). On Wayland it becomes a sensitive-data hint (`fork:vendor/winit/src/platform_impl/linux/wayland/seat/text_input/mod.rs:168`). | Phase 1 found an ordinary keyboard on both phones, which "may suggest and learn" (docs/spikes/P1-REPORT.md, "What did not, and where"). The owner's device check (D32 item 2) has no recorded result. | On both phones the keyboard is an ordinary one for the password and phrase fields. D32 item 2 makes native text-input glue part of phase 4 "if composition or secure entry fails" (`docs/DECISIONS.md:1461-1463`). Whether a keyboard that learns a password field counts as a failure of secure entry is not decided there; V1 reports it, and the owner decides. | owed (G3) |
| T18 | **Accessibility services read a secret field** (A4). | PLAN 4.2 requires that secret fields are not exposed as a value to accessibility services. No decision, mechanism or test says how. iced at this pin publishes no accessibility tree (no `accesskit` in the fork or in `Cargo.lock`), so a service likely sees no field at all; that is an inference, not a check. | — | Unknown until checked. The same inference means a screen reader cannot use Tawara. | owed (G10) |
| T19 | **Copy or cut of a secret field.** | iced refuses copy and cut in a `secure(true)` field (`fork:widget/src/text_input.rs:914-944`). The password, confirmation-word and restore-phrase fields are secure (`crates/app/src/screens/first_run.rs:449-470`, `:607-615`, `:706-711`, `:765-773`; `crates/app/src/ui.rs:298-316`). Paste into them is allowed (owner, D32 item 9 (g)). | No test in this repository. | — | source only (G11) |
| T20 | **The phrase on the clipboard.** | No copy control on S4 or S5 (PLAN 4.3; `first_run.rs:521`). | — | A phrase the person pastes into S6 stays on the system clipboard afterwards; Tawara wipes its own raw copy of the read, not the system clipboard, and puts the cleaned text in the field (`app.rs:539-557`, `:550-552`; `paste.rs:109-111`). | source only; the pasted phrase, owed (G12) |

### 3.4 The store at rest and on disk

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T21 | **A copied store attacked offline** (A2, A3). | Argon2id at the library's recommended cost, 64 MiB and three passes, one lane (`Kdf::RECOMMENDED`; `crates/wallet-core/src/lib.rs:138-141`); ChaCha20-Poly1305 with the header as associated data; a wrong password and any tampering are one refusal (library specification, store format). The test-only cheap KDF never appears outside test code (D15). Password floor of 12 characters, the library's (`MIN_PASSWORD_LEN`). | Library keystore suite; `policy.rs:1272` `cheap_kdf_appears_only_in_test_code`, `:1305`, `:1370`, `:1542`. | A weak password of 12 or more characters is guessable; no complexity rule. One lane, not RFC 9106's four, because the library builds Argon2 without threads; its cost was measured on a desktop, not a phone. | verified |
| T22 | **A thief reads a locked phone's storage** (A2). | iOS: `NSFileProtectionComplete` on the store's folder and what is in it when the app starts (D32 item 3; `Tawara-mobile:crates/mobile/src/ios.rs:84`, `:91-119`). Android: the app's private storage, under the system's file encryption. D32 item 3 gives the locked-phone guarantee to iOS only, so the record claims none for Android (G14). | Tawara-mobile CI: `store.protected` shows only that setting the attribute returned success (`platform/ios/tawara.sh:163-164`); the simulator does not enforce protection classes. | iOS protection is applied at launch only; files the library writes later in the run get no call of their own, and whether a new file takes its folder's class is not established. Nothing reads the class back, and Android's file encryption is not shown to keep the store unreadable on a locked phone (G14). The entitlement for a default protection class is phase 5 (`Tawara-mobile:platform/ios/tawara.sh:12-13`). | verified (the call returns success on the simulator, which enforces nothing); effect on a device, owed (V5) |
| T23 | **Other users or processes on the machine read or redirect the store** (A3). | The library's checks on every open and create: owner and mode on Unix (Android and iOS included), access lists on Windows, a linked store directory refused (PLAN 4.6). It makes the store folder `0700` and files `0600` or a private access list. The folders Tawara makes are `0700` on Unix (`worker.rs:918-936`; `preferences.rs:174-180`). | `tests/worker.rs:246` `a_store_the_library_will_not_write_leaves_its_phrase_waiting` (a `0777` folder refused); `:1594`; the library's keystore suite; phase 1 on the emulator and simulator (docs/spikes/P1-REPORT.md, "The library on a device"). | Malware running as the same user can read the store and log the password (A3); nothing on a desktop prevents it. The library on a real phone's file system is unchecked (G15). | verified (desktop hosts, emulator, simulator) |
| T24 | **A store placed outside the protected folder on a phone.** | No folder field on S3, S6 or S7 on a phone; the store is always `keystore/` in `Host::private_dir` (D32 item 10 (a)). A folder outside it is refused before anything is sent; `..` and links do not lead out (`app.rs:322-345`, `:975-991`; D32 item 7). | `app.rs:2771`, `:2801`, `:3319`, `:3339`, `:3371`. | Enforced in `crates/app` only; `wallet-core`'s commands accept any folder (G19). | verified |
| T25 | **Two writers on one store**: two windows, or Tawara and the command line. | The store's lock is held while the wallet is unlocked; the library's refusal is shown on S7 (PLAN 4.7). | `tests/worker.rs:307` `unlocking_refuses_a_wrong_password_a_missing_store_and_a_held_one`; phase 1 on the emulator and simulator. | The lock is meaningful on a local file system only (library specification); a store on a network share is not guarded. | verified |

### 3.5 The node

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T26 | **A network attacker reads or rewrites node traffic** (A7). | `https://` required; `http://` only to `127.0.0.0/8`, `::1` and `localhost`, with no override (PLAN 4.8; D22; `crates/wallet-core/src/node.rs:1-14`, `:116-137`). Every connection goes through that check. The library's transport: rustls with `ring` and the bundled `webpki-roots`; redirects off; timeouts; request and reply size caps (D22; library). Platform roots are not used, so an installed inspection root is not trusted (owner, 2026-10-04, D22). A second TLS stack in C is banned (`deny.toml:236-246`). | `node.rs:144` `loopback_is_the_only_plaintext_accepted`; `:167`; `:185`; `tests/worker.rs:1371` `the_node_is_chosen_https_or_loopback_only`. | A certificate wrongly issued by a root in Mozilla's store. No pinning. | verified |
| T27 | **A malicious or broken node** (A7). | No default node: the person chooses (PLAN 4.8; `node.rs:3-5`). The owner's default, `https://api.mochimo.org` (D43, 2026-10-13), supersedes PLAN 4.8's rule; `crates/` does not have it yet, and this row changes when it does. Changing or clearing the node detaches the open store and drops a pending plan (D22). The library: a returned address must begin with the tag asked for; a submit's echoed id must match the one computed locally; the store's index never moves back, so a lying node cannot make one store reuse a key (I4). Replies without the tip's timestamp or the genesis block are refused, and an impossible block time is no solve time, never a panic (D30, D31). Text from the node is made safe to show (`crates/wallet-core/src/explorer.rs:961`). "Account not found" is never read as an account that does not exist (D31 item 5). "Send to this account" is offered only for an address the person typed, never one the node named (D31; `app.rs:5351`). A balance not known is never counted as zero (`crates/wallet-core/src/view.rs:399`). | `tests/worker.rs:1448` `clearing_or_changing_the_node_stops_the_open_wallet_using_the_old_one`; `explorer.rs:952`, `:961`, `:969`; `tests/explorer.rs:123`, `:319`; `app.rs:5351`; `view.rs:399`. | Balances, the tip, history and settlement are the node's word; a lying node can make the wallet refuse, skip a key or settle early, and can show false balances and false incoming payments. Combined with a rolled-back copy (T2) or a second wallet (T5), a node that hides the chain's position can let a key sign twice (I4). There is no second node to compare with. **For the owner.** | verified (the protections named) |
| T28 | **The node learns who the person is and what they hold** (A7). | Explorer reads only when asked, never on a timer (D31 item 6). | — | The node sees every tag of the store on each open and refresh, and so learns that they belong together, with the asker's network address and timing. The library's proxy features are banned (`deny.toml:238`), so routing is only at the system level. Not recorded as accepted. **For the owner.** | owed (acceptance) |

### 3.6 Signed transactions and the clipboard

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T29 | **The signed bytes are lost** before the spend settles. | The three facts are never softened, and the sent page opens with them (PLAN 4.9; D28). Desktop: Save artifact writes to Downloads under a new name, never overwriting, and shows the path (`worker.rs:2999-3040`; D27 item 5); Copy hex. The spend is kept for the run however its page is left, a banner stays until it is saved, and the stopped screen offers every one (docs/SCREENS.md W6; D32 item 6). Phone: `Host::share` asks the system to open its share sheet. `Err`, with the reason, is a refusal: the platform could not be asked to show the sheet. `Ok` says only that it was asked, so a refusal the platform makes later is not reported. The spend still counts as not saved (D32 item 10 (i), amended 2026-10-13; `crates/app/src/host.rs:30-38`). | `report.rs:916` `the_sent_summary_states_the_three_facts`; `app.rs:3843`, `:4000`, `:4070`; `crates/app/src/app/wallet.rs:2504`, `:2526`, `:2560`, `:2576`; `worker.rs:3157` `save_artifact_never_overwrites`. | Tawara-mobile's `main` has no share sheet: its `Host` predates `Host::share`, and the shells' share is in draft Tawara-mobile#8 (G17). Until it merges and its pin moves, a phone has Copy hex only. | desktop: verified. Phone: owed |
| T30 | **The signed bytes reach someone else**: the clipboard, Downloads, a share target. | The bytes authorise only the spend already reserved and submitted (D27 item 5). iOS's share sheet excludes Copy (`Tawara-mobile` #8, `crates/mobile/src/ios.rs:259-260`, draft). | — | Anyone holding them can submit that same spend again (W9 needs no store), and they show its payees and amounts. On Android the chooser is the system's own window, outside `FLAG_SECURE`: it previews the text, and its 'Copy text' action copies the text with no sensitive mark, which Tawara cannot set. The owner accepted this on 2026-10-13 as an exception to D32 item 4, for the sheet's lifetime only (D32 item 10 (i), amended 2026-10-13; G18). | source only |
| T31 | **The clipboard is read or swapped** (A3, A4): a copied destination replaced by an attacker's. | Copy buttons copy only public material: destinations, transaction ids and the signed hex (`app/wallet.rs:1250-1269`). On a phone every Copy button goes through the shell's protected write: Android marks it sensitive (`ClipDescription` `IS_SENSITIVE`, read from API 33, so below that the mark does nothing; `Tawara-mobile:crates/mobile/src/android.rs:125-190`); iOS makes it local-only, expiring after two minutes (`Tawara-mobile:crates/mobile/src/ios.rs:138-191`; D32 item 10 (j)). Paste buttons read only on a tap, and a late read, or one for a form no longer shown, is dropped (D32 item 10 (g)). W5 shows each destination to check against its payee before signing (docs/SCREENS.md W5). | Tawara-mobile CI: the copy is on the simulator's pasteboard and gone after 121 s (`platform/ios/tawara.sh:226-278`); `ios.rs:395-439` asserts the options. `app.rs:3091`, `:3127`, `:3164`, `:3226`, `:3245`; `send.rs:2288`. | Desktop copies have no expiry or sensitivity mark (G21). Android's mark is not checked by any script (`Tawara-mobile:.github/workflows/mobile.yml:24-25`), does nothing below API 33, and Android has no expiry (D32 item 10 (j)). Copy or cut from a hardware keyboard in a field that is not secure goes through iced's clipboard, which on a phone sets no protection (`fork:winit/src/clipboard/ios.rs:22-26`, `fork:winit/src/clipboard/android.rs:146-170`) (G22). A swapped destination is caught only if the person checks it on W5. | iOS: verified on the simulator for the expiry only, read from outside with `simctl pbpaste`; local-only is asserted, not observed (`Tawara-mobile:crates/mobile/src/ios.rs:170-172`, `:418-424`). Android: source only. Desktop: owed |
| T32 | **A signed transaction broadcast by accident.** | On a phone, Return in W9's field does nothing (owner, D32 item 9 (f); `RETURN_BROADCASTS` in `crates/app/src/screens/wallet/send.rs`). | `send.rs`: `return_in_the_signed_bytes_field_submits_on_the_host` and `return_in_the_signed_bytes_field_does_nothing_on_a_phone`. | On the desktop, Return in W9 and the Submit button send at once with no confirmation, in both W9 modes, the artifact's and the resubmit (D32 item 9 (f); D38 item 5 (b)). That the constant is false on phones is checked only by review and the phone builds. | verified (desktop host) |

### 3.7 The person's own mistakes

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T33 | **The phrase or the password is lost.** | The phrase is confirmed by three words before the store is written (`tests/worker.rs:99`); the library's `SCHEME_WARNING` is shown whole on S6 (docs/SCREENS.md S6). | `tests/worker.rs:99`, `:189`, `:222`. | No way to recover either. The app has no password-change control: D27 item 4 leaves it out because the library cannot back it, and item 11 leaves that library change to the owner. | verified |
| T34 | **A wrong destination or amount.** | Destinations are checked by the library's parser, with its checksum; W5 lists every destination in wire order with the library's figures; the library's "NOT SIGNED" page, titled for an emptying spend when the change is zero (D26; D28); signing is a separate step. An account asked for by name stays the chosen one (`app.rs:4562`). | `app.rs:4562`; the planned-page tests (D26). | The person does not check. | verified |
| T35 | **Pressing on past a refusal.** | Five states fail closed, each with a banner, a summary and the library's page (PLAN 4.9; D28); no button resets, deletes or restores the store or moves an index to make a refusal go away (PLAN 4.9; D27 item 3); the library's protective words are its own, and any paraphrase is the owner's (PLAN section 3; D25). | `report.rs:837`, `:892`; `crates/wallet-core/src/sample.rs:1107`, `:1144`. | The summaries' wording (D28) is the owner's to approve (D25; G24). | verified |
| T36 | **A counterfeit Tawara** installed in place of the real one. | Packaging exists, unsigned: `packaging/macos.sh`, `packaging/linux.sh` and `packaging/windows.sh` build the packages from pinned tools (D36; PR #43). Signing and notarising are the owner's steps, on the owner's machines, where the keys are (D36 decision 4; `docs/RELEASE.md` sections 6.1 to 6.3). No signed package exists yet. | None for signatures. The packaging job checks the unsigned packages' contents (`packaging/check.sh`). | Until the owner signs and publishes, a user cannot tell a genuine build. | owed (signing: the owner's step) |

### 3.8 The supply chain

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T37 | **A changed or malicious crate.** | `Cargo.lock` committed and every resolving command run `--locked` (D13). `cargo deny`: advisories with `unmaintained = "all"` and `yanked = "deny"`, two ignores each with a reason and re-examined at every iced move (`deny.toml:45-73`; D12); permissive licences only; wildcards denied; unknown registries and git sources denied, with the library and the iced fork the only git sources (`deny.toml:282-295`); feature bans on `ureq`, `argon2`, `serde_json`, `native-tls`, `openssl-sys` and the library's `raw-backend`; `digest` 0.11 denied (`deny.toml:196-280`). | `cargo deny --locked check` in CI (`.github/workflows/ci.yml:203-213`), cargo-deny 0.20.2 built from crates.io with `--locked`. | A new advisory is seen only when CI runs. Updates are reviewed by hand. `deny.toml` has no rule against `rustls-native-certs` or `rustls-platform-verifier`, so D22's choice of bundled roots is held by review only (G25). `policy.rs` never reads `deny.toml`; the gate is CI alone. | verified |
| T38 | **The library changed under the wallet.** | Pinned by full hash, never patched, vendored or overridden; no `[patch]`, `[replace]`, `vendor/` or cargo-config override; the lockfile resolved at the pin; the licence text pinned by hash (D1, D7, D10, D15). | `policy.rs:1724`; `cargo deny` (a changed licence text fails it). | Trust in the library's repository and its maintainer. The manifest scans cover the root and `crates/*`; `spikes/` has its own `[patch]` and an older library pin, outside the shipped workspace (D17; G26). | verified |
| T39 | **The iced fork changed under the wallet.** | Pinned by full hash to a release tag; every fork package at that commit, once; one `winit`, from the fork, named by no manifest; `default-features = false` with the listed features (D32 items 1 and 9; `deny.toml:248-257`). | `policy.rs:1925`, `:2088`, `:2117`, `:2137`, `:2172`. | Trust in the fork's repository, which is the owner's. Each move of the pin also needs a hand-run desktop check on macOS, Windows and Linux (D32 item 9 (a) and (h); `docs/DECISIONS.md:1709`, `:1831`), which no job runs. | verified |
| T40 | **CI subverted.** | No third-party action: no `uses:` line; plain commands; `permissions: {}`; an anonymous clone fetching the commit by SHA; CRLF checkouts refused; nothing uploaded (D13; `ci.yml:40-98`). The toolchain is pinned to 1.98.0 with its components (D5). | The workflow itself. | Runner images are `-latest` (`ci.yml:57`); `apt-get` installs are unpinned (`ci.yml:140-142`); `rustup` comes from the image. CI prints an image that differs as base64 into its public log (`ci.yml:179-186`); it is drawn from fixed sample data (D27 item 1) and the published test vector (`crates/wallet-core/src/secret.rs:92-95`), not a secret. | verified (the commands and the CRLF refusal, `ci.yml:88-98`); source only (no `uses:` line, `permissions: {}` or upload: no test reads `.github`) |
| T41 | **The release profile undone**, skipping `Drop` scrubs or letting balances wrap. | `panic = "unwind"` and `overflow-checks = true`, restated in the root manifest; any override that turns overflow checks off or sets `panic = "abort"` fails a test (D11). | `policy.rs:2250` `release_profile_keeps_the_library_scrubs`. | Overrides from outside the repository (`$CARGO_HOME`, `RUSTFLAGS`), which CI does not set (D11). | verified |
| T42 | **Packaging and signing** subverted or absent. | Packaging is in the repository (`packaging/`, `.github/workflows/packaging.yml`): pinned tools, each download checked against its SHA-256 (D36 decision 8), the glibc floor checked, and a job with `permissions: {}`, no secret and no upload (D13). Signing is not in CI and runs only on the owner's machines (D36 decision 4). The Windows installer is made from the NSIS archive whose hash is recorded (D36 decision 8). | `packaging/check.sh` and the packaging job, on unsigned packages. Nothing checks a signature yet. | As T36. NSIS's download page publishes no checksum, so the archive's hash was trusted on first use: recorded 2026-10-19 from one network, at the owner's request, and matched SourceForge's published SHA-1 and MD5 (D36 decision 8). | owed (signing: the owner's step) |

### 3.9 The device unlock (iOS, D45)

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T43 | **The kept password opens the store for whoever passes the phone's check.** With the device unlock on, the passcode is enough: the system's prompt may accept Face ID, Touch ID or the passcode, so anyone who knows the passcode can open the wallet (D45 item 2). A face or a finger can be compelled, which no row of section 2 names. | The unlock is off until the person accepts the offer, which is made once, after a password unlock has opened the store (D45 item 4; `crates/app/src/app.rs`, `make_offer` and `answer_offer`). The password the offer would keep is copied only from the unlock's send until the answer, and `release_device` drops it with any ask on a lock, a move to the background and a stop (T12). The password is kept under the store identifier: 32 random bytes in `store-id` beside `preferences`, drawn once and not secret (`crates/wallet-core/src/store_identifier.rs`). An item labelled with another identifier, or with no identifier to match, is deleted and not used (`app.rs`, `unlock_with_device` and `device_missing`). A password the store refuses as wrong is deleted, and the unlock is turned off (`Purpose::KeptUnlock`). Turning the unlock off deletes the password first (`turn_off_device_unlock`). A declined check changes nothing, and an answer that comes after its ask was dropped is ignored, with its password wiped (`released`). The shell's side, the Keychain item under `.userPresence` and `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly` as D45 item 2 designs it, will be in a Tawara-mobile pull request, not yet opened (its number goes here when it is), and it is not in this change. The Tawara password is still the way in, and its field stays on S7 (D45 item 3). | `crates/app/src/app.rs` tests `the_device_unlock_is_offered_once_after_a_password_unlock` (`:9187`), `declining_the_offer_keeps_nothing` (`:9281`), `a_device_unlock_sends_unlock_with_the_password_it_released` (`:9299`), `a_wrong_kept_password_is_forgotten_and_the_form_is_shown` (`:9331`), `a_mismatched_identifier_forgets_the_password_and_shows_the_form` (`:9358`), `a_declined_check_keeps_the_password` (`:9393`), `turning_the_unlock_off_forgets_the_password` (`:9433`), `a_password_released_after_the_app_left_the_foreground_is_not_used` (`:9484`), `the_offer_waits_on_the_wallet_and_a_lock_takes_it_away` (`:9539`); `crates/app/src/host.rs` `a_released_password_and_an_answer_print_no_text` (`:637`); `crates/wallet-core/src/store_identifier.rs` `the_identifier_file_is_private_to_the_user` and `a_file_that_is_not_an_identifier_is_replaced`. | A passcode opens the wallet with the kept password, and so does a compelled face or finger. A passcode can be watched (A5), or shared, a risk D45 item 5 records and this row keeps. The shell's own copies of the password, the one it is given to write and the one it reads back, are not zeroed by Tawara, since the calls carry text. A Keychain item that survives an uninstall is reached by no check until a keep replaces it (G28). **For the owner** (5.4 item 16). | verified in the app with a fake shell; the Keychain side and the device, owed (V19, G28) |

### 3.10 The camera: QR scanning on a phone (D42)

| # | threat | protections (sources) | checked by | residual | status |
|---|---|---|---|---|---|
| T44 | **A scanned text fills a destination row it should not**: a code that is not a destination, is longer than a destination can be, is not UTF-8, or was scanned for a row or a page that has since changed (A11). | The text is trimmed, bounded at 128 bytes, read as UTF-8 and checked by `parse_destination` (`crates/app/src/scan.rs`, `destination`). It fills only the row the scan began for, and only while the page, its form and that row are as they were when the scan began (`crates/app/src/app/wallet.rs`, `scan_fits` and `scanned`). A refusal shows under that row's address. No other message takes the text, so it never reaches a password or a phrase field. The scanner's page draws the camera's frame and no text a code holds (`crates/app/src/screens/wallet/send.rs`, `scanner`). | `scan.rs` tests `a_destination_is_trimmed_and_checked_as_the_library_checks_it`, `a_text_is_bounded_once_trimmed_and_not_before`, `an_empty_text_and_a_text_that_is_not_utf8_are_refused`; `app.rs` tests `a_scan_fills_the_row_it_was_begun_for_and_no_other`, `a_text_that_is_not_a_destination_is_refused_under_its_row`, `an_overlong_or_non_utf8_text_is_refused_under_its_row`, `a_reply_after_the_row_the_scan_fills_changed_is_dropped`, `a_reply_after_a_row_was_added_or_the_page_was_left_is_dropped`, `a_reply_to_a_scan_that_has_ended_is_dropped_even_when_another_is_under_way`; `send.rs` tests `the_scan_page_draws_its_frame_a_line_and_cancel_and_fits_a_phone` and `a_tap_on_a_scan_button_asks_for_its_own_row_and_none_is_asked_while_a_scan_is_under_way`. | A valid destination that is the wrong one fills its row, as a swapped code would: only the review catches it, as it does a Paste (T34; D42 item 4). | verified in the application; the shells' scanners owed (G29) |
| T45 | **The camera keeps reading after the person has left the scan** (A1, A2): the page is left, the store locks, or the application moves to the background. | The scan stops when the page or its row changes, checked after each message (`update`); on a lock (`Message::Lock`, and the worker's `Locked`); on a move to the background, where the shell stops the camera on its own thread as `host::left_foreground` runs (`leaving_hook`) and the application drops the scan at its next message (`Model::backgrounded`); on Back on the scanner's page, which ends the scan through `stop_scan` and is not the shell's Back (`Message::SystemBack`; owner, 2026-10-18); and when the shell's reply comes back, a read or none (`scanned`). A reply to a scan that has ended is dropped. | `app.rs` tests `a_read_stops_the_camera_as_its_reply_is_taken`, `a_reply_to_a_scan_that_has_ended_is_dropped_even_when_another_is_under_way`, `leaving_the_page_locking_or_backgrounding_stops_the_camera`, `a_scan_ends_when_the_destinations_are_continued_past`, `a_scan_cancelled_or_refused_by_the_shell_fills_nothing`, `back_on_the_scanner_page_cancels_the_scan_and_keeps_the_form`, `a_preview_frame_is_shown_while_a_scan_draws_its_preview_in_the_application`. | The shells must stop the camera when they are told, which only a device shows (G29, V20). | verified in the application; on a device, owed (G29, V20) |

## 4. Platform specifics

### Desktop: Windows, macOS, Linux

- **Store.** `%LOCALAPPDATA%\Tawara\keystore` (never Roaming);
  `~/Library/Application Support/Tawara/keystore`;
  `$XDG_DATA_HOME/tawara/keystore` or `~/.local/share/tawara/keystore` (D21).
  The person may choose another folder; a synced one is warned about (T4).
  Unix folders Tawara makes are `0700`; on Windows they take the parent's
  access list, and the library makes the store's own folder private (D21).
- **Backups.** Time Machine, File History and shadow copies may keep copies
  of the store. The owner accepted the residual exposure (D21; T2).
- **Locking.** The idle period and Lock only. Minimising the window, or
  moving to another, does not lock; nothing in `crates/desktop` calls
  `left_foreground`, and the application hears only the window's opening
  and resizing (`app.rs:2202-2204`). docs/SCREENS.md S3 and S7 name "the
  window leaving the screen" as a lock reason; on the desktop it does not
  happen (G9).
- **Screen.** No protection from screenshots, recording or screen sharing,
  at S4 or anywhere (G2).
- **Clipboard.** iced's write, with no expiry or sensitivity mark (G21).
  Keyboard paste only; no Paste buttons (D32 item 10 (g)).
- **Input methods.** On Wayland the secret fields are marked sensitive to
  the input method (T17). On X11, macOS and Windows the purpose is not known
  to reach the input method; not checked.
- **Same-user malware** can read the store, the clipboard and the screen
  and log keys (A3). Nothing on the desktop prevents it; the store's
  encryption and the password are what remain.
- **Verified** on every pull request: build, clippy, tests, docs and
  `cargo deny` on Windows, macOS and Linux, and on Linux the screenshot
  check and a start-up check on wgpu and on tiny-skia (D32 item 9 (h)). The
  start-up check passes if the process lives and its log holds no panic; a
  panic on another thread does not end the process.

### Android (Tawara-mobile)

- **Store.** `no_backup/keystore`, made `0700`; `allowBackup="false"` and
  `dataExtractionRules` excluding every domain (D21; D32 item 7). No folder
  field (D32 item 10 (a)).
- **Screen.** `FLAG_SECURE` on the window before the first frame, for the
  whole app (D32 item 4), with one exception: the system's share chooser,
  which `FLAG_SECURE` does not cover. The owner accepted it on 2026-10-13,
  for the sheet's lifetime (D32 item 10 (i), amended 2026-10-13; G18).
- **Locking.** `Suspended` (the window destroyed) locks and wipes, without
  waiting (D32 item 6). Back moves the task to the background and never finishes
  the activity (`Tawara-mobile:crates/mobile/src/android.rs:97-108`;
  `AndroidManifest.xml:28`), and so locks the application, except on pages 2 to
  4 of a phone's Send, and on the scanner's page while the application draws its
  preview (D42 item 5; owner, 2026-10-18). There Back steps back a page, or
  cancels the scan, inside the application, without moving the task to the
  background, so it does not lock (`SystemBack` in `crates/app/src/app.rs`; D40
  item 3, for the owner's approval with the pages). Going back a page keeps the
  form, and gives up a plan only as Edit does (D40 item 2); a cancelled scan
  keeps it too. On those pages, as on the desktop, the idle period and Lock are
  the only locks. On page 1 of Send, except on the scanner's page, and
  everywhere else, Back moves the task to the background as before. After
  `Destroy` the process ends with `libc::_exit`, without waiting for the lock
  (D32 item 9 (j), amended 2026-10-13; G6).
- **Clipboard.** Copy buttons mark the copy sensitive. The system reads the
  mark from API 33, so below that it does nothing; Tawara's minimum is API
  26 (D32 item 10 (j) and its note). Android gets no expiry and no
  local-only copy. Hardware-keyboard copy in non-secure fields is
  unprotected (G22).
- **Keyboard.** Ordinary; the secure purpose is dropped (T17; G3).
- **Configuration changes.** The 18 names of the API 35 set are handled, so
  a change of country code, network code or grammatical gender does not
  recreate the Activity (D32 item 9 (i); `AndroidManifest.xml:39-44`). Target
  API 36 is PR B, due before 2026-11-01 if the Play extension is needed
  (D32 items 9 (i) and 10 (k)).
- **Camera (D42; not built, G29).** The scanner's permission is
  `android.permission.CAMERA`, with `android.hardware.camera` and
  `android.hardware.camera.any` both not required, so that the app installs on a
  phone with no camera (D42 item 5; owner, 2026-10-18). The permission is asked
  through JNI and then polled, since a NativeActivity is not expected to receive
  the answer; that is an inference, to be checked on a device (D42 item 3). The
  camera's frames reach the application through `host::preview_frame`. The
  camera stops when the scan ends, on a lock, on a move to the background, and
  on Back on the scanner's page (T45; owner, 2026-10-18). Nothing of this is in
  the shell yet, and nothing has run on a device (G29, V20).
- **Verified** in Tawara-mobile CI on an x86_64 API 35 emulator: 14 checks,
  passed on 587329c (the manifest's attributes, `SECURE` in `dumpsys`,
  `no_backup` mode, the lock's log lines on Home and Back). Nothing on a
  physical phone; a real arm64 device decides what the x86_64 image's
  translation could not (docs/spikes/P1-REPORT.md, A22; V12).

### iOS (Tawara-mobile)

- **Store.** `Library/Application Support/Tawara/keystore`, made `0700`,
  `NSFileProtectionComplete`, `NSURLIsExcludedFromBackupKey` on the folder
  (D32 items 3 and 7). Protection is set at launch and not read back (G14).
- **Screen.** A cover over the key window when the app is about to become
  inactive, so the switcher's snapshot shows nothing while there is a key
  window. With none, no cover is added and the log says so
  (`Tawara-mobile:crates/mobile/src/ios.rs:384`); the `scene.window`
  check (`platform/ios/tawara.sh:262-263`) catches that on the simulator only
  (D32 item 6). Once the app is active again nothing hides the screen, and
  the app does not stop a screenshot, a screen recording or mirroring of S4
  (G27).
- **Locking.** On entering the background a UIKit background task holds the
  process until the interface has taken the worker's answer to the move;
  if time runs out, or none is granted, the process ends itself (D32 item 6;
  owner, 2026-10-06).
- **Clipboard.** Copy buttons write a local-only item that expires after two
  minutes (D32 item 10 (j)). Hardware-keyboard copy in non-secure fields is
  unprotected (G22).
- **Keyboard.** Ordinary; the secure purpose is ignored (T17; G3).
- **Scenes.** The scene manifest and the fork's scene adoption keep the app
  launching on SDKs after iOS 26 (D32 item 8); re-checked at release.
- **Device unlock (D45).** The Keychain side will be in a Tawara-mobile pull
  request, not yet opened (its number goes here when it is). Until it is in the
  shell's `main`, no phone keeps a password or offers the check (T43).
- **Camera (D42; not built, G29).** The scanner is UIKit's own screen, presented
  as the share sheet is, with `AVCaptureMetadataOutput`, which decodes the code
  (D42 item 2). Its usage text is `NSCameraUsageDescription`, which awaits the
  owner's approval (D42 item 5). The camera stops when the scan ends, and on a
  move to the background, where the shell stops it on its own thread (T45).
  Nothing of this is in the shell yet, and nothing has run on a device (G29,
  V20).
- **Verified** in Tawara-mobile CI on the simulator: 18 checks, passed on
  587329c (protection call returned success, backup attribute, folder mode
  and access list, cover added, key window in a scene, lock on background,
  pasteboard expiry). The simulator does not enforce data protection.
  Nothing on a physical phone.

## 5. Known gaps and open items

"Owner" items are decisions or device checks only the owner can make (the
owner has the devices, D32 item 2). "Delegated" items are engineering work
done under the owner's delegation, through pull requests the owner reviews.

### 5.1 Gaps: threats without a control, or controls without a check

| # | gap | threat | owner | what closes it |
|---|---|---|---|---|
| G1 | No warning before a divergence, anywhere but W12, that one phrase must not be live in two wallets. After a divergence the summaries name a second wallet as a cause (`crates/app/src/screens/wallet/report.rs:191-192`, `:240`, `:243`). | T5 | Owner | The owner decides whether S4 and S6 need one, and approves its wording (PLAN section 3). **(Recommended)** add one; the library's own warning on its advance page is the source. |
| G2 | No desktop protection for the phrase on screen. | T16 | Owner | **(Recommended)** record it as an accepted risk, with a plain line on S4 before the phrase is shown, in words the owner approves. The alternative is platform calls that ask capture to leave the window out, which exist on Windows and macOS with limits and not on X11. |
| G3 | The phone keyboard may learn or suggest what is typed in secret fields. | T17 | Owner (device check V1), then delegated | The device check of D32 item 2. If composition or secure entry fails, native text-input glue in Tawara-mobile (D32 item 2, `docs/DECISIONS.md:1461-1463`); whether learning alone counts is the owner's to decide. Otherwise record the result here. |
| G4 | Typed secrets are plain `String`s in the forms and in each keystroke's message until submitted, against PLAN 4.2's "at once". | T12 | Owner | **(Recommended)** record the deviation in DECISIONS: iced's `text_input` is handed its value each time it is drawn and copies it into graphemes of its own (`fork:widget/src/text_input.rs:135-139`), so a field that never leaves a copy needs the fork's widget changed, which is the owner's to allow. Amend PLAN 4.2's wording to what holds: the value is taken into `Zeroizing` on submit and wiped on leaving. |
| G5 | `Model`, `Screen` and the four secret-holding forms derive `Debug` over plaintext. | T12, T14 | Delegated | Manual `Debug` that prints `(..)` for those types, with a test like `app.rs:5046`. |
| G6 | **Answered** by the owner's 2026-10-13 amendment to D32 item 9 (j). With the fork's winit, `std::process::exit` after `Destroy` aborted with SIGABRT on an arm64 API 36 emulator in every run, so, as (j) prescribes for a reproduced SIGABRT, the process now ends with `libc::_exit`. `exit.lock` is a reported count, not a gate (owner, 2026-10-13). A bounded wait after `run_in` cannot help. The phone check is V4. | T11 | Owner (device check V4) | Done on the emulator, as the amendment records. Still owed: V4 on the phone, and the emulator check repeated at the release's pin (section 6, item 2). |
| G7 | No check shows that `FLAG_SECURE` blanks screenshots, recordings and the recents thumbnail from the first frame. CI reads only the flag. | T16 | Owner (device check V2) | Device check V2. |
| G8 | No check shows the iOS switcher's snapshot is covered; CI reads the cover's log line, with Settings in front. | T16 | Owner (device check V6) | Device check V6. |
| G9 | The desktop does not lock on minimise or loss of focus, and no decision records that it should not. SCREENS.md S3 and S7 imply it can. | T10 | Owner | **(Recommended)** keep the idle lock as the desktop's rule (PLAN 4.1 ties the background lock to mobile) and record it in DECISIONS; correct the two SCREENS.md rows to say "on a phone". |
| G10 | No mechanism or check for PLAN 4.2's "not exposed as a value to accessibility services". | T18 | Owner (device check V7) | Decide what is required; check with TalkBack and VoiceOver; record the result. |
| G11 | No test that copy and cut are refused in secret fields. | T19 | Delegated | A headless test in `crates/app` that sends the copy and cut shortcuts to each secret field and checks that the clipboard is not written. |
| G12 | A phrase pasted into S6 stays on the system clipboard. | T20 | Owner | **(Recommended)** clear the clipboard after a phrase is pasted by the Paste button, and say so on S6. |
| G13 | No list of what the apps log: Tawara-mobile writes lifecycle lines to logcat and the system log, and the graph's `log` and `tracing` crates (`Cargo.lock:1790`, `:3467`) write what iced, winit and the rest of the graph send them. | T14 | Delegated | A list in DECISIONS of every log line the shells and the application write, with a check that none carries a secret, an address or a balance. |
| G14 | iOS file protection is set at launch only, and nothing reads the class back. The record gives a locked-phone guarantee to iOS only (D32 item 3), not to Android's private storage. | T22 | Delegated, then owner | A readback of the protection class of the store folder and of the store file after its first commit in the simulator checks (the attribute can be read there); a device check (V5). For Android, the owner decides whether the system's file encryption is enough for a locked phone, and records it. |
| G15 | The library's Unix arm (directory handle, `flock`, `fsync`, owner and mode checks) has not run on a physical phone (PLAN section 2). | T1, T23 | Owner (device check V8) | Device check V8. |
| G16 | The Android backup rules' contents are checked by no script, and no check shows that a backup skips the store on either phone. | T3 | Delegated, then owner | A script check of `data_extraction_rules.xml`'s domains; on a device, `bmgr` and a device-transfer test (Android) and a device backup (iOS) that leave the store out. |
| G17 | No working save path on phones: Tawara-mobile's `main` predates `Host::share`, and #8 (the share sheet) is a draft that lacks Tawara-mobile `main`'s later merges and pins wallet `4ce2550`. | T29 | Delegated | Merge Tawara-mobile `main` into #8, move its pin to this repository's `main`, run its checks, and merge it. |
| G18 | The Android share chooser is drawn outside `FLAG_SECURE`, previews the signed transaction's text, and offers a 'Copy text' action that Tawara cannot mark sensitive. | T30 | Owner (device check V3) | Recorded by the owner on 2026-10-13 as an exception to D32 item 4 (D32 item 10 (i), amended 2026-10-13). What remains is device check V3, and whether item 4's sentence is narrowed (C4). |
| G19 | Phone confinement is enforced in `crates/app` only. | T24 | Delegated | Refuse a folder outside the hosted private folder in `wallet-core` too, with a test. |
| G20 | No user-facing note about desktop backup tools, although the owner accepted their exposure (D21). | T2 | Owner | **(Recommended)** a short note on S3 or W11, in words the owner approves. |
| G21 | Desktop copies have no expiry or sensitivity mark. | T31 | Owner | **(Recommended)** record as accepted: the desktop copies only public material, and the platforms' clipboards offer no portable expiry. |
| G22 | Hardware-keyboard copy and cut in non-secure fields on a phone bypass the shell's protected write. | T31 | Delegated | Route iced's standard clipboard writes through `Host::copy` on phones, with the wrapper pattern already used for paste (`crates/app/src/paste.rs:105-118`); no fork change. Once done, a hardware-keyboard copy gets what a Copy button gets on that platform (D32 item 10 (j)): on iOS, local-only and expiring after two minutes; on Android, the sensitive mark from API 33, with no expiry and no local-only copy. V15 checks each platform on those terms. |
| G23 | The records do not say what follows if Android suspends the process before the lock has finished. | T11 | Owner | Open. G6's check covers the exit after `Destroy`, not suspension; V4 on a phone and the owner's acceptance (5.4 item 12) remain; record the result. |
| G24 | The wording still open: the placeholders in the table at `docs/SCREENS.md:180-196` ("The application's own texts (D25)"), which D25 leaves to be settled with their screens; and the summaries' sentences under D28 (`crates/app/src/screens/wallet/report.rs`), which no decision approves word for word (D25; D27 item 12). | T35 | Owner | The owner approves the wording on the built screens. |
| G25 | `deny.toml` does not hold D22's bundled roots. | T26, T37 | Delegated | Ban `rustls-native-certs` and `rustls-platform-verifier` in `deny.toml`, with a reason naming D22. |
| G26 | The policy test's manifest scans skip `spikes/`, which has a `[patch]` (allowed by D17) and an older library pin. | T38 | Delegated | Extend the scan to every manifest, listing the spike's `iced_winit` patch as the one exception under D17. |
| G27 | **iOS while the app is active.** The app does not stop a screenshot, a screen recording or mirroring of S4. The cover comes off on `DidBecomeActive` (`Tawara-mobile:crates/mobile/src/ios.rs:338`, `uncover` at `:403-409`); the iced fork's iOS `set_content_protected` is empty (`fork:vendor/winit/src/platform_impl/ios/window.rs:459`); no code pinned before D44 item 3 reads `UIScreen.isCaptured`, which Apple's Technical Q&A QA1970, "Responding to screen capture in iOS 11" (https://developer.apple.com/library/archive/qa/qa1970/_index.html), describes as true while the screen is recorded (QuickTime Player over a cable among the routes), mirrored or sent over AirPlay; and PLAN 4.3 asks for the iOS cover only as far as the switcher's snapshot (`docs/PLAN.md:171-172`). V6 does not see this. | T16 | Owner (device check V18), then delegated | The control is D44 item 3, merged with PR #26 (a recommendation, since on 2026-10-13 the owner decided only D44's frame order, item 1): on iOS the grid is blanked while iOS reports the screen as recorded or mirrored, as far as it reports it, and a warning follows a screenshot, which cannot block it; on a phone the warning takes the words' place until "Show the phrase" is pressed (D44 item 1). Its app side is built in this repository; its iOS calls are in Tawara-mobile, and it is not active on a phone until they land. Item 3 also records developers' reports that the capture state can read inactive while a recording runs, so the blanking is best effort, and V18 can fail even once the control is built. The owner decides G27 after V18 has a recorded result (section 6, items 1 and 6); meanwhile 5.4 item 15 puts the risk to the owner. |
| G28 | **A Keychain item left by an earlier install.** The item may survive an uninstall (D45 item 5). After one, `store-id` and the setting are gone with the folder, so the unlock starts unoffered and asks nothing, and only a keep reaches an item left behind. | T43 | Owner (device check V19) | V19 on a device, recording whether the item survives and whether a keep replaces it. If it survives, the owner decides between deleting it at the first start where no `store-id` exists (a delegated change in `crates/app`, through the shell's forget) and accepting it (5.4 item 16). |
| G29 | **The camera on a phone is not checked on a device.** The shells' scanners (AVFoundation's metadata output on iOS, Camera2 and `rqrr` on Android) are Tawara-mobile's. Until they are, no code is read on a phone. The Scan button and the scanner page are built in the application, which draws the camera's frame and no text a code holds, and its tests check the camera's stop on a read, a page's leaving, a lock, a move to the background and Back on the scanner's page (T44, T45). The Android spike of about 50 captures per decoder (D42 item 3) is owed too. | T44, T45 | Delegated (the shells), then Owner (the devices, V20) | The shells, through their own pull request; then V20 on each phone, with the spike's results recorded in the records. |

### 5.2 Device checks still owed

None has a recorded result on a phone. The owner runs them (D32 item 2), on an
arm64 Android phone and an iPhone, on the builds to be released (section 6).

| # | check | for | source |
|---|---|---|---|
| V1 | **Text entry and learning.** Type into the password and phrase fields: does the keyboard suggest, autocorrect or learn what is typed? Also accented and non-Latin text, composition, glide typing, and paste from another app, noting any prompt or notice each platform shows for it. | G3, T17 | D32 item 2; spikes/README.md, "What CI cannot measure" |
| V2 | **`FLAG_SECURE` timing.** From a cold launch, a screenshot and a screen recording show nothing from the first frame on; the recents thumbnail is blank; the same after rotating and after a return from the background. | G7, T16 | D32 item 4 |
| V3 | **The Android chooser.** Save artifact on W6. The owner's 2026-10-13 amendment to D32 item 10 (i) records that a screenshot taken while the chooser was up showed its preview. This check covers a screen recording, what the chooser's 'Copy text' action copies, and what each target receives. | G18, T30 | D32 item 10 (i) and its 2026-10-13 amendment; Tawara-mobile#8 |
| V4 | **Android exit after `Destroy`.** Back from the foreground, on a page other than pages 2 to 4 of a phone's Send (there Back steps back a page, as the Locking row says); then with "Don't keep activities" on; read `RustStdoutStderr` for the lock's completion and for SIGABRT. The emulator run is recorded in the 2026-10-13 amendment to D32 item 9 (j) (G6); the phone run is owed. | G6, G23, T11 | D32 item 9 (j); Tawara-mobile#4 |
| V5 | **iOS data protection.** The protection class of the store's folder and of each of its files after a commit, read on the phone; and that the files cannot be opened while the phone is locked. | G14, T22 | D32 item 3 |
| V6 | **The iOS app-switcher snapshot** shows the cover, with no wallet content, from every screen. | G8, T16 | spikes/README.md, "What CI cannot measure" |
| V7 | **Accessibility.** TalkBack and VoiceOver on S3, S6 and S7: what is read out of the secret fields, and whether the app can be used at all. | G10, T18 | PLAN 4.2 |
| V8 | **The library on a phone's file system**: create, open, commit, a second open refused, a group-writable folder and a linked folder refused. | G15, T1, T23 | PLAN section 2; docs/spikes/P1-REPORT.md |
| V9 | **The first gesture after launch, and touch.** The first tap after a cold launch and after a return; the footer link by touch; a drag that starts on a row, and one that starts on a field or a button of a compact form (S3, S6, S7, W4, W8, W9), which does not scroll the page and is "not yet checked on a device"; Paste; the keyboard shown again on a tap of the focused field; the system's keyboard sheet over the soft keys. | usability of every control above | D32 item 10 (d) and (f) |
| V10 | **The background lock on a device**: Home, the app switcher, a call and the screen locking, each during a node request and during a spend; the wallet is locked on return and nothing typed survives. On iOS the lock must finish before the background time runs out, or the process ends itself (D32 item 6); read the lock's log line where the device's log can be read. On Android the lock is not waited for (D32 item 6), so record whether the lock's line comes before the process is suspended; G23 and V4 cover what follows if it does not. | T11 | D32 item 6 |
| V11 | **Backup and transfer leave the store out.** On Android, `bmgr` and a device-to-device transfer; on iOS, a device backup and its restore. The store is absent from each restored copy. | G16, T3 | D21; D32 items 3 and 7 |
| V12 | **The app starts on arm64.** A cold launch on an arm64 Android phone, then an unlock. The x86_64 image's translation could not decide this. | section 4 (Android) | docs/spikes/P1-REPORT.md, A22 |
| V13 | **A kill mid-send, and split screen (Android).** The system kills the app between a reservation and its submission (a low-memory kill, or the app closed from the switcher). On relaunch and reconcile the reservation is still open, and the spend can be re-signed (W8) or submitted from its saved bytes (W9). On Android, in split screen with another app, check what the wallet draws and whether it locks; an iPhone has no split screen. | T7, T11 | spikes/README.md, "What CI cannot measure"; D32 item 6 |
| V14 | **Copy mark on Android.** On an Android 13 or later phone, copy a destination with a Copy button, then open the system's clipboard preview. The copy is marked sensitive, and its text is not shown there. | T31 | section 4 (Android); D32 item 10 (j) |
| V15 | **Hardware-keyboard copy and cut, per platform.** On a phone with a hardware keyboard, copy and cut in a field that is not secure, then check the copy on that platform's terms, and record the API level or iOS version. **iOS:** the copy must be local-only, so a Mac or iPad signed in to the same Apple account, with Handoff on, must not receive it; and it must expire two minutes after the copy, so a paste into another app works within two minutes and finds nothing after. **Android:** the copy must carry the sensitive mark. On Android 13 (API 33) or later the clipboard preview then shows no text, as V14 checks for a Copy button. Below API 33 the mark has no effect, and nothing the system shows reflects it, so the check cannot observe it there. The record gives Android no expiry and no local-only copy, so none is checked. Until G22 is done the copy goes around Tawara's write, and the record must say so. | G22, T31 | section 4, Clipboard bullets; D32 item 10 (j) and its note on API 26 to 32; G22 |
| V16 | **The first screen on an iPhone SE.** On an iPhone SE (3rd generation, iOS 18.3), where the safe area reported no inset on a cold launch, check that the wordmark stands clear of the 20-point status bar, with the fallback of D32 item 10 (b) in place. | usability | D32 item 10 (f); `docs/DECISIONS.md:2100-2107` |
| V17 | **Drawing, speed and memory on a phone.** Real-GPU drawing, frame pacing, unlock time and memory on a phone; scrolling on iOS, which `simctl` cannot drive. | usability | spikes/README.md, "What CI cannot measure" |
| V18 | **Screen recording and mirroring while S4 is shown (iPhone).** Start a capture by one of three routes: a screen recording from Control Center; a recording in QuickTime Player on a Mac, with the phone on a cable; or a mirror to another screen, by AirPlay or by a wired display adapter. Then open S4 with a phrase on it, and check the recording or the mirror: the words must not be visible in it. Record the iOS version and the route used. The app blanks the words while the shell reports a capture (D44 item 3), but a recording can still show them if iOS reports the screen as not captured, so a fail is recorded, not waived. Also take a screenshot of S4 with the words drawn: the warning must take their place afterwards, and "Show the phrase" must bring them back (D44 item 1). **Not run:** it needs an iPhone and a Tawara-mobile build that reports the capture; until one exists, no phone blanks S4 or warns after a screenshot. | G27, T16 | G27; D44 item 3 (merged with PR #26); Apple QA1970 |
| V19 | **The device unlock on a phone.** On an iPhone with Face ID and one with Touch ID (D45 item 6), with the unlock on: unlock with the check; with a face or a finger that does not match, then with the passcode; cancel the check; and move the app to the background while the check is open. Read the usage string on the prompt. Then delete the app, reinstall it, and read the Keychain: whether the item is there, and whether a keep after the reinstall replaces it. Record the iOS version and the device for each. | T43, G28 | D45 items 2, 5 and 6 |
| V20 | **The QR scanner on a phone.** Once the shells are built, on an iPhone and on an arm64 Android phone: allow the camera at its first prompt, and refuse it on a second install; scan a destination into a row, then a code that is not a destination, then one with more text than a destination can hold; scan from a printed sheet and from a screen, in light and in shade; end a scan by a read, by Cancel, by Back on the scanner's page on Android, by leaving the page, by a lock and by a move to the background, and check that the camera's light goes out each time. On Android, run the spike of about 50 captures through `rqrr` (D42 item 3), and record the decode time and any wrong text, of which there must be none. Record the OS version and the device for each. | T44, T45, G29 | D42 items 2, 3 and 5 |

### 5.3 Records that disagree

| # | records | what to do |
|---|---|---|
| C1 | **What an interrupted send can leave.** D19 says the system ending the app "between signing a spend and recording it ... leaves the account behind the chain". D28 says a chain-ahead cause is "never a crash during a send". D32 item 6 says the index "is written before a spend is signed". The library's I2 holds D28 and D32 item 6 for a kill at a syscall boundary, which is how iOS and Android end a process, and leaves power loss and an `fsync` that does not flush outside its claim; its I4 still names "a crash between signing and persisting" among the causes of divergence. | Owner. **(Recommended)** correct D19's sentence to the library's order (the index is durable before the signature exists), and add to D28's causes, in words the owner approves, that power loss during a send is the one crash I2 does not cover. The library's authors confirm the reading first. |
| C2 | PLAN 4.2 "at once" against the forms' plain `String`s (G4). | Owner, with G4. |
| C3 | PLAN 4.5 excludes the store from every backup; D21 makes the desktop an exception, and PLAN is not amended. | Delegated: a note in PLAN 4.5 pointing to D21. |
| C4 | D32 item 4 says no screenshot or recording of any screen. The owner's 2026-10-13 amendment to D32 item 10 (i) makes the Android share chooser an exception to it; item 4's own sentence is unchanged (G18). | Owner, with G18: the amendment records the exception, and the owner may narrow item 4 to name it. |
| C5 | README.md says every safety property lives in the library; the application enforces others (the idle and background locks, wiping typed secrets, phone confinement, the paste rules). | Delegated: correct the README's sentence. |
| C6 | docs/SCREENS.md S3 and S7 name "the window leaving the screen" as a lock reason without saying it is a phone's (G9). | With G9. |
| C7 | **Whether the secret is copied on the way in.** D20 says the secret "is never copied on the way in" (`docs/DECISIONS.md:645-647`). Each keystroke's `Typed` (`crates/app/src/app.rs:407`) carries a new `String` from iced's `on_input` (`crates/app/src/ui.rs:304-313`; `crates/app/src/screens/first_run.rs:713`), so the text is copied on the way in. Only the final take moves the buffer. | Owner, with G4: D20's sentence is for the final take, or the forms change (G4). |

### 5.4 Residual risks for the owner's acceptance

Already accepted: T2's backup rollback (owner, 2026-10-04, D21) and D22's
bundled roots (owner, 2026-10-04). Not yet accepted, each named in section 3:

1. T1, T7: power loss during a send is outside the library's durability claim.
2. T5: two live wallets on one phrase (with G1 decided).
3. T9: a reverted settle, whose repairs are the spend landing after all and a saved copy sent again from W9 (D38 item 5 (b)).
4. T10: no password before signing, and the desktop's lock on idle only.
5. T13: memory not locked, core dumps not suppressed.
6. T21: the 12-character floor with no complexity rule.
7. T27: the node's word on balances, settlement and the chain's position.
8. T28: what the node learns.
9. T30: the signed bytes as something anyone holding them can submit.
10. T16 on the desktop (G2) and T31 on the desktop (G21). On phones each
    waits on the checks that can see it: for T16, V2 (Android) and V6 (iOS)
    for the flag and the cover, and item 15 with V18 for an iPhone while
    the app is active; for T31, V14 and V15.
11. T4: a synced folder kept after its warning, and a sync provider that
    the warning's list does not name.
12. T11: Android may suspend the process before the lock has finished
    (G23).
13. T32: on the desktop, Return in W9 and the Submit button send at once
    with no confirmation, in either W9 mode (D32 item 9 (f); D38 item 5 (b)).
14. T33: a lost phrase or password cannot be recovered, and the app has no
    password-change control (D27 items 4 and 11).
15. T16 on an iPhone while the app is active (G27). Today nothing blanks S4 or
    warns: the cover is off while the app is active, so a screenshot, a screen
    recording or a mirror shows the words. D44 item 3 (proposed, merged with
    PR #26) recommends a warning after a screenshot, which comes after the
    capture and cannot block it, with the words hidden behind it until "Show
    the phrase" is pressed (item 1), and a best-effort blanking of a recording
    or a mirror. V18 is the device check for the recording and mirroring case.
16. T43: the kept password opens the store with the phone's passcode alone,
    a face or a finger can be compelled, and a passcode can be watched or
    shared (D45 items 2 and 5). The offer says that anyone who knows the
    passcode can open the wallet, and the unlock is off until the person
    accepts it.
17. T44: a code printed or placed over another one (A11) can give a valid
    destination that is the wrong one, and it fills its row. Only the review
    catches it, as it does a destination typed or pasted wrongly (T34). D42 item
    4 names this residual, and the owner is asked to accept it, or to add a
    control.

## 6. Before release

PLAN section 7 makes this file a condition of phase 5. These must be closed,
or accepted by the owner in writing in DECISIONS, before any release:

1. **Every device check in 5.2**, V1 to V20, on the builds to be released, with
   results recorded in DECISIONS. A failure of V1 (what counts as one is the
   owner's to decide, T17), V2, V5, V6, or of the footer link or Paste in V9,
   blocks the phone release it concerns until fixed.
2. **G6** (the Android exit after `Destroy`) is answered by the
   2026-10-13 amendment to D32 item 9 (j). Its emulator check is repeated at
   the release's pin, and its device check is V4, in item 1.
3. **G17** (a working save path on phones): Tawara-mobile#8 merged, with
   its pin on this repository's `main`.
4. **C1** resolved, so the causes a person reads after a divergence are
   right.
5. **G4 and C7** decided: fixed, or the deviation from PLAN 4.2 and D20
   recorded.
6. **G1, G2, G10, G12, G24 and G27** decided by the owner, G27 after V18
   has a recorded result (item 1).
7. **Section 5.4** accepted, item by item, or a control added.
8. **Phase 5's own items**: signed packages for every platform (T36, T42),
   third-party notices generated in CI, the counsel questions answered
   before first distribution (PLAN section 6; D10), the installed
   executable's name (D9), the release checklist, Android target API 36
   (D32 item 9 (i)), and UIScene re-checked (D32 item 8).

Should close, and may follow the first release if the owner agrees: G5,
G9, G11, G13, G14's simulator readback, G16's script check, G18's record,
G19, G20, G21, G22, G23, G25, G26, and C3 to C6.

This file is kept current: a change that adds, removes or weakens a control
in section 3 updates its row in the same pull request.
