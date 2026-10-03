//! The repository's standing rules, as checks that run with `cargo test`.
//!
//! Each check reads the tree as committed -- manifests, the lockfile, the
//! sources -- and states in its failure message which rule it holds and
//! where that rule is written down. None of them reaches the network.
//!
//! - The wallet library is pinned by full commit hash and never patched,
//!   vendored or forked (docs/PLAN.md D1).
//! - The release profile keeps `panic = "unwind"` and overflow checks, which
//!   the library's key scrubs depend on and which a dependent has to restate
//!   because cargo reads profiles only from the root of the workspace being
//!   built.
//! - `Kdf::CHEAP_FOR_TESTS` never appears outside test code.
//! - `wallet-core` has no interface dependency, and the interface crates
//!   never name the library (docs/PLAN.md section 3).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::visit_mut::VisitMut;

/// Rep-1, the one source the library may come from.
const LIBRARY_GIT: &str = "https://github.com/patricksmithlaravel/mcm-rust-cli-windows";

/// The library's test-only key-derivation parameters.
const CHEAP_KDF: &str = "CHEAP_FOR_TESTS";

/// The crates that make up the interface. They talk to `wallet-core` and never
/// to the library.
const INTERFACE_CRATES: [&str; 3] = ["app", "desktop", "mobile"];

/// Dependency names that would put an interface toolkit, a windowing layer or
/// a renderer into `wallet-core`.
const INTERFACE_DEPENDENCY_PREFIXES: [&str; 8] = [
    "iced",
    "winit",
    "wgpu",
    "tiny-skia",
    "softbuffer",
    "android-activity",
    "ndk",
    "tawara-app",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root is two levels above crates/wallet-core")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn read_toml(path: &Path) -> toml::Table {
    read(path)
        .parse::<toml::Table>()
        .unwrap_or_else(|e| panic!("{} is not valid TOML: {e}", path.display()))
}

