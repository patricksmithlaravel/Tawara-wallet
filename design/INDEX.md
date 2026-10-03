# Reference renderings

The owner's renderings are the visual target for the interface
(docs/PLAN.md section 5). This file indexes them. They are never edited.

## Source

| | |
|---|---|
| file | `Mochimo_V3_Wallet__Explorer.html`, supplied by the owner on 2026-10-03 |
| size | 3,989,041 bytes |
| SHA-256 | `6bf331c121e25f2fac858dd678db969d77c38307732471044df47dbb65d47b6a` |
| form | a self-unpacking HTML bundle: a board of eight desktop frames, each an embedded page with its fonts (Montserrat, Poppins, IBM Plex Mono, as WOFF2) |

**The bundle itself is not in this repository.** Its packaging script
carries text that the owner's standing rule on attribution keeps out of the
repository, and that rule overrides the instruction to commit the renderings
unmodified (docs/DECISIONS.md, D14). What is committed instead is each frame
as an image, drawn from the bundle exactly as supplied. The owner keeps the
source file; the hash above identifies it.

## How the images were made

Each frame's page was opened on its own in headless Chromium (Playwright
1.56.1, Chromium build 1194) with a viewport of the frame's own size and a
device scale factor of 1, with all network requests blocked (every asset is
embedded in the bundle), left 4 seconds to render, and captured at the
viewport. The images are what the bundle draws; nothing was retouched,
cropped or rescaled.

## Index

| file | screen | platform | pixel size | frame title in the source |
|---|---|---|---|---|
| `renderings/01-create-or-restore.png` | first run: create, restore or import | desktop | 1440 × 900 | Desktop · Create or restore |
| `renderings/02-wallet-dashboard.png` | wallet: balance, one-time keys, accounts, recent activity, network | desktop | 1440 × 1160 | Desktop · Wallet dashboard |
| `renderings/03-batch-send.png` | send: up to 256 destinations, summary, signing key, password | desktop | 1440 × 860 | Desktop · Batch send |
| `renderings/04-activity-and-receipt.png` | activity list and one transaction's detail | desktop | 1440 × 860 | Desktop · Activity & receipt |
| `renderings/05-keystore-and-network.png` | settings: accounts and key indexes, keystore, node, display, about | desktop | 1440 × 1120 | Desktop · Keystore & network |
| `renderings/06-explorer-overview.png` | explorer: chain stats, latest blocks, mempool, block types | desktop | 1440 × 980 | Desktop · Explorer overview |
| `renderings/07-block-detail.png` | explorer: one block, its poem, stats, hashes, transactions | desktop | 1440 × 1200 | Desktop · Block detail |
| `renderings/08-tag-lookup.png` | explorer: one account tag, its three address forms, history | desktop | 1440 × 1000 | Desktop · Tag lookup |

## Notes

- **Desktop only.** Every frame is 1440 px wide, so these drive the
  expanded layout. There are no phone renderings; the compact layout is
  proposed to the owner before it is built.
- **Dark only.** Every frame is dark. The settings frame offers Dark, Light
  and System; no light frame exists (docs/PLAN.md section 5: "Build a dark
  variant only if the renderings show one").
- **Two frames overflow slightly.** The dashboard's content is 1165 px tall
  in a 1160 px frame and the explorer overview's 1028 px in a 980 px frame.
  The images show each frame as the board shows it; what is cut is margin
  below the last card.
- **Branding.** The frames carry the MOCHIMO wordmark and logo. The
  application is named Tawara and is not branded as an official product of
  the cryptocurrency (docs/PLAN.md section 6), so this is put to the owner
  before any screen is built.
- **Conflicts with the threat model.** Several controls conflict with
  docs/PLAN.md section 4, which wins (section 5, "Conflicts"). They are
  listed for the owner with the phase 0 review, and each one's safe version
  is recorded in docs/SCREENS.md when phase 3 builds the screen.
