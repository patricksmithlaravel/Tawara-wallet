# Design tokens from the reference renderings

Measured on 2026-10-03 from the source of the owner's renderings: the file
`design/INDEX.md` identifies by SHA-256, which is not in this repository.
This reference is what keeps its exact values here. docs/PLAN.md section 5
asks for the tokens to be derived into one theme module in `app`; phase 3
builds that module from this file, and records every deviation in
docs/DECISIONS.md. Where a value here belongs to a control that conflicts
with docs/PLAN.md section 4, or to the branding put to the owner, it is
recorded as drawn, not as approved.

These are desktop-only, dark-only frames. This reference covers the eight frames indexed in `design/INDEX.md`, numbered as in that index: 01 create or restore, 02 wallet dashboard, 03 batch send, 04 activity and receipt, 05 keystore and network settings, 06 explorer overview, 07 block detail, 08 tag lookup.

**Conventions**

- Values in backticks are quoted exactly from the frames' inline CSS, SVG attributes or row-style scripts.
- Values marked **derived** are computed from those values. Examples are flex sizes resolved at the 1440 px frame width, colour blends, and line heights from font metrics.
- Values marked **measured** were read from the frames as rendered at 1440 px in the Chromium build that drew the PNGs (build 1194, device scale factor 1). They are layout boxes and computed styles, or pixels in the PNGs. Positions are rounded to the nearest pixel unless decimals are given.
- At that scale factor, Chromium draws every border declared as `1.5px` at 1 px (§4.2).
- The frames have no hover, pressed, focus or disabled styles except the ones listed in §1.7.

---

## 1. Colour palette

All colours are opaque 6-digit hex. The frames use no `rgba()`, no gradients and no named colours, apart from the keywords `transparent` and `currentColor`. There are 20 distinct values:
- 19 appear in inline styles, SVG attributes and the frames' row-style scripts;
- `#5FF0BC` appears only in the shared stylesheet (`a:hover`).

### 1.1 Backgrounds and surfaces

| Token | Value | rgb8 | Used for |
|---|---|---|---|
| `bg_app` | `#1D2A2E` | 29, 42, 46 | Page background (`background: #1D2A2E` on the root). Also: the 01 feature pills; the inset fill for form inputs, the select, the account selector and the 03 address field; the segmented-control track; node info tiles (05); the sidebar network panel. Doubles as `text_on_accent` (§1.3) and as brand ink on 01. |
| `bg_surface` | `#172124` | 23, 33, 36 | Sidebar, every card, radio cards (01), search fields, stat tiles, the "Live" chip (06). |
| `bg_raised` | `#24353A` | 36, 53, 58 | Neutral chip ("3 accounts"), tonal buttons ("New account", "Test"), icon circles for outgoing and own-account rows, "Normal" block tags and tiles, the node icon tile (05). |
| `accent_soft` | `#173B33` | 23, 59, 51 | Active nav item, "Settled" chip, selected table row (04), "Yours" row (07), green callouts, "Neogenesis" tag, the "Next neogenesis" tile (06), incoming-row icon circle. |
| `warning_soft` | `#3D3520` | 61, 53, 32 | "Settling" chip, "Pseudo" tag and tile, pending-row icon circle, "Reconcile" button, warning icon tile (05). |
| `warning_surface` | `#2A2A1F` | 42, 42, 31 | Amber callout background and the warning table row (05). |

### 1.2 Borders and dividers

| Token | Value | Used for |
|---|---|---|
| `border_subtle` | `#2A3A3F` | Card borders (`border: 1px solid #2A3A3F`) and stat tiles. Every divider (`border-bottom: 1px solid #2A3A3F`). Sidebar `border-right` and the sidebar network panel. The unselected radio card. `border-top` on the 01 footer and on the 03 Total row. The "Live" chip border. The receipt destinations box. |
| `border_control` | `#2E4045` | Inputs, select, account selector, 03 address field, outline and icon buttons, search fields, unselected filter tabs, lock button. Also the fill of a pending progress-step bar (03). |
| `border_strong` | `#3A4D52` | Outline "Receive" header button (`1.5px solid`) and the dashed "Add destination" button (`1.5px dashed`). |
| `accent_border` | `#1F5A49` | Green callouts (03, 04) and the "Next neogenesis" tile (06). |
| `warning_border` | `#5A4A22` | Amber callout (05) and the "Inspect tag" outline button. |
| `swatch_normal` | `#4A5E63` | Only the "Normal" legend swatch under "Last 6 blocks" (02). |

### 1.3 Text

| Token | Value | Used for |
|---|---|---|
| `text_primary` | `#FFFFFF` | Default text colour (root `color: #FFFFFF`), values, amounts, titles. |
| `text_secondary` | `#B4C2C5` | Inactive nav items, descriptions (01), balance decimals, addresses and hashes in tables, form labels, unselected segments and tabs, sidebar panel text, own-account amounts, secondary table cells, neutral chip and tag text, icon-button glyphs. |
| `text_muted` | `#8FA0A4` | Section labels, table headers, page subtitles, captions, helper text, timestamps, breadcrumbs, search and chevron icon strokes, the 03 row numbers and remove button, the disabled-looking "Previous" button. |
| `text_on_accent_soft` | `#CFEFE2` | Body text inside green callouts; label and sub-line of the "Next neogenesis" tile. |
| `text_on_warning_soft` | `#F2E3BF` | Body text of the amber callout; "Inspect tag" label. |
| `text_on_accent` | `#1D2A2E` | Text and icons on `#15DC96` and `#F5B841` fills. Same value as `bg_app`. |

The frames do not set a placeholder colour, so the placeholders on the 02, 04 and 06 search inputs use Chromium's default. **Measured:** `#757575` as computed, and visible in the 06 PNG.

### 1.4 Accent (green)

| Token | Value | Used for |
|---|---|---|
| `accent` | `#15DC96` | Primary buttons and the accent badges and tags. Links (`a{color:#15DC96}`). Active nav text. Positive amounts. OK status text and dots. The "MCM" unit labels. Selected tab and segment fills. The selected radio-card border and `accent-color` on radios. The tip-tile border, the selected-row ring and complete progress bars. The valid-checksum tick. The "Add destination" label. The brand panel (01), the logo mark and the watermarks. The key-hash half of the 08 ledger address. The poem caption (07). |
| `accent_hover` | `#5FF0BC` | Only `a:hover{color:#5FF0BC}`. |

### 1.5 Warning (amber)

| Token | Value | Used for |
|---|---|---|
| `warning` | `#F5B841` | Settling, pending, "Spending paused", "Review →", "Reconcile", "Pseudo" (tag text, tile text and legend swatch), the ledger-ahead index (05), the warning glyph (05), and the "Advance to #33" button fill. |

### 1.6 Error

No screen uses a red or error colour.

### 1.7 Semantic mappings and states

**Status text**
- `#15DC96`: "Reconciled", "ready", "Connected · 42 ms", "Indexer available".
- `#F5B841`: "settling", "Settling", "Spending paused", "Review →".
- `#8FA0A4`: "Settled" in the 02 recent-activity list.
- The 04 detail shows "Settled" as a green chip instead.

**Status dots**
- 8×8 at `border-radius: 4px` (01 footer, sidebar panel, 06 "Live" chip; always green).
- 6×6 at `border-radius: 3px` (02 accounts table).
- Filled `#15DC96` or `#F5B841`.

**Amounts**

| Direction | Colour | Sign |
|---|---|---|
| Incoming | `#15DC96` | `+` |
| Outgoing or pending | `#FFFFFF` | `−` (U+2212) |
| Own-account move | `#B4C2C5` | none |

On 08 the move to Savings is listed as an outgoing row: white, with `−`.

**Block types**

| Type | Background | Text |
|---|---|---|
| Normal | `#24353A` | `#B4C2C5` (`#FFFFFF` on 02 tiles) |
| Pseudo | `#3D3520` | `#F5B841` |
| Neogenesis | `#173B33` | `#15DC96` (tag) |
| Neogenesis, 02 tile | `#15DC96` | `#1D2A2E` |

**States defined in the frames**
- Link hover: `#5FF0BC`.
- Active nav: `background: #173B33; color: #15DC96`.
- Selected filter tab and selected segment: `background: #15DC96; color: #1D2A2E`.
- Selected radio card: `border: 1.5px solid #15DC96`.
- Selected table row (04): `background: #173B33; box-shadow: inset 0 0 0 1.5px #15DC96; border-radius: 12px`.
- Highlighted own row (07): `background: #173B33`.
- Warning row (05): `background: #2A2A1F` with 12 px end radii.
- Current tip tile (02): `border: 1.5px solid #15DC96`.
- Disabled-looking pager button: text `#8FA0A4` instead of `#FFFFFF`. No `disabled` attribute is set.
- Pending (03) and complete (04) progress steps (§5.14).

**Focus and pointer**
- Every text input sets `outline: none` and defines no focus style.
- The 05 select and the 01 radios do not set it, so they keep the browser's default focus ring.
- The only pointer rule is `button{cursor:pointer}`.

### 1.8 Brand panel, pattern and watermarks