/// Every `.rs` file under `dir`, skipping build output.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries =
            fs::read_dir(&d).unwrap_or_else(|e| panic!("cannot list {}: {e}", d.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Whether `path` is an integration test, a benchmark, or other code cargo
/// compiles only for testing: anything under a `tests/` or `benches/`
/// directory of a crate.
fn is_test_target(path: &Path, crates_dir: &Path) -> bool {
    path.strip_prefix(crates_dir)
        .expect("walked from the crates directory")
        .components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "benches")
}

/// Whether an attribute marks the item it sits on as compiled only for
/// tests: `#[test]`, `#[cfg(test)]`, or `#[cfg(all(test, ...))]`.
/// `#[cfg(any(test, ...))]` is NOT test-only, since its other arms compile
/// the item into ordinary builds.
fn is_test_only(attr: &syn::Attribute) -> bool {
    if attr.path().is_ident("test") {
        return true;
    }
    if !attr.path().is_ident("cfg") {
        return false;
    }
    let Ok(meta) = attr.parse_args::<syn::Meta>() else {
        return false;
    };
    match meta {
        syn::Meta::Path(p) => p.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .map(|args| {
                args.iter()
                    .any(|m| matches!(m, syn::Meta::Path(p) if p.is_ident("test")))
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(is_test_only)
}

/// Removes every item, impl item, trait item and statement that is compiled
/// only for tests, so that what is left is what a non-test build compiles.
struct StripTestCode;

impl VisitMut for StripTestCode {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        file.items.retain(|item| !item_is_test_only(item));
        syn::visit_mut::visit_file_mut(self, file);
    }

    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        if let Some((_, items)) = &mut module.content {
            items.retain(|item| !item_is_test_only(item));
        }
        syn::visit_mut::visit_item_mod_mut(self, module);
    }

    fn visit_item_impl_mut(&mut self, imp: &mut syn::ItemImpl) {
        imp.items.retain(|item| {
            !test_only(match item {
                syn::ImplItem::Const(i) => &i.attrs,
                syn::ImplItem::Fn(i) => &i.attrs,
                syn::ImplItem::Type(i) => &i.attrs,
                syn::ImplItem::Macro(i) => &i.attrs,
                _ => &[],
            })
        });
        syn::visit_mut::visit_item_impl_mut(self, imp);
    }

    fn visit_item_trait_mut(&mut self, tr: &mut syn::ItemTrait) {
        tr.items.retain(|item| {
            !test_only(match item {
                syn::TraitItem::Const(i) => &i.attrs,
                syn::TraitItem::Fn(i) => &i.attrs,
                syn::TraitItem::Type(i) => &i.attrs,
                syn::TraitItem::Macro(i) => &i.attrs,
                _ => &[],
            })
        });
        syn::visit_mut::visit_item_trait_mut(self, tr);
    }

    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        block.stmts.retain(|stmt| match stmt {
            syn::Stmt::Item(item) => !item_is_test_only(item),
            syn::Stmt::Local(local) => !test_only(&local.attrs),
            syn::Stmt::Macro(mac) => !test_only(&mac.attrs),
            syn::Stmt::Expr(..) => true,
        });
        syn::visit_mut::visit_block_mut(self, block);
    }
}

fn item_is_test_only(item: &syn::Item) -> bool {
    let attrs: &[syn::Attribute] = match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::ExternCrate(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::ForeignMod(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::TraitAlias(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        _ => &[],
    };
    test_only(attrs)
}

/// Every identifier in a token stream, including those inside macro
/// invocations, whose bodies `syn` keeps as raw tokens.
fn idents(tokens: proc_macro2::TokenStream, out: &mut Vec<String>) {
    for tt in tokens {
        match tt {
            proc_macro2::TokenTree::Ident(i) => out.push(i.to_string()),
            proc_macro2::TokenTree::Group(g) => idents(g.stream(), out),
            _ => {}
        }
    }
}

/// The identifiers a non-test build of `source` compiles.
fn non_test_idents(source: &str) -> Result<Vec<String>, syn::Error> {
    let mut file = syn::parse_file(source)?;
    StripTestCode.visit_file_mut(&mut file);
    let mut out = Vec::new();
    idents(file.into_token_stream(), &mut out);
    Ok(out)
}

fn names_cheap_kdf_outside_tests(source: &str) -> Result<bool, syn::Error> {
    Ok(non_test_idents(source)?.iter().any(|i| i == CHEAP_KDF))
}

/// `Kdf::CHEAP_FOR_TESTS` derives the store key at a cost chosen to make tests
/// fast, which is the cost an attacker with a stolen store would choose too.
/// Production code uses `Kdf::RECOMMENDED`. This holds every crate's non-test
/// code to that: the identifier may appear in `tests/` and `benches/`, under
/// `#[cfg(test)]`, and in `#[test]` functions, and nowhere else.
///
/// `spikes/` is outside this walk on purpose: it is phase 1's throwaway
/// feasibility code, which uses the cheap parameters for on-device store
/// checks, and nothing in it is built into a shipped artifact.
#[test]
fn cheap_kdf_appears_only_in_test_code() {
    let crates_dir = workspace_root().join("crates");
    let files = rust_files(&crates_dir);
    let walked: Vec<&PathBuf> = files
        .iter()
        .filter(|p| !is_test_target(p, &crates_dir))
        .collect();

    // A walk that reached nothing would pass. Each crate's root must be in it.
    for root in [
        "wallet-core/src/lib.rs",
        "app/src/lib.rs",
        "desktop/src/main.rs",
        "mobile/src/lib.rs",
    ] {
        let root = crates_dir.join(root);
        assert!(
            walked.contains(&&root),
            "the walk did not reach {}; a check over nothing passes vacuously",
            root.display()
        );
    }

    let mut offenders = Vec::new();
    for path in walked {
        let source = read(path);
        match names_cheap_kdf_outside_tests(&source) {
            Ok(true) => offenders.push(path.display().to_string()),
            Ok(false) => {}
            Err(e) => panic!(
                "{} does not parse, so this check cannot say whether it names {CHEAP_KDF}: {e}",
                path.display()
            ),
        }
    }
    assert!(
        offenders.is_empty(),
        "Kdf::{CHEAP_KDF} appears outside test code in: {offenders:?}. Production code \
         derives the store key with Kdf::RECOMMENDED; the cheap parameters are for tests only."
    );
}

/// The detector above, held to cases where the answer is known, so that a
/// detector that sees nothing cannot pass the walk.
#[test]
fn the_cheap_kdf_detector_sees_what_it_must() {
    let flagged = [
        "const K: Kdf = Kdf::CHEAP_FOR_TESTS;",
        "fn f() { let k = Kdf::CHEAP_FOR_TESTS; }",
        "use mochimo_crypto::keystore::Kdf; fn f() { g(Kdf::CHEAP_FOR_TESTS) }",
        "fn f() { open!(Kdf::CHEAP_FOR_TESTS); }",
        "#[cfg(any(test, feature = \"fast\"))] const K: Kdf = Kdf::CHEAP_FOR_TESTS;",
        "#[cfg(not(test))] const K: Kdf = Kdf::CHEAP_FOR_TESTS;",
        "impl S { fn k() -> Kdf { Kdf::CHEAP_FOR_TESTS } }",
        "mod inner { pub fn k() -> Kdf { Kdf::CHEAP_FOR_TESTS } }",
    ];
    for source in flagged {
        assert!(
            names_cheap_kdf_outside_tests(source).expect("test source parses"),
            "the detector missed a non-test use: {source}"
        );
    }

    let allowed = [
        "#[cfg(test)] mod tests { const K: Kdf = Kdf::CHEAP_FOR_TESTS; }",
        "#[cfg(test)] const K: Kdf = Kdf::CHEAP_FOR_TESTS;",
        "#[cfg(all(test, unix))] fn k() -> Kdf { Kdf::CHEAP_FOR_TESTS }",
        "#[test] fn opens() { let k = Kdf::CHEAP_FOR_TESTS; }",
        "impl S { #[cfg(test)] fn k() -> Kdf { Kdf::CHEAP_FOR_TESTS } }",
        "fn f() { #[cfg(test)] let k = Kdf::CHEAP_FOR_TESTS; }",
        "const K: Kdf = Kdf::RECOMMENDED;",
    ];
    for source in allowed {
        assert!(
            !names_cheap_kdf_outside_tests(source).expect("test source parses"),
            "the detector flagged test-only code: {source}"
        );
    }
}

/// docs/PLAN.md D1: the library comes from Rep-1 at an exact commit, with
/// `default-features = false` and exactly `native` and `mesh-https`, and is
/// never patched. Upgrading it is a change to `rev` and to nothing else.
#[test]
fn library_is_pinned_by_full_commit_hash_and_never_patched() {
    let root = workspace_root();
    let manifest = read_toml(&root.join("Cargo.toml"));

    assert!(
        !manifest.contains_key("patch") && !manifest.contains_key("replace"),
        "the workspace manifest has a [patch] or [replace] section; Rep-2 never modifies the \
         library (docs/PLAN.md D1)"
    );

    let dep = manifest["workspace"]["dependencies"]["mochimo-crypto"]
        .as_table()
        .expect("mochimo-crypto is a table in [workspace.dependencies]");
    assert_eq!(
        dep.get("git").and_then(|v| v.as_str()),
        Some(LIBRARY_GIT),
        "mochimo-crypto must come from Rep-1's repository"
    );
    let rev = dep.get("rev").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        rev.len() == 40
            && rev
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "mochimo-crypto must be pinned by a full 40-character lowercase commit hash, not {rev:?}"
    );
    for key in ["branch", "tag", "path", "registry"] {
        assert!(
            !dep.contains_key(key),
            "mochimo-crypto must be pinned by `rev` alone; it also names `{key}`"
        );
    }
    assert_eq!(
        dep.get("default-features").and_then(|v| v.as_bool()),
        Some(false),
        "mochimo-crypto must be taken with default-features = false"
    );
    let features: BTreeSet<&str> = dep
        .get("features")
        .and_then(|v| v.as_array())
        .expect("mochimo-crypto names its features")
        .iter()
        .map(|v| v.as_str().expect("feature names are strings"))
        .collect();
    assert_eq!(
        features,
        BTreeSet::from(["native", "mesh-https"]),
        "mochimo-crypto's features are exactly native and mesh-https; raw-backend is the \
         library's test-only feature and must never be named here"
    );

    // Every crate takes the library from the workspace line, never a line of
    // its own, and no crate manifest carries a [patch] of its own.
    for dir in fs::read_dir(root.join("crates")).expect("crates/ exists") {
        let manifest_path = dir.expect("directory entry").path().join("Cargo.toml");
        if !manifest_path.exists() {
            continue;
        }
        let crate_manifest = read_toml(&manifest_path);
        assert!(
            !crate_manifest.contains_key("patch") && !crate_manifest.contains_key("replace"),
            "{} has a [patch] or [replace] section",
            manifest_path.display()
        );
        for (section, deps) in dependency_tables(&crate_manifest) {
            if let Some(d) = deps.get("mochimo-crypto") {
                assert_eq!(
                    d.get("workspace").and_then(|v| v.as_bool()),
                    Some(true),
                    "{} names mochimo-crypto in [{section}] with a line of its own; it must be \
                     `mochimo-crypto.workspace = true`",
                    manifest_path.display()
                );
                let extra: Vec<&String> = d
                    .as_table()
                    .map(|t| t.keys().filter(|k| *k != "workspace").collect())
                    .unwrap_or_default();
                assert!(
                    extra.is_empty(),
                    "{} adds {extra:?} to mochimo-crypto in [{section}]; features are decided \
                     once, in the workspace line",
                    manifest_path.display()
                );
            }
        }
    }

    // Cargo's own configuration can patch or replace sources too.
    for config in [".cargo/config.toml", ".cargo/config"] {
        let path = root.join(config);
        if path.exists() {
            let config = read_toml(&path);
            assert!(
                !config.contains_key("patch") && !config.contains_key("source"),
                "{} patches or replaces a source",
                path.display()
            );
        }
    }
    assert!(
        !root.join("vendor").exists(),
        "a vendor/ directory exists; Rep-2 never carries a copy of the library"
    );

    // And the lockfile resolved the library at that commit.
    let lock = read_toml(&root.join("Cargo.lock"));
    let locked: Vec<&toml::Table> = lock["package"]
        .as_array()
        .expect("Cargo.lock lists packages")
        .iter()
        .filter_map(|p| p.as_table())
        .filter(|p| p.get("name").and_then(|v| v.as_str()) == Some("mochimo-crypto"))
        .collect();
    assert_eq!(
        locked.len(),
        1,
        "Cargo.lock must hold exactly one mochimo-crypto"
    );
    let source = locked[0]
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        source,
        format!("git+{LIBRARY_GIT}?rev={rev}#{rev}"),
        "Cargo.lock resolved mochimo-crypto from somewhere other than the pinned commit"
    );
}

fn dependency_tables(manifest: &toml::Table) -> Vec<(String, &toml::Table)> {
    let mut out = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(t) = manifest.get(section).and_then(|v| v.as_table()) {
            out.push((section.to_string(), t));
        }
    }
    if let Some(targets) = manifest.get("target").and_then(|v| v.as_table()) {
        for (cfg, body) in targets {
            for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(t) = body.get(section).and_then(|v| v.as_table()) {
                    out.push((format!("target.{cfg}.{section}"), t));
                }
            }
        }
    }
    out
}

