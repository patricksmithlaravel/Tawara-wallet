# Android platform glue

Nothing is built here yet. Phase 1's spike measures how iced runs on
Android, and the owner decides from its report whether iced on mobile goes
ahead (docs/PLAN.md D3, section 7). Phase 4 then adds, kept to the minimum
and each listed in docs/DECISIONS.md:

- `AndroidManifest.xml`, with `android:allowBackup="false"` and
  `dataExtractionRules` that exclude the store from Auto Backup and device
  transfer (docs/PLAN.md section 4.5);
- `FLAG_SECURE` on every screen that shows a secret (section 4.3);
- the soft keyboard and clipboard shims, if phase 1 shows they are needed;
- Gradle, if the chosen shell needs it.

`applicationId`: `com.patricksmithlaravel.tawara` (docs/DECISIONS.md, D8).
The Rust side is `crates/mobile`, loaded as `libtawara_mobile.so`.