- **Brand panel (01):** `background: #15DC96; color: #1D2A2E`.
- **Pattern:** `#1D2A2E` marks at `opacity: 0.16` over `#15DC96`. **Derived** blend ≈ `#16C085`; **measured** `#16BE84`–`#16BF85`.
- **Card watermark (02, 07):** `#15DC96` mark at `opacity: 0.10` over `#172124`. **Derived** blend ≈ `#17342F`; **measured** `#16342F`–`#163430`.
- **Measured artefact:** inside the watermark's box the PNGs draw the card fill as `#162023`. This is a compositing artefact; build the fill as `#172124`.

---

## 2. Typography

### 2.1 Families (stacks exactly as declared)

| Token | Declaration | Applied by |
|---|---|---|
| `font_body` | `font-family: Poppins, 'Avenir Next', sans-serif` | Root element. Everything inherits it unless overridden. |
| `font_display` | `.fd{font-family:Montserrat,'Avenir Next',sans-serif}` | Titles, numbers, amounts, most primary buttons. |
| `font_mono` | `.fm{font-family:'IBM Plex Mono',Menlo,monospace}` | Addresses, tags, hashes, IDs, key indexes, block numbers, references. |

**Shared stylesheet**
- In every frame:
  - `body{margin:0}`;
  - `.fd` and `.fm`;
  - `a{color:#15DC96;text-decoration:none}a:hover{color:#5FF0BC}`;
  - `button{font:inherit;cursor:pointer}`.
- `input{font:inherit}` on 02, 03, 04 and 06, and `input,select{font:inherit}` on 05. Neither appears on 01, 07 or 08.
- `th{font-weight:500;text-align:left}` on 02–08. 01 has no tables and no `th` rule.

There is one inline override, on the 04 fee line: `font-family: Poppins, sans-serif`.

### 2.2 Weights actually used

**Poppins (normal)**
- 400: default body, unset buttons.
- 500: nav, labels, chips, `th`, account names in the 02 and 05 tables.
- 600: active nav, badges and tags, selected tabs and segments, small emphasised buttons ("Reconcile", "Download", "Add destination", "Review →").
- No 700.

**Montserrat**
- Normal 600: section labels, callout titles, amounts in tables and lists, the 03 account-selector balance and amount inputs, 03 row numbers and step labels, the "Receive" button, the 06 "txs" unit.
- Normal 700: titles, stat values, the balance, primary buttons, 04 step labels.
- Italic 700: 01 headline, second line.
- Italic 500: 07 poem.
- **Unset weight.** Montserrat elements with no explicit weight request 400. Montserrat 400 is not declared, so the browser substitutes the nearest declared face, 500. **Measured:** the 400 and 500 advance widths are identical.
  - This applies to the 03 summary values, the 04 fee value, the 06 "Reward" column and the 07 "Fee" column.
  - Build these as **Montserrat 500**.

**IBM Plex Mono (normal)**
- 400: default mono.
- 500: 06 block-number links, 07 title number, 08 tag heading, 02 neogenesis tile.

### 2.3 Line height "normal" (derived from the bundled font files)

| Family | hhea values (UPM 1000) | Multiplier |
|---|---|---|
| Poppins | asc 1050, desc −350, gap 100 | 1.5 |
| Montserrat | 968 / −251 / 0 | 1.219 |
| IBM Plex Mono | 1025 / −275 / 0 | 1.3 |

The OS/2 typo values equal the hhea values and `USE_TYPO_METRICS` is set in all three, so either table gives the same result. **Measured** at 40 px, the line boxes are 60, 49 and 52 px.

Explicit line heights appear only where the type scale lists them. Every other element uses "normal".

### 2.4 Type scale by role

"lh" is line-height; "n" means normal (see §2.3). Colours are §1 tokens.

| Role | Family | Size | Weight | Letter-spacing | lh | Transform | Colour | Where |
|---|---|---|---|---|---|---|---|---|
| Hero headline | Montserrat | `52px` | `700`; line 2 `font-style: italic` | `-0.005em` (−0.26 px) | `1.05` | `uppercase` | `text_on_accent` | 01 brand panel. **Measured:** the first line wraps at 1440, so it sets in three lines, 164 px tall. |
| Onboarding title | Montserrat | `28px` | `700` | — | n | `uppercase` | `text_primary` | 01 "Get started" |
| Page title (h1) | Montserrat | `26px` | `700` | — | n | `uppercase` | `text_primary` | 02–08 |
| Page title number | IBM Plex Mono | 26px (inherited) | `500` | — | n | `none` | `text_primary` | 07 "Block 871,172" |
| Page subtitle | Poppins | `13px` | 400 | — | n | — | `text_muted` | under the h1 on 02–06 |
| Header meta | Poppins | `13px` | 400 | — | n | — | `text_muted` | 07, beside the h1 |
| Breadcrumb | Poppins | `13px` | 400 | — | n | — | `text_muted`; first crumb `accent`; current `text_primary` | 07, 08 |
| Intro paragraph | Poppins | `14px` | 400 | — | `1.55` | — | `text_secondary` | 01 |
| Card title | Montserrat | `16px` | `700` | — | n | — | `text_primary` | 05 cards |
| Detail title | Montserrat | `18px` | `700` | — | n | — | `text_primary` | 04 "Batch send" |
| Section label (caps) | Montserrat | `12px` | `600` | `0.12em` (1.44 px) | n | `uppercase` | `text_muted` (`accent` for the 07 poem caption) | card headers |
| Table header | Poppins | `12px` | `500` (`th` rule) | — | n | — | `text_muted` | all tables |
| Table body | Poppins | `13px` | 400; account names `500` (02, 05); 04 description title `500` | — | n | — | `text_primary` / `text_secondary` | all tables |
| Body | Poppins | `14px` / `13px` | 400 | — | n | — | `text_primary` | list rows, settings rows |
| List-row title | Poppins | `14px` | `500` | — | n | — | `text_primary` | 02 activity, 03 selector |
| Description / note | Poppins | `12px` | 400 | — | `1.5` | — | `text_muted` or `text_secondary` | 02 keys note, 06 block types |
| Helper text | Poppins | `12px` | 400 | — | n | — | `text_muted` | 03 field helpers, card sub-lines |
| Radio card title | Montserrat | `15px` | `700` | — | n | — | `text_primary` | 01 |
| Radio card description | Poppins | `13px` | 400 | — | `1.5` | — | `text_secondary` | 01 |
| Small / caption | Poppins | `12px` | 400 | — | n | — | `text_muted` | metadata |
| Tiny caption | Poppins | `11px` | 400 | — | n | — | `text_muted` | mempool meta, legends, 08 dt sub-lines |
| Form label | Poppins | `12px` | `500` | — | n | — | `text_secondary` | 03, 05 |
| Mono address in tables | IBM Plex Mono | 13px (inherited) | 400 | — | n | — | `text_secondary` (08 counterparty and 07 destinations `text_primary`) | 02, 05, 06, 07, 08 |
| Mono address under name | IBM Plex Mono | `12px` | 400 | — | n | — | `text_muted` | 03 selector |
| Mono reference sub-line | IBM Plex Mono | `11px` | 400 | — | n | — | `text_muted` | 04 |
| Mono hash value | IBM Plex Mono | 13px | 400 | — | n | — | `text_primary` (nonce `text_secondary`, linked hashes `accent`) | 07 fields |
| Mono transaction ID | IBM Plex Mono | `12px` | 400 | — | n | — | `text_secondary` | 04 detail |
| Mono tag heading | IBM Plex Mono | `20px` | `500` | — | n | — | `text_primary` | 08 |
| Mono block link | IBM Plex Mono | 13px | `500` (06) / 400 | — | n | — | `accent` | tables, 04 detail |
| Mono sidebar value | IBM Plex Mono | 12px | 400 | — | n | — | `text_primary` | sidebar panel |
| Big balance integer | Montserrat | `56px` | `700` | `-0.015em` (−0.84 px) | `1` | — | `text_primary` | 02 |
| Balance decimals | Montserrat | `26px` | `700` | — | n | — | `text_secondary` | 02, baseline-aligned |
| Balance unit "MCM" | Montserrat | `18px` | `700` | — | n | — | `accent` | 02, `margin-left: 12px` |
| Balance in nanoMCM | IBM Plex Mono | `12px` | 400 | — | n | — | `text_muted` | 02 |
| Receipt amount | Montserrat | `32px` | `700` | — | n | — | `text_primary`; unit `14px` `accent` | 04 |
| Summary total | Montserrat | `24px` | `700` | — | n | — | `text_primary`; unit `13px` `accent` | 03 |
| Unweighted values | Montserrat | 13px | unset, so 500 (§2.2) | — | n | — | `text_primary`; 03 change value and 07 fee `text_secondary` | 03 summary, 04 fee, 06 reward, 07 fee |
| Stat value | Montserrat | `24px` (06), `22px` (07, 08), `20px` (02) | `700` | — | n | — | `text_primary` | stat tiles |
| Stat unit | Montserrat | `12px` | inherits 700 | — | n | — | `accent` | 07, 08 |
| Stat unit "txs" | Montserrat | `13px` | `600` | — | n | — | `text_muted` | 06 |
| Stat label | Poppins | `12px` | 400 | — | n | — | `text_muted` | stat tiles |
| Table amount | Montserrat | 13px | `600` | — | n | — | per §1.7 | tables |
| List amount | Montserrat | `14px` | `600` | — | n | — | per §1.7 | 02 activity |
| Selector balance | Montserrat | `15px` | `600` | — | n | — | `text_primary`, unit in the same style | 03 |
| Poem | Montserrat | `28px` | `500` `italic` | — | `1.45` | — | `text_primary` | 07 figure |
| Primary button | Montserrat | `15px` (56/52 px tall), `14px` (44/42), `13px` (40) | `700` | — | n | — | `text_on_accent` | |
| Outline header button | Montserrat | `14px` | `600` | — | n | — | `text_primary` | 02 "Receive" |
| Secondary button | Poppins | `13px` | `500` (some unset, so 400) | — | n | — | `text_primary` | |
| Small button | Poppins | `12px` | `500` / `600` | — | n | — | varies | lock, "Reconcile" |
| Nav item | Poppins | `14px` | `500`; active `600` | — | n | — | `text_secondary`; active `accent` | sidebar |
| Chip (pill) | Poppins | `12px` | `500` (neutral, warning, Live) / `600` (status, header tag) | — | n | — | varies | |
| Badge / block tag | Poppins | `11px` | `600` | — | n | — | varies | |
| Feature pill | Poppins | `13px` | `500` | — | n | — | `text_primary` | 01 |
| Filter tab | Poppins | `13px` | `600` selected / `500` | — | n | — | varies | 04 |
| Segment | Poppins | `12px` | `600` selected / `500` | — | n | — | varies | 05 |
| Text link | Poppins | `13px` (`12px` on 01) | `500` | — | n | — | `accent` | card headers |
| Callout title | Montserrat | `14px` | `600` | — | n | — | `text_primary` | 03, 04, 05 |
| Callout body | Poppins | `12px` | 400 | — | `1.5` (03, 05); n (04) | — | `text_on_accent_soft` / `text_on_warning_soft` | |
| Step label | Montserrat | 12px | `600` (pending, 03) / `700` (done, 04) | — | n | — | `text_primary` | progress steps |
| Password field | Poppins | `15px` | 400 | `0.08em` (1.2 px) | n | — | `text_primary` | 03 |
| Search input | Poppins | `13px` (`15px` on 06) | 400 | — | n | — | `text_primary` | 02, 04, 06 |