/// Cargo reads `[profile.*]` only from the root of the workspace being built,
/// so Rep-1's release profile does not reach this build. The library's key
/// scrubs run through `Drop`, which `panic = "abort"` skips, and a release
/// build turns overflow checks off unless told otherwise. Both are restated in
/// the root manifest, and this holds them there.
#[test]
fn release_profile_keeps_the_library_scrubs() {
    let manifest = read_toml(&workspace_root().join("Cargo.toml"));
    let profiles = manifest
        .get("profile")
        .and_then(|v| v.as_table())
        .expect("the workspace manifest has a [profile] table");
    let release = profiles
        .get("release")
        .and_then(|v| v.as_table())
        .expect("the workspace manifest has [profile.release]");
    assert_eq!(
        release.get("panic").and_then(|v| v.as_str()),
        Some("unwind"),
        "[profile.release] must keep panic = \"unwind\": under abort, Drop does not run and the \
         library's scrubs of key material are skipped"
    );
    assert_eq!(
        release.get("overflow-checks").and_then(|v| v.as_bool()),
        Some(true),
        "[profile.release] must keep overflow-checks = true"
    );
    for (name, profile) in profiles {
        let panic = profile.get("panic").and_then(|v| v.as_str());
        assert_ne!(
            panic,
            Some("abort"),
            "[profile.{name}] sets panic = \"abort\", which skips the library's scrubs"
        );
    }
}

