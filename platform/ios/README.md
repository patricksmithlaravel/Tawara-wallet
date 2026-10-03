# iOS platform glue

Nothing is built here yet. Phase 1's spike measures how iced runs on iOS,
and the owner decides from its report whether iced on mobile goes ahead
(docs/PLAN.md D3, section 7). Phase 4 then adds, kept to the minimum and
each listed in docs/DECISIONS.md:

- the Xcode project and `Info.plist`;
- `NSURLIsExcludedFromBackupKey` on the store directory, so iCloud and
  device backups never hold it (docs/PLAN.md section 4.5);
- the file-protection class the owner chooses for the store (section 4.6);
- hiding the content when the app becomes inactive, so the app switcher's
  snapshot shows nothing (section 4.3);
- safe areas.

Bundle identifier: `com.patricksmithlaravel.tawara` (docs/DECISIONS.md, D8).
The Rust side is `crates/mobile`, linked as a static library.