### 2.5 Notes

- **Uppercase.** `text-transform: uppercase` appears only on page titles, the 01 headline and title, and section labels (23 elements). In iced, uppercase the strings themselves.
- **Tabular figures.** `font-variant-numeric: tabular-nums` is set on:
  - the 02 balance, table balances and activity amounts;
  - the 03 amount inputs and every summary value;
  - the 04 list amounts, receipt amount and receipt destination amounts;
  - the 06 reward column and mempool amounts;
  - the 07 amount and fee columns;
  - the 08 history amounts and Balance tile.

  It is **not** set on the other stat-tile values (02, 06, 07, 08), the 03 account-selector balance or the 04 fee value. The bundled Montserrat has a `tnum` feature; Poppins has none.
- **Units.** Unit spans are smaller than their number and in `#15DC96`: 18 on 56, 14 on 32, 13 on 24, 12 on 22. There are three exceptions:
  - 06 "txs" is 13 px, 600, `#8FA0A4`;
  - in the 03 account selector, "MCM" is plain text in the number's own style;
  - on the 04 fee line, "MCM" is also plain text in the number's own style.

---

## 3. Spacing and layout

### 3.1 Spacing scale

All `padding`, `gap` and `margin` values used, in px: **2, 3, 4, 6, 7, 8, 9, 10, 12, 14, 16, 18, 20, 24, 28, 32, 40, 48, 56**. 7 and 9 are one-offs. Proposed tokens are `space_2` … `space_56`.

| px | Typical use |
|---|---|
| 2 | title↔subtitle gap; stacked label↔value gap; badge vertical padding (`2px 8px`); header-cell bottom padding (03); segment gap; receipt-box vertical padding |
| 3 | segmented-control track padding |
| 4 | nav item gap; callout title↔body gap (03, 05); radio-card text gap; node-tile gap; 04 detail title gap; lock button `margin-top` |
| 6 | chip icon gap; tile label↔value gap; progress-step gap; filter-tab gap; label↔field gap; nanoMCM line gap; 02 last-6 tile gap; 08 header column gap |
| 7 | "Default" badge horizontal padding (`2px 7px`) |
| 8 | card header↔body gap in list cards; button icon gap; breadcrumb gap; destination row spacing (`border-spacing: 0 8px`); 01 title↔intro gap |
| 9 | block-type tag horizontal padding (`0 9px`) |
| 10 | header action gap (02, 03); chip-row gap; select horizontal padding; search icon gap; sidebar logo `margin-left` |
| 12 | stat-grid gap; list-row inner gap; table cell horizontal padding (02, 05); dl column gap |
| 14 | radio card inner gap; activity rows; button horizontal padding; table cell vertical padding (02, 04, 05, 07 fields); sidebar panel padding |
| 16 | sidebar horizontal padding; header gap; tile padding; 01 footer top padding; summary aside gap (03) |
| 18 | radio-card and callout padding; balance-card gap; 04 detail-card gap |
| 20 | page stack gap (04–08); column gap between cards; card padding (06 side) |
| 24 | card padding; page stack gap (02, 03); right-panel stack gap (01); brand-panel text group gap |
| 28 | sidebar top/bottom padding (`28px 16px`); balance-card vertical padding |
| 32 | sidebar section gap; balance-card horizontal padding; page top padding; 07 figure padding |
| 40 | page horizontal padding; brand panel gap |
| 48 | page bottom padding; brand panel vertical padding |
| 56 | brand panel horizontal padding; right panel padding (01) |

### 3.2 App shell (02–08)

**Sidebar**
- `flex: 1 1 232px`, `padding: 28px 16px`, `border-right: 1px solid #2A3A3F`, background `#172124`, `box-sizing: border-box`.
- Column with `gap: 32px`: logo, nav, then the optional network panel pushed down with `margin-top: auto`.
- **Derived/measured** at 1440: 232.6 px wide. It takes 1/1000 of the 648 px free space, because main grows 999. Nav items are 199.6 px wide.

**Main**
- `flex: 999 1 560px`, `padding: 32px 40px 48px`.
- Inner column `max-width: 1240px; margin: 0 auto`, stack `gap: 24px` (02, 03) or `gap: 20px` (04–08).
- **Derived/measured** at 1440: main is 1207.4 px and the content column 1127.4 px, starting at x = 272.6. The column never reaches its max-width.

**Card header row**
- `display: flex; justify-content: space-between; align-items: center`.
- Section label on the left; a 13 px link or 12 px muted meta on the right.

### 3.3 Card and container padding

| Container | Radius | Padding | Inner gap |
|---|---|---|---|
| Standard card | `20px` | `24px` | 12–20 (05 About 12, 03 summary 14, 05 accounts 16, 04 detail 18, 03 transaction 20) |
| List or table card with header (02 accounts, 02 activity) | `20px` | `20px 24px 8px` | `8px` |
| 08 address-forms card | `20px` | `20px 24px 8px` | none; the label has `margin: 0 0 6px` |
| Table card (07 transactions, with pagination; 08 history, without) | `20px` | `20px 24px 12px` | `8px` |
| 02 network card | `20px` | `20px 24px` | `16px` |
| 06 latest blocks | `20px` | `20px 20px 8px` | `8px` |
| 06 side cards | `20px` | `20px` | `8px` / `12px` |
| 04 transaction list | `20px` | `8px 8px` | — |
| 07 field list | `20px` | `8px 24px` | — |
| Hero balance card (02) | `24px` | `28px 32px` | `18px` |
| One-time keys card (02) | `24px` | `24px` | `14px` |
| Poem figure (07) | `24px` | `32px` | `14px` |
| Stat tile | `16px` | `16px` | `6px` |
| Node info tile (05) | `12px` | `12px` | `4px` |
| Radio card (01) | `16px` | `18px` | `14px` |
| Green callout (03) | `20px` | `18px` | `14px` |
| Green callout (04) | `14px` | `14px` | `12px` |
| Amber callout (05) | `16px` | `18px` | `16px` |
| Receipt destinations box (04) | `14px` | `2px 14px` | — |
| Sidebar network panel | `14px` | `14px` | `8px` |

### 3.4 Row heights

| Row | Height |
|---|---|
| Nav item | `height: 44px` |
| 02 one-time-keys rows | `min-height: 46px` + 1 px divider (**measured** 47; last row 46) |
| 02 recent-activity rows | `min-height: 60px` + 1 px divider (**measured** pitch 61) |
| 02 accounts table rows | 14 + 36 (icon button) + 14 + 1 = **measured** 65 (last row 64.5, no divider) |
| 03 destination rows | 44 px (the address field, §5.5), `border-spacing: 0 8px` (**measured** pitch 52) |
| 04 activity table rows | `padding: 14px` with two-line cell (**measured** 65; selected row 64) |
| 04 receipt destination rows | `min-height: 52px` (+1 divider on the first) |
| 05 accounts table rows | **measured** 49 |
| 05 keystore rows | `min-height: 52px` + 1 px divider |
| 05 display rows | `min-height: 48px` + 1 px divider |
| 06 blocks table rows | 12 px vertical padding around a 22 px tag (**measured** 47) |
| 06 mempool rows | `min-height: 52px` + 1 px divider, card `gap: 8px` (**measured** pitch 61) |
| 07 field rows | `padding: 14px 0` (**measured** 49) |
| 07, 08 table rows | 12 px vertical padding (**measured** 45; the 07 row with the "Yours" badge 45.5) |
| 08 address-form rows | **measured** 68 / 85 / 84 (the captions of rows 2 and 3 wrap in the 200 px column) |
| Table header (02) | 10 + 18 (12 px text) + 10 + 1 divider = **derived** 39 |

