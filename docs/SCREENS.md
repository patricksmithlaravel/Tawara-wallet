# Screens

Every screen of the desktop application (docs/PLAN.md section 7, phase 3),
the rendering it follows (`design/INDEX.md`), the worker commands it sends,
and where it departs from the rendering and why. docs/DECISIONS.md D27
holds the recommendations this inventory rests on. Rendering numbers are
those of `design/INDEX.md`: 01 create or restore, 02 dashboard, 03 batch
send, 04 activity, 05 settings, 06 explorer, 07 block, 08 tag.

The **PR** column says which of D27's four pull requests (item 16) builds
the screen: (a) the first-run screens and the shell, (b) the wallet, (c)
activity, settings and recovery, (d) the explorer. Every built screen has
a screenshot in `design/screenshots/` with the same name.

## The states that fail closed (docs/PLAN.md section 4.9)

Each of these gets a designed screen, never an OK dialog, and each shows
the library's own page whole (D27, item 13):

| state | where it is shown | what the person can do there |
|---|---|---|
| an account diverged from the chain | account state panel (W7, `diverged`) | read the library's report; open account recovery (W12). Nothing on this screen moves an index |
| reserved and not settled | account state panel (`outstanding`) | settle once the chain shows it landed; re-sign the same spend (W8); submit a saved artifact (W9) |
| a reservation that can no longer be accepted | account state panel (`dead`) | read why (balance moved, or its block-to-live passed); settle after it is resolved on the chain |
| an account emptied to zero | account state panel (`not found`) | read the library's reading of "account not found"; receive to it again |
| a spend between submission and settlement | sent page (W6) | read the three facts; save the artifact; settle later |

The three facts are never softened: a submit is a socket write, not a
verdict; the node's validation has never run offline; the signed
transaction can be lost, so the sent page offers to save it (D27, item 5).

## First run and unlocking (no sidebar)

Layout of 01: the brand panel on the left (green, the "Tawara" wordmark,
the headline and the feature pills; no coin mark or pattern, D27 item 2),
the form on the right, the node footer under it.