/// docs/PLAN.md section 3: `wallet-core` has no interface dependency, so the
/// mobile shells can be replaced without touching it; and the interface
/// crates talk to `wallet-core` only, never to the library.
#[test]
fn wallet_core_and_the_interface_stay_apart() {
    let crates_dir = workspace_root().join("crates");

    let core = read_toml(&crates_dir.join("wallet-core/Cargo.toml"));
    for (section, deps) in dependency_tables(&core) {
        if section.contains("dev-dependencies") {
            continue;
        }
        for name in deps.keys() {
            assert!(
                !INTERFACE_DEPENDENCY_PREFIXES
                    .iter()
                    .any(|p| name.starts_with(p)),
                "wallet-core depends on `{name}` in [{section}]; it must have no interface \
                 dependency"
            );
        }
    }

    for krate in INTERFACE_CRATES {
        let dir = crates_dir.join(krate);
        let manifest = read_toml(&dir.join("Cargo.toml"));
        for (section, deps) in dependency_tables(&manifest) {
            assert!(
                !deps.contains_key("mochimo-crypto"),
                "crates/{krate} depends on mochimo-crypto in [{section}]; the interface talks \
                 to wallet-core only"
            );
        }
        for path in rust_files(&dir) {
            let source = read(&path);
            let mut found = Vec::new();
            idents(
                source
                    .parse::<proc_macro2::TokenStream>()
                    .unwrap_or_else(|e| panic!("{} does not lex: {e}", path.display())),
                &mut found,
            );
            assert!(
                !found.iter().any(|i| i == "mochimo_crypto"),
                "{} names mochimo_crypto; the interface talks to wallet-core only",
                path.display()
            );
        }
    }

    // wallet-core must not hand the whole library through to the interface.
    for path in rust_files(&crates_dir.join("wallet-core/src")) {
        let file = syn::parse_file(&read(&path))
            .unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()));
        for item in &file.items {
            let reexports_library = match item {
                syn::Item::Use(u) if matches!(u.vis, syn::Visibility::Public(_)) => {
                    use_tree_is_whole_library(&u.tree)
                }
                syn::Item::ExternCrate(e) if matches!(e.vis, syn::Visibility::Public(_)) => {
                    e.ident == "mochimo_crypto"
                }
                _ => false,
            };
            assert!(
                !reexports_library,
                "{} re-exports mochimo_crypto wholesale; the interface would then reach the \
                 library around the worker",
                path.display()
            );
        }
    }
}

/// `pub use mochimo_crypto;`, `pub use mochimo_crypto as x;` or
/// `pub use mochimo_crypto::*;`.
fn use_tree_is_whole_library(tree: &syn::UseTree) -> bool {
    match tree {
        syn::UseTree::Name(n) => n.ident == "mochimo_crypto",
        syn::UseTree::Rename(r) => r.ident == "mochimo_crypto",
        syn::UseTree::Path(p) => {
            p.ident == "mochimo_crypto" && matches!(*p.tree, syn::UseTree::Glob(_))
        }
        syn::UseTree::Group(g) => g.items.iter().any(use_tree_is_whole_library),
        syn::UseTree::Glob(_) => false,
    }
}