### 3.5 Per-screen grid (derived at 1440 px)

| Screen | Layout | Widths |
|---|---|---|
| 01 | Two panels, `flex: 1 1 520px` each, `flex-wrap: wrap` | 720 + 720. Right content `max-width: 480px`, centred both ways. |
| 02 | Row 1: balance `flex: 2 1 560px` + keys `flex: 1 1 320px`, gap 20. Row 2: accounts full width. Row 3: activity `2 1 560px` + network `1 1 320px`. | 711.6 + 395.8. The 02 network mini-stats are a 2-column grid, `gap: 16px 12px`. The 02 last-6 tiles are `repeat(6, minmax(0, 1fr))`, `gap: 6px` (52.6 px each). |
| 03 | Transaction `flex: 2 1 620px` + summary aside `flex: 1 1 340px`, gap 20, `align-items: flex-start` | ≈ 718.2 + 389.1. The aside stacks three cards with `gap: 16px`. |
| 04 | Toolbar row (tabs left, 320 px search right, `gap: 12px`). List `flex: 3 1 560px` + detail `flex: 2 1 400px`, gap 20, `align-items: flex-start`. | ≈ 648.4 + 458.9 |
| 05 | Accounts card full width. Then `repeat(auto-fit, minmax(min(420px, 100%), 1fr))`, `gap: 20px`. | Two columns of 553.7 (Keystore and node; Display and About). |
| 06 | Search bar full width. Stats `repeat(auto-fit, minmax(min(170px, 100%), 1fr))`, `gap: 12px`. Latest blocks `flex: 3 1 600px` + side column `flex: 2 1 340px` (two cards, `gap: 20px`), gap 20. | Stats: 6 × 177.9. Main row ≈ 700.4 + 406.9. |
| 07 | Poem figure `flex: 1 1 420px` + summary grid `flex: 1 1 420px`, gap 20. Summary grid `repeat(auto-fit, minmax(min(180px, 100%), 1fr))`, gap 12. Fields and transactions full width. | 553.7 + 553.7. Summary grid: 2 × 270.8, three rows. |
| 08 | Stats `repeat(auto-fit, minmax(min(200px, 100%), 1fr))`, gap 12. Forms and history full width. | Stats: 4 × 272.8 (the empty fifth track collapses). |

**Key–value grids**
- `120px minmax(0, 1fr)`, `gap: 10px 12px` (04).
- `160px minmax(0, 1fr)`, `gap: 10px 12px` (05 About).
- `180px minmax(0, 1fr)`, no gap (07).
- `200px minmax(0, 1fr)`, no gap (08).
- `repeat(2, minmax(0, 1fr))`, `gap: 12px` (05 node).
- Progress steps: `repeat(3, minmax(0, 1fr))`, `gap: 8px`.

### 3.6 Table columns

The tables use automatic layout, so these positions are **measured** at 1440. They are layout positions measured from the table's left edge:
- for a left-aligned column, the x where the cell's content box starts;
- for a right-aligned column, "→N", the content box's right edge. The last glyph's ink ends 1–2 px inside it.

| Table | Inner width | Columns |
|---|---|---|
| 02 Accounts (`min-width: 760px`) | 1077 | Account 0 · Address 166 · Balance →600 · Next key 624 · Status 782 · actions →1077 |
| 03 Destinations (`min-width: 640px`) | 668 | `#` `width: 28px` (0–28) · address field 36–288 · amount `width: 150px` (field 304–454) · reference `width: 150px` (field 470–620) · remove `width: 40px` (628–668) |
| 04 Transactions (`min-width: 620px`) | 630 | Date 14 · Description 127 · Account 350 · Amount →616 |
| 05 Accounts (`min-width: 720px`) | 1077 | Account 0 · Tag 211 · On this device 519 · On ledger 717 · Status 852 |
| 06 Latest blocks (`min-width: 640px`) | 658 | Block 0 · Type 99 · Txs →291 · Solve 307 · Miner 403 · Reward →598 · Age →658 |
| 07 Transactions (`min-width: 720px`) | 1077 | ID 0 · From tag 364 · Destinations →728 · Amount →924 · Fee →1077 |
| 08 History (`min-width: 720px`) | 1077 | Block 0 · Transaction 123 · Counterparty 338 · Reference 698 · Amount →1077 |

**Outer cell padding**
- On 02 and 05–08 the first and last cells use zero outer padding, so table text aligns with the card padding. Examples: `padding: 14px 0` on 02 and 05; `12px 8px 12px 0` and `12px 0 12px 8px` on 06–08.
- 04 pads every cell. Its table sits inside an 8 px card padding.
- In 03, the `#` and remove columns have no set padding.

---

## 4. Radii, borders, elevation, opacity

### 4.1 Corner radii

| Token | Value | Used for |
|---|---|---|
| `radius_2` | `2px` | Progress bars (4 px tall), legend swatches |
| `radius_3`, `radius_4` | `3px`, `4px` | 6 px and 8 px status dots |
| `radius_8` | `8px` | Segment inside a segmented control |
| `radius_10` | `10px` | Controls 36–44 px tall: secondary and icon buttons, inputs, select, segmented track, 03 address field, block tiles, compact and 40 px primary buttons, tonal "Test" |
| `radius_12` | `12px` | Nav items, 44 px header controls, the 42 px 06 "Search" button, account selector, password field, dashed button, 36–40 px icon tiles, node info tiles, selected table row |
| `radius_14` | `14px` | 56/52 px primary buttons, sidebar panel, receipt box, receipt callout |
| `radius_16` | `16px` | Stat tiles, radio cards, explorer search bar, amber callout |
| `radius_20` | `20px` | Standard cards, 03 green callout |
| `radius_24` | `24px` | Hero cards (02 balance and keys, 07 figure) |
| `radius_pill` | half the height | `9px` (inline badges), `10px` (20 px tags), `11px` (22 px), `13px` (26 px), `14px` (28 px), `15px` (30 px), `16px` (the "Live" chip), `18px` (36 px pills and the 36 px icon circle) |

**Partial radii:** `12px 0 0 12px` and `0 12px 12px 0`.
- On 05 they are set only on the warning row's first and last cells.
- On 04 they are set on the first and last cells of every row, but only the selected row's fill shows them.

### 4.2 Borders

- **Defaults:** `1px solid #2A3A3F` on surfaces and `1px solid #2E4045` on controls. Callouts use `#1F5A49` or `#5A4A22`.
- **Declared 1.5 px** on:
  - the selected radio card (`#15DC96`);
  - the 02 "Receive" header button (`#3A4D52`);
  - the 06 search bar and the 03 password field (`#2E4045`);
  - the tip tile (`#15DC96`);
  - the dashed add button (`1.5px dashed #3A4D52`).
- **Rendered at 1 px.** **Measured:** at scale factor 1, Chromium computes and draws every one of these borders at 1 px. The PNGs therefore show 1 px lines, and the selected radio card has the same box size as the unselected ones.
- **Inset ring.** The selected-row ring (`box-shadow: inset 0 0 0 1.5px #15DC96`) is not snapped. It draws one solid green pixel plus one blended pixel.
- **Content-box sizing.** Three bordered elements set a fixed `height` without `box-sizing: border-box`, so the border adds to it (**measured**):
  - the 06 search bar, 56 → 58;
  - the 06 "Live" chip, 32 → 34;
  - the 03 address field, 42 → 44.

### 4.3 Elevation

There are no drop shadows anywhere; the design is flat. The only `box-shadow` is the inset ring on the selected 04 row, which acts as an inner border.

Layering comes from fills alone, darkest to lightest:
1. `#172124` (surface);
2. `#1D2A2E` (page and inset fields);
3. `#24353A` (raised).

### 4.4 Opacity

Two opacity values appear, in three places:
- `opacity: 0.16`: the 01 pattern layer.
- `opacity: 0.10`: the watermark marks in 02 and 07.

No other translucency is used.

---

## 5. Components

### 5.1 Buttons