| id | screen | rendering | PR | commands | notes |
|---|---|---|---|---|---|
| S1 | **Get started**: create, or restore from a recovery phrase | 01 | a | — | "Import a legacy .mcm file" is not built: the library has no import (D27, item 4). When a store is there to open (the one last opened, wherever it is, or else the default folder's), S7 is shown instead, with a link here; S1 links to S7 too ("Open a wallet already on this computer"). |
| S2 | **Choose a node**: the URL, a test, save | 01 footer, 05 node card | a | `SetNode`, `NetworkStatus`, `ClearNode` | No default node (section 4.8). The plaintext refusal is the library's, with D22's three changes. Remembered in preferences (D27, item 10). |
| S3 | **New wallet: folder and password** | — | a | `CreateBegin` | The default folder (D21) with the sync warning when the chosen one is synced (section 4.5). The password floor is the library's (`MIN_PASSWORD_LEN`). Masked fields (section 4.2). When a phrase waiting on S4 or S5 is discarded by the auto-lock or by the window leaving the screen, S3 comes back in the folder chosen, saying nothing was created and why. |
| S4 | **New wallet: the recovery phrase**, shown once | — | a | — | 24 words in a numbered grid. No copy button, no clipboard (section 4.3). Continuing asks the person to confirm they wrote it down. Leaving drops it (`CreateAbandon`). |
| S5 | **New wallet: confirm three words** | — | a | `CreateConfirm` | The positions are the library's (`CONFIRM_POSITIONS`). A wrong answer keeps the phrase pending; the refusal is shown on the screen. So does a store the library refuses to write (an unsafe folder, say): the folder can be put right and the words given again. If the worker no longer holds the phrase, the one shown is dropped and S3 comes back with the refusal. |
| S6 | **Restore from a recovery phrase** | 01 (second option) | a | `CreateFromPhrase` | The library's `SCHEME_WARNING` is shown before the phrase field, whole. Masked phrase field. |
| S7 | **Unlock** | — | a | `Unlock` | The store's folder (default first). The library's refusals: wrong password, store in use (section 4.7), no store, unsafe folder. A store made on S5 or S6 that could not be opened afterwards is the one offered here, with the refusal, and it is remembered. |
| S8 | **Opening**: waiting its turn, deriving the key, asking the node, progress | — | a | `Event::Busy`, `Event::Progress`, `WorkerHandle::cancel` | Shown from the moment S3, S5, S6 or S7 sends its command, so nothing else can be started while it waits behind an earlier one (a slow node's answer); Cancel stops both. A cancel leaves the store open on its own (D26). |

## The wallet (sidebar: Wallet, Send, Receive, Activity, Explorer, Settings)

The sidebar of 02-08: the "Tawara" wordmark, the six items, and on 02 and
03 the network panel (node, block, latency) with "Lock wallet" (section
4.1).

| id | screen | rendering | PR | commands | notes |
|---|---|---|---|---|---|
| W1 | **Wallet**: total balance, one-time keys, accounts, recent activity, network | 02 | b | `Refresh`, `NetworkStatus`, the explorer reads | Accounts are named by their shortened destination, with "derived" or "imported" (D27, item 3). "Reconcile" on a diverged row and "Review →" open W7, never an advance. The settling chip counts outstanding spends, with no amount. The store's notice (`WalletView::notice`) is shown whole above the cards, never folded away. While a refresh runs, a card says what it is doing in S8's words, with its progress and Cancel, and Refresh and Node wait until it answers. |
| W2 | **Receive**: an account's destination | — (nav item of 02) | b | `Receive` | The destination in Base58 with its checksum, a copy button, and the library's explanation of the ledger address beside it, whole. |
| W3 | **Add an account**: the next derived account's destination, and adding it once funded | 02 "New account", "Discover tags" | b | `Discover`, `Restore` | The library adds a derived account only where the chain already shows it (`restore`), so the screen shows where the next account receives, then adds it. Discovery results are the library's page. |
| W4 | **Send: compose** | 03 | b | `PlanSend` | Up to 256 destinations, references by the node's rule, "everything" for one destination. "Import CSV" and "Scan QR" are not built (D27, item 5). No password field (D27, item 3). |
| W5 | **Send: review before signing** | 03 summary | b | `ConfirmSend`, `DiscardPlan` | The library's "NOT SIGNED" page (`PlanView::text`) whole, with its "THIS EMPTIES THE ACCOUNT" warning when the change is zero, then "Sign & submit". |
| W6 | **Send: sent** | 03 steps, 04 receipt | b | `Settle` | The library's sent page whole (the three facts). "Save artifact" and "Copy hex" (D27, item 5). The Reserve, Submit and Settle steps show what has happened, nothing more. |
| W7 | **Account**: its state, and the actions that state allows | 02 rows, 05 rows | b | `Status`, `Settle` | The section 4.9 panel for its state (table above). |
| W8 | **Re-sign the reserved spend** | 03 | b | `Resign` | The same spend exactly, or the library refuses it; the library's page says so. |
| W9 | **Submit a saved artifact** | — | b | `SubmitArtifact` | Paste the hex. Opens no store. |
| W10 | **Activity**: every account's transactions, and one transaction's detail | 04 | c | the explorer reads | From the node's transaction index, newest 100 per account (D27, item 11). Outstanding spends from the store's own state, with no amounts. No "Receipt verified" (D27, item 3). "Export CSV" not built. |
| W11 | **Settings**: accounts and key indexes, keystore, node, display, about | 05 | c | `Refresh`, `SetNode`, `NetworkStatus` | "Advance to #N" becomes "Review" (W12). No "Show recovery phrase", "Export keystore", "Change password" or "Theme" (D27, items 3, 4, 8). Auto-lock: 1, 2, 5, 10 or 15 minutes. Amounts: MCM or nanoMCM. |
| W12 | **Account recovery** (the acknowledged advance) | 05 callout | c | `Status`, `Reconcile`, `Restore` | The owner's terms in D19: reached on purpose; every account's report first; the person types the index the report names; they confirm no other wallet uses this seed; then the library advances or refuses. |

## The explorer

| id | screen | rendering | PR | commands | notes |
|---|---|---|---|---|---|
| E1 | **Explorer**: search, the tip, the latest blocks | 06 | d | the explorer reads | Block height and the last block's age. Not built until the library serves them: difficulty, average solve, mempool, next neogenesis, block types (D27, item 4). |
| E2 | **Block**: its hashes, reward, transactions | 07 | d | the explorer reads | No haiku, difficulty, nonce or root yet. "Yours" marks a transaction from an account in this store. |
| E3 | **Tag**: balance, the three forms, history | 08 | d | the explorer reads | "Current key on ledger" only for an account in this store. |
| E4 | **Transaction**: one transaction by id | 04 detail | d | the explorer reads | Needs the node's index; the library's refusal when it has none. |

## The worker's own texts (D25, D26 item 3, D27 item 12)

| text | where it is shown |
|---|---|
| no node chosen (`NO_NODE`), node changed (`NODE_CHANGED`), node silent (`NODE_SILENT`), reconciling cancelled (`OPEN_CANCELLED`) | the store's notice on W1, after S7 or S8 |
| restore cancelled (`RESTORE_CANCELLED`), advance cancelled (`ADVANCE_CANCELLED`) | W3, W12 |
| the create refusals, each ending "Nothing was created." | S3, S5, S6 |
| "there is no keystore at ...: the folder does not exist" | S7 |
| the short refusals for a command in the wrong session | wherever the command was sent |
| a key index out of range; a discovery bound outside 1 to 1,024 | W3, W12 |
| the plaintext-node refusal (D22) | S2, W11 |

## The application's own texts (D25)

Placeholders on D25's terms, settled with the owner:

| text | where it is shown |
|---|---|
| the introductions under each first-run title, the radio cards' descriptions, the field labels and "At least 12 characters." | S1 to S7 |
| the phrase warning: the library's words from `create`, without its sentence about the terminal | S4 |
| what the worker is doing: waiting its turn, deriving the key, reconciling, "Account N of M", "N of at most M key positions searched" | S8, and W1 while it refreshes |
| "Nothing was created": a recovery phrase discarded before it was confirmed, and why (the auto-lock, or the window leaving the screen) | S3 |
| why the store locked: idle ("Locked after N minutes with nothing done..."), the window leaving the screen, a store that could not be read back | S7 |
| the notes naming the control for a command-line verb under a library page (D27, item 13) | wherever a library page is shown |
| "The wallet stopped", with or without a fault | the stopped screen (whatever the screen before it held, a phrase or a password, is zeroized and dropped) |
| the account states in a word or two ("Reconciled", "Spend outstanding", "Spending paused"...) | W1 |
