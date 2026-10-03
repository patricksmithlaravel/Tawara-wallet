//! Tawara for Android and iOS: the entry points the platform shells load.
//!
//! The same `tawara-app` crate runs here as on the desktop. Android loads
//! this crate as `libtawara_mobile.so`; the iOS Xcode project links it as a
//! static library. Which shell drives iced on each platform is phase 1's
//! feasibility spike, and the owner's decision after it (docs/PLAN.md D3);
//! nothing mobile is built before that decision.