| Variant | Height | Padding | Radius | Fill / border | Text | Icon | Where |
|---|---|---|---|---|---|---|---|
| Primary XL | `56px` | full width, centred | `14px` | `#15DC96` | Montserrat `15px` `700` `#1D2A2E` | — | 01 "Continue" |
| Primary L | `52px` | full width | `14px` | `#15DC96` | Montserrat 15/700 | — | 03 "Sign & submit" |
| Primary header | `44px` | `0 20px` | `12px` | `#15DC96` | Montserrat `14px` `700` | 18 px, stroke `2.4`, gap 8 | 02 "Send" |
| Primary search | `42px` | `0 20px` | `12px` | `#15DC96`, `border: 0` | Montserrat 14/700 | — | 06 "Search" |
| Primary M | `40px` | `0 16px` | `10px` | `#15DC96` | Montserrat `13px` `700` | — | 08 "Send to this tag" |
| Primary compact | `38px` | `0 14px` | `10px` | `#15DC96`, `border: 0` | Poppins `13px` `600` | 15 px, `2.2`, gap 6 | 04 "Download" |
| Warning primary | `40px` | `0 16px` | `10px` | `#F5B841`, `border: 0` | Montserrat 13/700 `#1D2A2E` | — | 05 "Advance to #33" |
| Outline header | `44px` | `0 18px` | `12px` | transparent, `1.5px solid #3A4D52` | Montserrat 14/`600` `#FFFFFF` | 18 px, `2.2`, gap 8 | 02 "Receive" |
| Secondary | `40px` | `0 14px` | `10px` | transparent, `1px solid #2E4045` | Poppins 13/`500` white | 16 px, `2`, gap 8 | 03 "Import CSV", "Scan QR"; 04 "Export CSV"; 08 "Copy" |
| Secondary 38 | `38px` | `0 14px` | `10px` | same | 13/500 | 15 px, `2`, gap 8 | 05 "Check again" |
| Prev/next block | `40px` | `0 14px 0 10px` / `0 10px 0 14px` | `10px` | same | 13/500 | chevron 16 px, gap 6 | 07 |
| Small outline | `36px` | `0 14px` | `10px` | same | 13 px, `500` ("Discover tags") or unset, so 400 ("Change", "Show", "Export", "Previous", "Next") | — | 02, 05, 07 |
| Tonal | `36px` / `44px` | `0 14px` | `10px` | `#24353A`, `border: 0` | 13/500 white | 14 px, `2.4`, gap 6 (02 only) | 02 "New account" (36, with plus); 05 "Test" (44, no icon, `white-space: nowrap`) |
| Warning tonal | `36px` | `0 12px` | `10px` | `#3D3520` | Poppins `12px` `600` `#F5B841` | — | 02 "Reconcile" |
| Warning outline | `40px` | `0 14px` | `10px` | transparent, `1px solid #5A4A22` | 13/500 `#F2E3BF` | — | 05 "Inspect tag" |
| Dashed add | `46px` | full width | `12px` | transparent, `1.5px dashed #3A4D52` | Poppins 13/`600` `#15DC96` | plus 16 px, `2.4`, gap 8 | 03 "Add destination" |
| Icon button | `36px` × `36px` | — | `10px` | transparent, `1px solid #2E4045` | colour `#B4C2C5` | copy 16 px, `2` | 02 table |
| Ghost icon button | 36 × 36 | — | `10px` | transparent, `border: 0` | `#8FA0A4` | trash 16 px | 03 remove row |
| Lock button | `36px` | full width, centred | `10px` | transparent, `1px solid #2E4045` | Poppins `12px` `500` white | lock 14 px, `2`, gap 8 | 02 sidebar, `margin-top: 4px` |

### 5.2 Text link

- `color: #15DC96`, no underline; hover `#5FF0BC`.
- Header links are Poppins `13px` `500` ("View all", "Open explorer", "All blocks").
- "View all pending" also sets `min-height: 36px`.
- 01 "Change node" is `500` at 12 px.
- "Review →" is `12px` `600` in `#F5B841`, overriding the link colour.
- Mono links (block numbers, hashes, miner) keep the link green.

### 5.3 Nav item

- `height: 44px; border-radius: 12px; padding: 0 12px; gap: 12px`.
- Icon 20 px, stroke `2`.
- Text Poppins `14px`.
- **Default:** no fill, `#B4C2C5`, `500`.
- **Active:** `background: #173B33`, `#15DC96`, `600`.
- Nav column gap `4px`.
- The frames define no hover state.

### 5.4 Chips and badges

| Variant | Size | Padding | Radius | Fill | Text | Where |
|---|---|---|---|---|---|---|
| Feature pill | `36px` | `0 14px` | `18px` | `#1D2A2E` | 13/500 `#FFFFFF` | 01, wrap with gap 10 (**measured**: two rows at 1440) |
| Recommended | inline | `2px 8px` | `9px` | `#15DC96` | `11px` `600` `#1D2A2E` | 01 radio title |
| Default | inline | `2px 7px` | `9px` | `#15DC96` | 11/600 `#1D2A2E`, `margin-left: 6px` | 02 table |
| Yours | `20px` | `0 8px` | `10px` | `#15DC96` | 11/600 `#1D2A2E`, 8 px after the ID | 07 table |
| Header tag, accent | `26px` | `0 10px` | `13px` | `#15DC96` | 12/600 `#1D2A2E` | 08 "Your account · Primary" |
| Header tag, neutral | `26px` | `0 10px` | `13px` | `#24353A` | 12/600 `#B4C2C5` | 07 "Normal" |
| Neutral count | `30px` | `0 12px` | `15px` | `#24353A` | 12/500 `#B4C2C5` | 02 "3 accounts" |
| Warning | `30px` | `0 12px` | `15px` | `#3D3520` | 12/500 `#F5B841`, clock 13 px `2.4`, gap 6 | 02 settling |
| Status: Settled | `28px` | `0 12px` | `14px` | `#173B33` | 12/600 `#15DC96`, check 13 px `3`, gap 6 | 04 |
| Live | `32px` content-box (34 rendered) | `0 12px` | `16px` | `#172124` + `1px solid #2A3A3F` | 12/500 `#B4C2C5`, 8 px green dot, gap 8 | 06 |
| Block-type tag (table) | `22px` | `0 9px` | `11px` | per §1.7 | 11/600 | 06 |
| Block-type tag (legend) | `20px` | `0 8px` | `10px` | per §1.7 | 11/600, fixed `width: 76px`, centred | 06 |
| Dot + text status | inline, gap 6 | — | dot `3px` | dot 6 × 6 | 13 px in status colour | 02 table |

### 5.5 Form fields

All fields in this table:
- fill `#1D2A2E`;
- set `outline: none`;
- have no focus style;
- set no placeholder colour.

| Variant | Height | Radius | Border | Padding | Font |
|---|---|---|---|---|---|
| Address with check (03) | `42px` content-box container, 44 rendered | `10px` | `1px solid #2E4045` | `0 10px 0 12px`, gap 8 | IBM Plex Mono 13 px white, on a transparent borderless input; trailing tick 16 px, stroke `#15DC96`, width `2.6` |
| Amount (03) | `42px` (`box-sizing: border-box`) | `10px` | `1px solid #2E4045` | `0 12px` | Montserrat 13/600, `text-align: right`, tabular-nums |
| Reference (03) | `42px` (border-box) | `10px` | `1px solid #2E4045` | `0 12px` | IBM Plex Mono 13 px |
| Endpoint (05) | `44px` (border-box) | `10px` | `1px solid #2E4045` | `0 12px` | IBM Plex Mono 13 px; followed by the 44 px tonal "Test" button, gap 8 |
| Password (03) | `46px` (border-box) | `12px` | `1.5px solid #2E4045` (1 px rendered) | `0 14px` | Poppins `15px`, `letter-spacing: 0.08em`, masked |

- **Address field height.** The address field renders 2 px taller than the amount and reference inputs beside it.
- **Labels.** Field labels are Poppins 12/500 `#B4C2C5`.
- **Label gap.** The label-to-field gap is `8px` for 03 "From account" and `6px` elsewhere.
- **Helper text.** Helper text is 12 px `#8FA0A4`.

### 5.6 Select (05 "Auto-lock")

- `height: 36px; border-radius: 10px; border: 1px solid #2E4045; background: #1D2A2E; color: #FFFFFF; padding: 0 10px; font-size: 13px`. It inherits Poppins through `input,select{font:inherit}`.
- **Measured** box: 143 × 36.
- The arrow is the native control; the rendering shows a white chevron at the right.
- `outline: none` is not set.

### 5.7 Account selector (03)

- A button `56px` tall: `border-radius: 12px`, `1px solid #2E4045`, fill `#1D2A2E`, `padding: 0 16px`, `gap: 14px`, `text-align: left`.
- **Left:** name 14/500 over a mono 12 px `#8FA0A4` address.
- **Right, end-aligned:** Montserrat `15px` `600` "3,180.000000 MCM" (unit in the same style, no tabular-nums), over 12 px `#8FA0A4` "next key #N" with the index in mono.
- **Trailing:** chevron-down, 18 px, stroke `#8FA0A4`.

### 5.8 Search fields

| Where | Height | Width | Radius | Fill / border | Padding, gap | Icon | Input |
|---|---|---|---|---|---|---|---|
| 02 header | `44px` (border-box) | `300px` | `12px` | `#172124`, `1px solid #2E4045` | `0 14px`, gap 10 | 16 px, stroke `#8FA0A4` | 13 px |
| 04 toolbar | `40px` (border-box) | `320px` | `10px` | same | `0 12px`, gap 10 | 16 px | 13 px |
| 06 explorer | `56px` content-box, 58 rendered | full width | `16px` | `#172124`, `1.5px solid #2E4045` (1 px rendered) | `0 8px 0 18px`, gap 12 | 20 px | 15 px, then the 42 px primary "Search" button |

The input inside each one is transparent and `border: 0`, with `outline: none` and white text.

### 5.9 Segmented control (05 "Amounts" MCM/nanoMCM, "Theme" Dark/Light/System)

- **Track:** `height: 36px; border-radius: 10px; background: #1D2A2E; padding: 3px; gap: 2px; box-sizing: border-box`.
- **Segments:** `padding: 0 14px; border-radius: 8px; border: 0`. Height is 30 px (**derived**, **measured**).
- **Selected:** `#15DC96` fill, `#1D2A2E` text, 12/600.
- **Unselected:** transparent, `#B4C2C5`, 12/500.

### 5.10 Filter tabs (04)

- Pills `height: 36px; padding: 0 16px; border-radius: 18px`, gap 6, wrapping.
- **Selected:** `#15DC96`, `border: 0`, `#1D2A2E` 13/600.
- **Unselected:** transparent, `1px solid #2E4045`, `#B4C2C5` 13/500.

### 5.11 Radio card (01)

- `border-radius: 16px; background: #172124; padding: 18px; gap: 14px; align-items: flex-start`.
- Cards are stacked with `gap: 12px`. **Measured** heights are 82.5 for a one-line description and 100 for two lines.
- **Selected:** `border: 1.5px solid #15DC96` (1 px rendered). **Unselected:** `border: 1px solid #2A3A3F`.
- **Radio:** native, `20px` × `20px`, `accent-color: #15DC96`, `margin: 2px 0 0`.
  - **Checked (measured):** a green ring and a green dot, with a dark `#3B3B3B` gap between them.
  - **Unchecked (measured):** a white disc.
- The text column has gap 4. The title row has gap 8 and holds the Recommended badge.

### 5.12 Tables

- **Font:** body 13 px; header row 12 px, `#8FA0A4`, weight 500, left-aligned. Numeric columns use `text-align: right`.
- **Header cell padding:**
  - 02: `10px 12px`, with `10px 0` on the first and last cells;
  - 05: `8px 12px`, with `8px 0` on the first and last;
  - 06, 07, 08: `10px 8px`, with `10px 8px 10px 0` first and `10px 0 10px 8px` last;
  - 04: `12px 14px` (Date, Amount) and `12px 10px` (others);
  - 03: `0 8px 2px`, with `0 0 2px` on the `#` and remove columns.
- **Header rule:** `border-bottom: 1px solid #2A3A3F` on 02, 05, 06, 07 and 08. Tables 03 and 04 have no header rule.
- **Body cell padding:**
  - 02, 05: `14px 12px`, with `14px 0` first and last;
  - 06, 07, 08: `12px 8px`, with `12px 8px 12px 0` first and `12px 0 12px 8px` last;
  - 04: `14px` (Date, Amount) and `14px 10px` (others).
- **Dividers:** `border-bottom: 1px solid #2A3A3F`.
  - The last row has no divider on 02 and 05.
  - It keeps one on 06, 07 and 08.
  - On 04, every unselected row has one.
- **Selected row (04):** `#173B33` with a `1.5px` `#15DC96` inset ring and radius 12. The Date and Amount cells use `white-space: nowrap`.
- **Own row (07):** `#173B33` fill only.
- **Warning row (05):** `#2A2A1F` with 12 px end radii and an extra `12px` of inner padding at both ends. Its "On ledger" index and status cells are `#F5B841`.
- **03 destinations:** `border-collapse: separate; border-spacing: 0 8px`. The row number is Montserrat 600 `#8FA0A4`.

### 5.13 Stat tile

- `border-radius: 16px; background: #172124; border: 1px solid #2A3A3F; padding: 16px; gap: 6px`.
- Label 12 px `#8FA0A4`; value as in §2.4.
- **Measured:** 110 px tall on 06 (the "Next neogenesis" tile has a sub-line) and 85 px on 07 and 08.
- **Highlighted (06 "Next neogenesis"):** fill `#173B33`, border `#1F5A49`, label `#CFEFE2`, value `#15DC96`, sub-line `11px` `#CFEFE2`.
- **02 network mini-stats:** no box; a 12 px muted label over Montserrat 20/700, gap 2.
- **05 node tiles:**
  - `#1D2A2E`, radius 12, padding 12, gap 4, no border;
  - dt 12 px muted over a 13 px dd (mono, or `#15DC96` for status);
  - **measured** 66 px tall.

### 5.14 Progress steps (Reserve, Submit, Settle)

- Grid of 3, gap 8, text 12 px.
- Each step is a column with `gap: 6px` that starts with a bar: `height: 4px; border-radius: 2px`.
- **Pending (03):**
  - bar `#2E4045`;
  - text block in `#8FA0A4` holding the label (Montserrat `600` white) and, after a `<br>`, the sub-line. The sub-line therefore follows without the 6 px gap.
- **Complete (04):**
  - bar `#15DC96`;
  - label Montserrat `700` as its own row;
  - sub-line `#8FA0A4` with a mono time.

### 5.15 Callouts

**Green info (03)**
- `#173B33`, `1px solid #1F5A49`, radius `20px`, padding 18, gap 14, top-aligned.
- Icon tile `40px`, radius 12, `#15DC96` fill, `#1D2A2E` key icon (20 px, `2.2`).
- Text gap 4. Title Montserrat 14/600; body 12 px, `line-height: 1.5`, `#CFEFE2`.

**Green receipt (04)**
- Same colours; radius `14px`, padding 14, gap 12, centre-aligned, wraps.
- Icon tile `36px`, radius 12, shield-check icon 18 px, `2.2`.
- Text block `flex: 1 1 180px`, gap 2; body 12 px `#CFEFE2` at normal line height.
- Trailing compact primary "Download".

**Amber warning (05)**
- `#2A2A1F`, `1px solid #5A4A22`, radius `16px`, padding 18, gap 16, centre-aligned, wraps.
- Icon tile 40 px, radius 12, `#3D3520` fill, `#F5B841` triangle (20 px, `2`).
- Text block `flex: 1 1 360px`, gap 4. Title 14/600 white; body 12 px / 1.5 `#F2E3BF`.
- Actions gap 10: warning outline, then warning primary.

### 5.16 Lists

**Activity row (02)**
- `min-height: 60px; gap: 14px`, divider except on the last row.
- Icon circle `36px`, `border-radius: 18px`, glyph 16 px, stroke `2.2`:
  - in: `#173B33` / `#15DC96`;
  - pending: `#3D3520` / `#F5B841`;
  - out or move: `#24353A` / `#FFFFFF`.
- Title 14/500 over a 12 px muted sub-line, gap 2.
- Status 12/500, `width: 84px`.
- Amount Montserrat 14/600, `width: 150px`, right-aligned.

**One-time keys (02)**
- Rows `min-height: 46px`, gap 12, 13 px.
- Name `flex: 1`; mono index; status 12 px, `width: 92px`, right-aligned.

**Mempool (06)**
- Rows `min-height: 52px`, gap 12, divider on every row.
- Mono 13 px ID over 11 px Poppins meta, gap 2; amount Montserrat 13/600.
- A "View all pending" link follows the rows.

**Receipt destinations (04)**
- Rows `min-height: 52px`, gap 12.
- Mono 13 px address over mono 11 px `#8FA0A4` reference; amount Montserrat 600.

**Block-type legend (06)**
- Rows gap 12, top-aligned, 12 px / 1.5 `#B4C2C5`, fixed-width tag.

**Last 6 blocks (02)**
- Label 12 px muted, then tiles (gap 8).
- Tiles `height: 44px; border-radius: 10px`, IBM Plex Mono `11px` (the neogenesis tile is 500), colours per §1.7.
- The tip tile adds `1.5px solid #15DC96` with `box-sizing: border-box`.
- Legend: 11 px `#8FA0A4`, gap 14; swatches `8px` square at radius `2px`, 6 px before each label.

### 5.17 Key–value lists

- **03 summary:**
  - rows `space-between`, gap 10, 13 px;
  - dt `#B4C2C5` (suffix `#8FA0A4`); dd Montserrat (unset weight, §2.2);
  - the Total row has `border-top: 1px solid #2A3A3F; padding-top: 12px`, is baseline-aligned, and its dt is 500 white.
- **04 and 05 About:** dt `#8FA0A4`, 13 px. The 04 transaction ID dd uses `word-break: break-all`.
- **07:** dt `#8FA0A4`, `padding: 14px 0` with dividers except on the last row; dd mono, `word-break: break-all`.
- **08:**
  - dt is a 500 title over an 11 px muted caption, gap 2;
  - dd is mono, `align-self: center`, `word-break: break-all`;
  - the ledger address dd is two-toned: the first 40 hex digits (the tag) `#FFFFFF`, the last 40 (the key hash) `#15DC96`.

### 5.18 Pagination (07)

- Right-aligned, gap 8, `padding-top: 6px`.
- Two small outline buttons: "Previous" in `#8FA0A4`, "Next" in `#FFFFFF`.
- The range label "1–6 of 211" sits in the card header, 12 px `#8FA0A4`.

### 5.19 Breadcrumbs (07, 08)

- 13 px `#8FA0A4`, gap 8, vertically centred.
- The first crumb is a green link; separators are `/`; the middle crumb is muted; the current crumb is `#FFFFFF` (mono on 08).
- The breadcrumb sits above the page header.

### 5.20 Sidebar network panel (02, 03)

- `margin-top: auto; border-radius: 14px; background: #1D2A2E; border: 1px solid #2A3A3F; padding: 14px; gap: 8px; font-size: 12px; color: #B4C2C5`.
- Title row: white, 500, gap 8, 8 px green dot.
- Two `space-between` rows, each a label and a mono white value.
- Lock button (§5.1) on 02 only.

### 5.21 Page header

- On 02–06 the h1 sits over a subtitle, gap 2.
- On 08 the h1 sits beside a header tag (gap 10), above a 20 px mono tag (column gap 6).
- On 07 the h1, type tag and meta share one row (gap 14). There is no subtitle.

### 5.22 Balance hero (02) and poem figure (07)

**Balance hero**
- Baseline-aligned Montserrat 700 row: integer, decimals, unit (§2.4).
- Then the nanoMCM line (gap 6), then the chip row (gap 10).

**Poem figure**
- `justify-content: center`, gap 14.
- Caption: section-label style in `#15DC96`. It names the block's poem, decoded from the nonce.
- Three-line poem: Montserrat 28 px, 500 italic, `line-height: 1.45`.

### 5.23 Node status footer (01)

- `border-top: 1px solid #2A3A3F; padding-top: 16px`, `space-between`, gap 12, wraps, 12 px `#8FA0A4`.
- **Left:** an 8 px green dot (gap 8), "Mesh API node", then the mono endpoint in `#B4C2C5`.
- **Right:** the "Change node" link, 12 px, 500.

---

## 6. Icons

- Every UI icon is an inline SVG line icon with `viewBox="0 0 24 24"`, written `sc-camel-view-box` in the templates. There are 71 instances.
- **Attributes:** `fill="none"`, `stroke-linecap="round"`, `stroke-linejoin="round"`, `stroke="currentColor"`. Three icons use an explicit stroke colour instead: search, chevron-down and the address tick, five instances in all.
- **Stroke widths:** `2` (default), `2.2`, `2.4`, `2.6` or `3`.
- **Rendered sizes:** 13, 14, 15, 16, 18 and 20 px.
- **Draw from these paths.** Some shapes match common open-source line-icon sets (sliders, activity, copy and lock are identical to Feather's). Others are simplified or redrawn: the wallet, cube, server, swap and warning. Draw every icon from these paths rather than substituting a library glyph.
- `h.01` segments rely on round caps to draw as dots.

| # | Icon | Path data | Sizes / stroke | Screens |
|---|---|---|---|---|
| 1 | wallet | `rect x2 y5 w20 h15 rx3` + `M2 10h20M16 15h2` | 20 / 2 | nav, all sidebar screens |
| 2 | arrow-up-right (send) | `M7 17 17 7M7 7h10v10` | 20/2 nav; 18/2.4 header Send; 16/2.2 outgoing and pending rows | 02–08 |
| 3 | arrow-down-left (receive) | `M17 7 7 17M17 17H7V7` | 20/2 nav; 18/2.2 header Receive; 16/2.2 incoming rows | 02–08 |
| 4 | activity | `M22 12h-4l-3 9L9 3l-3 9H2` | 20 / 2 | nav |
| 5 | cube (explorer) | `M21 8 12 3 3 8v8l9 5 9-5V8z` + `m3 8 9 5 9-5M12 13v8` | 20 / 2 | nav |
| 6 | sliders (settings) | `M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6` | 20 / 2 | nav |
| 7 | lock | `rect x3 y11 w18 h11 rx2` + `M7 11V7a5 5 0 0 1 10 0v4` | 14/2 lock button (02); 20/2.2 keystore tile (05) | 02, 05 |
| 8 | search | `circle cx11 cy11 r8` + `m21 21-4.3-4.3`, stroke `#8FA0A4` | 16/2 (02, 04); 20/2 (06) | 02, 04, 06 |
| 9 | clock | `circle cx12 cy12 r10` + `M12 6v6l4 2` | 13 / 2.4 | 02 settling chip |
| 10 | plus | `M12 5v14M5 12h14` | 14/2.4; 16/2.4 | 02 New account, 03 Add destination |
| 11 | copy | `rect x9 y9 w13 h13 rx2` + `M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1` | 16 / 2 | 02 table, 08 Copy |
| 12 | swap | `M17 3l4 4-4 4M3 7h18M7 21l-4-4 4-4M21 17H3` | 16 / 2.2 | 02 own-account row |
| 13 | file-upload | `M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z` + `M14 2v6h6M12 18v-6M9 15l3-3 3 3` | 16 / 2 | 03 Import CSV |
| 14 | scan | `M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2M7 12h10` | 16 / 2 | 03 Scan QR |
| 15 | chevron-down | `m6 9 6 6 6-6`, stroke `#8FA0A4` | 18 / 2 | 03 account selector |
| 16 | check | `M20 6 9 17l-5-5` | 16/2.6 stroke `#15DC96` (03 address); 13/3 (04 Settled chip) | 03, 04 |
| 17 | trash | `M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6` | 16 / 2 | 03 remove destination |
| 18 | key | `circle cx7.5 cy15.5 r5.5` + `m21 2-9.6 9.6M15.5 7.5l3 3L22 7l-3-3` | 20 / 2.2 | 03 callout |
| 19 | download | `M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3` | 16/2 Export CSV; 15/2.2 Download | 04 |
| 20 | shield-check | `M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z` + `m9 12 2 2 4-4` | 18 / 2.2 | 04 receipt callout |
| 21 | refresh | `M21 12a9 9 0 1 1-3-6.7L21 8` + `M21 3v5h-5` | 15 / 2 | 05 Check again |
| 22 | warning | `M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z` + `M12 9v4M12 17h.01` | 20 / 2 | 05 amber callout |
| 23 | server | `rect x2 y3 w20 h8 rx2` + `rect x2 y13 w20 h8 rx2` + `M6 7h.01M6 17h.01` | 20 / 2 | 05 node tile |
| 24 | chevron-left | `m15 18-6-6 6-6` | 16 / 2 | 07 previous block |
| 25 | chevron-right | `m9 18 6-6-6-6` | 16 / 2 | 07 next block |

Icons take the text colour of their parent (`currentColor`). The exceptions are search (`#8FA0A4`), chevron-down (`#8FA0A4`) and the address tick (`#15DC96`). The repository's `Cargo.toml` notes that iced's `svg` feature is added by the change that first draws these icons.

---

## 7. Logo, branding marks, pattern

**The mark.** A coin: a circular ring enclosing a rounded frame of three vertical bars on a base, with a slanted stroke joining the frame's top to the ring just left of centre, and a lens-shaped segment along the bottom of the ring. It exists in two coordinate spaces:
- inside the full lockup, `viewBox 0 0 311 72`, where the mark occupies x ≈ 0.75–71.8, y ≈ 1.0–70.3;
- standalone, `viewBox 0 0 31.6 31`, where it spans x 0.25–31.38, y 0.29–30.62.

**Lockups**

| Use | Size | viewBox | Paths and fills | Where |
|---|---|---|---|---|
| Full lockup with tagline | `width="236" height="55"` | `0 0 311 72` | Mark, "MOCHIMO" wordmark (x ≈ 87–310, y ≈ 15–48) and "Post-Quantum Currency" tagline (x ≈ 112–310, y ≈ 53–68), all `fill="#1D2A2E"` | 01 brand panel, top-left (**measured** at 56, 48) |
| Sidebar lockup | `width="140" height="32"`, `margin-left: 10px` | `0 0 311 72` | Mark `fill="#15DC96"`, wordmark `fill="#FFFFFF"`, no tagline | 02–08 sidebar top. **Derived:** default meet scaling gives 138.2 × 32, centred. |
| Watermark | `360` × `353` | `0 0 31.6 31` | Mark `fill="#15DC96"`, `opacity: 0.10`, `right: -70px; top: -90px`, clipped by the card (`overflow: hidden`, radius 24) | 02 balance card |
| Watermark | `300` × `294` | same | Same fill and opacity, `right: -60px; bottom: -80px` | 07 poem figure |

The vector paths are long and are not reproduced here. Take them from the owner's source bundle:
- the 311 × 72 SVG in 01's brand panel for the lockup with tagline, or any sidebar for the mark and wordmark;
- the 31.6 × 31 SVG in the 02 balance card or the 07 figure. The same path is used in the 01 pattern.

The frames carry the MOCHIMO wordmark. `design/INDEX.md` records that branding is put to the owner before any screen is built.

**Brand panel (01)**
- `flex: 1 1 520px; min-height: 560px; padding: 48px 56px; background: #15DC96; color: #1D2A2E`.
- Column, `justify-content: space-between`, `gap: 40px`, `overflow: hidden`.
- Top: the full lockup.
- Bottom: a group (`gap: 24px`) holding the 52 px uppercase headline (second line italic) and the feature pills.

**Background pattern (01)**
- An absolutely positioned SVG covers the panel (`position: absolute; inset: 0; opacity: 0.16`). It is drawn beneath the panel content, which is `position: relative`.
- It defines a `<pattern>` 64 × 64 in user-space units (`patternUnits="userSpaceOnUse"`, written `sc-camel-pattern-units`), with its origin at the panel's top-left.
- Each tile holds the standalone mark path, `fill="#1D2A2E"`, with `transform="translate(18 18) scale(0.9)"`.
  - **Derived:** a ≈ 28 px mark spanning about x 18.2–46.2, y 18.3–45.6 of each tile.
  - The mark centres therefore sit on a 64 px grid starting at about (32, 32).
- A `rect` of 100% × 100% fills with the pattern.
- To rebuild it in iced, tile the mark at 64 px steps behind the content. Draw it in `#1D2A2E` at 16% alpha, or in the flat blend `#16C085` (**derived**).

---

## 8. Layout differences between screens

| Aspect | 01 | 02 | 03 | 04 | 05 | 06 | 07 | 08 |
|---|---|---|---|---|---|---|---|---|
| Shell | Split screen, no sidebar | sidebar | sidebar | sidebar | sidebar | sidebar | sidebar | sidebar |
| Active nav | — | Wallet | Send | Activity | Settings | Explorer | Explorer | Explorer |
| Sidebar network panel | — | yes, with "Lock wallet" | yes, no lock button | no | no | no | no | no |
| Page stack gap | 24 (right panel) | `24px` | `24px` | `20px` | `20px` | `20px` | `20px` | `20px` |
| Breadcrumb | — | — | — | — | — | — | yes | yes |
| Header alignment | — | `center` | `flex-end` | `flex-end` | no action row (column only) | `flex-end` | `center` | `center` |
| Header actions | — | search 300 px + outline Receive + primary Send (all 44 px) | Import CSV, Scan QR (40 px) | Export CSV (40 px), plus a filter and search toolbar row below | none | "Live" chip | prev/next block (40 px) | Copy + Send to this tag (40 px) |
| Title extras | 28 px h2 | — | — | — | — | — | mono number (500, not uppercased), Normal tag, meta | accent tag + 20 px mono tag line |

Further differences:
- The nav "Receive" item links to the dashboard; there is no Receive frame.
- The sidebar logo, nav items and order (Wallet, Send, Receive, Activity, Explorer, Settings) are identical on 02–08.

---

## 9. @font-face declarations

Every face declares `font-display: swap` and `format('woff2')`. Each frame declares 39 faces; 07 declares 44 because it adds Montserrat italic 500.

| Family | Style | Weights declared | Subsets declared | How the frames supply them |
|---|---|---|---|---|
| `'IBM Plex Mono'` | normal | 400, 500 | cyrillic-ext, cyrillic, vietnamese, latin-ext, latin | Only `latin` is embedded (static Regular and Medium). The other four point to `fonts.gstatic.com/s/ibmplexmono/v20/…`. |
| `'Montserrat'` | normal | 500, 600, 700 | cyrillic-ext, cyrillic, vietnamese, latin-ext, latin | All five embedded. Per subset, one file serves 500, 600 and 700: a variable font, `wght` axis 100–900, default instance Thin (100), named "Montserrat Thin". |
| `'Montserrat'` | italic | 700 (every frame); 500 (07 only) | same five | All embedded. On every frame except 07 the italic 700 file is a static instance, named "Montserrat Thin Bold Italic" with weight class 700. On 07, 500 and 700 share one variable italic file per subset ("Montserrat Thin Italic", `wght` 100–900, default 100). |
| `'Poppins'` | normal | 400, 500, 600 | devanagari, latin-ext, latin | Only `latin` is embedded (static Regular, Medium, SemiBold). devanagari and latin-ext point to `fonts.gstatic.com/s/poppins/v24/…`. |

Unicode ranges are identical across families and frames for the same subset:

| Subset | `unicode-range` |
|---|---|
| latin | `U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD` |
| latin-ext | `U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C0, U+2113, U+2C60-2C7F, U+A720-A7FF` |
| cyrillic-ext | `U+0460-052F, U+1C80-1C8A, U+20B4, U+2DE0-2DFF, U+A640-A69F, U+FE2E-FE2F` |
| cyrillic | `U+0301, U+0400-045F, U+0490-0491, U+04B0-04B1, U+2116` |
| vietnamese | `U+0102-0103, U+0110-0111, U+0128-0129, U+0168-0169, U+01A0-01A1, U+01AF-01B0, U+0300-0301, U+0303-0304, U+0308-0309, U+0323, U+0329, U+1EA0-1EF9, U+20AB` |
| devanagari (Poppins only) | `U+0900-097F, U+1CD0-1CF9, U+200C-200D, U+20A8, U+20B9, U+20F0, U+25CC, U+A830-A839, U+A8E0-A8FF, U+11B00-11B09` |

**Bundling notes for iced**

1. **Faces to bundle.** The text needs:
   - Poppins Regular, Medium, SemiBold;
   - Montserrat Medium, SemiBold, Bold, Medium Italic, Bold Italic;
   - IBM Plex Mono Regular, Medium.

   Prefer static TTF/OTF instances. iced's font loader takes TrueType/OpenType bytes, not WOFF2. A variable Montserrat file whose axis is not applied would draw its default Thin instance.
2. **Coverage.** Beyond ASCII, the screens use these characters:
   - `…` U+2026, `·` U+00B7, `−` U+2212, `—` U+2014, `–` U+2013, `’` U+2019, `×` U+00D7;
   - `→` U+2192.

   The latin subsets cover all of them except **`→` U+2192**. It falls outside every declared range, and no bundled file contains it, so the browser draws it from a system fallback. It appears in "Review →", "Primary → Savings" (02, 04) and "#48 → #49" (04). Bundle font files that contain it, or provide a fallback face.
3. **Licence.** All three families are distributed under the SIL Open Font License 1.1, and the embedded files' licence URLs say so. Record each in the notices (docs/PLAN.md §5).

---

## Verification notes

**Shared stylesheet**
- `input{font:inherit}` exists only on 02, 03, 04 and 06 (`input,select` on 05), not in every frame.
- 01 has no `th` rule.
- Added the rules the draft omitted: `body{margin:0}`, `a{text-decoration:none}` and `button{cursor:pointer}`.

**Colour**
- The "no named colours" claim is qualified: `transparent` and `currentColor` are used.
- The colour count wording now covers the script-built row styles.
- Usage lists completed:
  - `bg_app` adds the 01 feature pills;
  - `border_subtle` adds stat tiles, the sidebar panel and the 03 Total rule;
  - `border_control` adds the account selector and address field;
  - text tokens add breadcrumbs, row numbers and neutral tag text.
- Placeholder: the draft reported a measured value. It is now stated as the computed Chromium default, on the 02, 04 and 06 inputs.
- Focus: the select and the radios do not set `outline: none`.
- Watermark measured range widened to `#16342F`–`#163430`. Added the `#162023` compositing artefact.

**Typography**
- Page subtitles exist only on 02–06. 07 has a meta line beside the h1, and 08 has a tag line. Added header meta and breadcrumb rows.
- The table-body 500 weight applies to account names on 02 and 05 (and the 04 description title), not to every first column.
- Callout body line height 1.5 applies on 03 and 05 only; 04 uses normal.
- `tabular-nums` is not on every Montserrat value. Listed exactly where it is set and where it is not.
- The units rule ("always about half size, green") is wrong for 06 "txs", the 03 selector balance and the 04 fee line.
- Added rows for the 03 selector balance, unweighted Montserrat values, and helper text with normal line height.
- Confirmed by measurement that unweighted Montserrat renders identically to 500.

**Spacing and layout**
- The spacing scale lacked 7 (`2px 7px` badge) and 9 (`0 9px` block tag).
- Sidebar is 232.6 px (flex-grow share), not 232. Nav items are 199.6, main 1207.4, and the column 1127.4 at x = 272.6.
- Standard-card inner gap is 12–20, not 14–20.
- The 08 address-forms card has no gap (it uses a 6 px label margin), so it is split from the 02 list cards. The 08 history card has no footer, so that row was relabelled.
- Per-screen widths corrected:
  - 03: 718.2 + 389.1, not 718.7 + 389.3;
  - 04: 648.4 + 458.9, not 648.8 + 459.2;
  - 06: 700.4 + 406.9, not 700.8 + 407.2.
- Table columns re-measured as layout positions:
  - 04 inner width is 630, not 629;
  - several starts and right edges moved by 1–2 px;
  - added the `min-width` of the 03 (640) and 04 (620) tables;
  - outer-cell zero padding applies to 02 and 05–08 only.
- Row heights: added the last-row and selected-row variants and the 08 form rows. The 03 row height comes from the 44 px content-box address field.

**Tables, borders, opacity**
- Header and body cell padding corrected per table. The first and last cells differ from the middle cells.
- The 04 end radii are set on every row, not only the highlighted one.
- Every declared 1.5 px border renders at 1 px in the frames; the inset ring stays 1.5 px.
- Content-box heights: the 06 search bar renders 58, the "Live" chip 34, and the 03 address field 44.
- Opacity: two values in three places, not "three values".
- Radii: added the 06 "Search" button (12), the segmented track (10) and the 03 callout (20).

**Components**
- The tonal "Test" button has no icon.
- Breadcrumbs are vertically centred, not horizontally.
- 08 shows the own-account move as an outgoing row.
- Added the 01 node status footer component, the radio rendering detail, the headline's three-line wrap and the 03 step-label line break.

**Icons and fonts**
- Explicit stroke colours are on three icons (five instances), not four.
- The sliders icon is identical to Feather's, so it was removed from the "differs from any library" list. Server, swap and warning were added to that list.
- The static Montserrat Bold Italic file is used on every frame except 07, not only on 01.
- Added the per-frame face counts and the licence source.