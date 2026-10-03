//! The repository's standing rules, as checks that run with `cargo test`.
//!
//! Each check reads the tree as committed -- manifests, the lockfile, cargo's
//! own configuration files in the repository, the sources -- and states in
//! its failure message which rule it holds and where that rule is written
//! down. None of them reaches the network.
//!
//! - The wallet library is pinned by full commit hash and never patched,
//!   replaced, path-overridden, vendored or forked (docs/PLAN.md D1).
//! - The release profile keeps `panic = "unwind"` and overflow checks, which
//!   the library's key scrubs depend on and which a dependent has to restate
//!   because cargo reads profiles only from the root of the workspace being
//!   built.
//! - `Kdf::CHEAP_FOR_TESTS` never appears outside test code.
//! - `wallet-core` has no interface dependency, and the interface crates
//!   never depend on or name the library (docs/PLAN.md section 3).
//!
//! What no check here can see: cargo configuration outside the repository
//! (`$CARGO_HOME/config.toml`), and flags passed through the environment
//! (`RUSTFLAGS`, `CARGO_PROFILE_*`). CI sets neither.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::ext::IdentExt;
use syn::visit_mut::VisitMut;

/// Rep-1, the one source the library may come from.
const LIBRARY_GIT: &str = "https://github.com/patricksmithlaravel/mcm-rust-cli-windows";

/// The library's package name.
const LIBRARY: &str = "mochimo-crypto";

/// The library's crate name, as source code names it.
const LIBRARY_IDENT: &str = "mochimo_crypto";

/// The library's test-only key-derivation parameters.
const CHEAP_KDF: &str = "CHEAP_FOR_TESTS";

/// The crates that make up the interface. They talk to `wallet-core` and never
/// to the library.
const INTERFACE_CRATES: [&str; 3] = ["app", "desktop", "mobile"];

/// Package names that would put an interface toolkit, a windowing layer, a
/// renderer or the interface itself into `wallet-core`'s dependency closure.
const INTERFACE_DEPENDENCY_PREFIXES: [&str; 10] = [
    "iced",
    "winit",
    "wgpu",
    "tiny-skia",
    "softbuffer",
    "android-activity",
    "ndk",
    "tawara-app",
    "tawara-desktop",
    "tawara-mobile",
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

/// Whether `dir` is a cargo build directory: a `target` directory beside a
/// manifest. Only those are skipped; a source directory that happens to be
/// called `target` is walked like any other.
fn is_build_output(dir: &Path) -> bool {
    dir.file_name().is_some_and(|n| n == "target")
        && dir.parent().is_some_and(|p| p.join("Cargo.toml").exists())
}

/// Every file under `dir` for which `keep` holds, skipping build output and
/// git's own directory.
fn files_under(dir: &Path, keep: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries =
            fs::read_dir(&d).unwrap_or_else(|e| panic!("cannot list {}: {e}", d.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                if is_build_output(&path) || path.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
                stack.push(path);
            } else if keep(&path) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    files_under(dir, &|p| p.extension().is_some_and(|e| e == "rs"))
}

/// Every crate directory under `crates/`.
fn crate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(workspace_root().join("crates"))
        .expect("crates/ exists")
        .map(|e| e.expect("directory entry").path())
        .filter(|p| p.join("Cargo.toml").exists())
        .collect();
    dirs.sort();
    dirs
}

/// Every cargo configuration file inside the repository.
fn cargo_configs() -> Vec<PathBuf> {
    files_under(&workspace_root(), &|p| {
        p.parent()
            .and_then(|d| d.file_name())
            .is_some_and(|d| d == ".cargo")
            && p.file_name()
                .is_some_and(|n| n == "config" || n == "config.toml")
    })
}

// ---------------------------------------------------------------------------
// Test-only code, as rustc decides it.
// ---------------------------------------------------------------------------

/// Whether an attribute marks what it sits on as compiled only for tests:
/// `#[test]`, `#[cfg(test)]`, or `#[cfg(all(test, ...))]`.
/// `#[cfg(any(test, ...))]` is NOT test-only, since its other arms compile
/// the item into ordinary builds; neither is `#[cfg(not(test))]`.
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

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
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
    }
}

fn expr_attrs(expr: &syn::Expr) -> &[syn::Attribute] {
    use syn::Expr::*;
    match expr {
        Array(e) => &e.attrs,
        Assign(e) => &e.attrs,
        Async(e) => &e.attrs,
        Await(e) => &e.attrs,
        Binary(e) => &e.attrs,
        Block(e) => &e.attrs,
        Break(e) => &e.attrs,
        Call(e) => &e.attrs,
        Cast(e) => &e.attrs,
        Closure(e) => &e.attrs,
        Const(e) => &e.attrs,
        Continue(e) => &e.attrs,
        Field(e) => &e.attrs,
        ForLoop(e) => &e.attrs,
        Group(e) => &e.attrs,
        If(e) => &e.attrs,
        Index(e) => &e.attrs,
        Infer(e) => &e.attrs,
        Let(e) => &e.attrs,
        Lit(e) => &e.attrs,
        Loop(e) => &e.attrs,
        Macro(e) => &e.attrs,
        Match(e) => &e.attrs,
        MethodCall(e) => &e.attrs,
        Paren(e) => &e.attrs,
        Path(e) => &e.attrs,
        Range(e) => &e.attrs,
        RawAddr(e) => &e.attrs,
        Reference(e) => &e.attrs,
        Repeat(e) => &e.attrs,
        Return(e) => &e.attrs,
        Struct(e) => &e.attrs,
        Try(e) => &e.attrs,
        TryBlock(e) => &e.attrs,
        Tuple(e) => &e.attrs,
        Unary(e) => &e.attrs,
        Unsafe(e) => &e.attrs,
        While(e) => &e.attrs,
        Yield(e) => &e.attrs,
        _ => &[],
    }
}

/// Removes everything a non-test build does not compile: a file whose own
/// inner attributes make it test-only, and every item, impl item, trait
/// item, statement, match arm, struct field and enum variant marked
/// test-only. What is left is what a non-test build compiles.
struct StripTestCode;

impl VisitMut for StripTestCode {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        if test_only(&file.attrs) {
            file.items.clear();
            return;
        }
        file.items.retain(|item| !test_only(item_attrs(item)));
        syn::visit_mut::visit_file_mut(self, file);
    }

    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        if let Some((_, items)) = &mut module.content {
            items.retain(|item| !test_only(item_attrs(item)));
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
            syn::Stmt::Item(item) => !test_only(item_attrs(item)),
            syn::Stmt::Local(local) => !test_only(&local.attrs),
            syn::Stmt::Macro(mac) => !test_only(&mac.attrs),
            syn::Stmt::Expr(expr, _) => !test_only(expr_attrs(expr)),
        });
        syn::visit_mut::visit_block_mut(self, block);
    }

    fn visit_expr_match_mut(&mut self, m: &mut syn::ExprMatch) {
        m.arms.retain(|arm| !test_only(&arm.attrs));
        syn::visit_mut::visit_expr_match_mut(self, m);
    }

    fn visit_expr_struct_mut(&mut self, s: &mut syn::ExprStruct) {
        s.fields = std::mem::take(&mut s.fields)
            .into_pairs()
            .filter(|pair| !test_only(&pair.value().attrs))
            .collect();
        syn::visit_mut::visit_expr_struct_mut(self, s);
    }

    fn visit_fields_named_mut(&mut self, fields: &mut syn::FieldsNamed) {
        fields.named = std::mem::take(&mut fields.named)
            .into_pairs()
            .filter(|pair| !test_only(&pair.value().attrs))
            .collect();
        syn::visit_mut::visit_fields_named_mut(self, fields);
    }

    fn visit_item_enum_mut(&mut self, e: &mut syn::ItemEnum) {
        e.variants = std::mem::take(&mut e.variants)
            .into_pairs()
            .filter(|pair| !test_only(&pair.value().attrs))
            .collect();
        syn::visit_mut::visit_item_enum_mut(self, e);
    }
}

/// Every identifier in a token stream, including those inside macro
/// invocations, whose bodies `syn` keeps as raw tokens. Raw identifiers are
/// compared without their `r#`, so `r#CHEAP_FOR_TESTS` is `CHEAP_FOR_TESTS`.
fn idents(tokens: proc_macro2::TokenStream, out: &mut Vec<String>) {
    for tt in tokens {
        match tt {
            proc_macro2::TokenTree::Ident(i) => out.push(i.unraw().to_string()),
            proc_macro2::TokenTree::Group(g) => idents(g.stream(), out),
            _ => {}
        }
    }
}

/// A source file with what only test builds compile removed.
fn non_test_file(source: &str) -> Result<syn::File, syn::Error> {
    let mut file = syn::parse_file(source)?;
    StripTestCode.visit_file_mut(&mut file);
    Ok(file)
}

fn names_cheap_kdf_outside_tests(source: &str) -> Result<bool, syn::Error> {
    let mut found = Vec::new();
    idents(non_test_file(source)?.into_token_stream(), &mut found);
    Ok(found.iter().any(|i| i == CHEAP_KDF))
}

// ---------------------------------------------------------------------------
// The module tree a non-test build compiles.
// ---------------------------------------------------------------------------

/// A source file in the module tree, and whether nested out-of-line modules
/// in it resolve beside it (a crate root, a `mod.rs`, or a `#[path]` file) or
/// in a directory named after it.
#[derive(Clone)]
struct ModFile {
    path: PathBuf,
    mod_rs: bool,
}

fn path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.path().is_ident("path"))
        .map(|a| match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => s.value(),
                _ => panic!("a #[path] attribute that is not a string literal"),
            },
            _ => panic!("a #[path] attribute that is not `path = \"...\"`"),
        })
}

/// The out-of-line `mod name;` declarations in `items`, each with the inline
/// modules it is nested in and its `#[path]`, if any. Run on a file already
/// stripped of test code, so a `#[cfg(test)] mod tests;` is not followed.
fn out_of_line_mods(
    items: &[syn::Item],
    inline: &[String],
    out: &mut Vec<(Vec<String>, String, Option<String>)>,
) {
    for item in items {
        if let syn::Item::Mod(m) = item {
            match &m.content {
                None => out.push((
                    inline.to_vec(),
                    m.ident.unraw().to_string(),
                    path_attr(&m.attrs),
                )),
                Some((_, inner)) => {
                    let mut nested = inline.to_vec();
                    nested.push(path_attr(&m.attrs).unwrap_or_else(|| m.ident.unraw().to_string()));
                    out_of_line_mods(inner, &nested, out);
                }
            }
        }
    }
}

/// Where rustc looks for `mod name;` declared in `from`, inside the inline
/// modules `inline`, with an optional `#[path]`.
fn resolve_mod(from: &ModFile, inline: &[String], name: &str, path: Option<&str>) -> ModFile {
    let dir = from.path.parent().expect("a source file has a directory");
    let module_dir = if from.mod_rs {
        dir.to_path_buf()
    } else {
        dir.join(from.path.file_stem().expect("a source file has a stem"))
    };
    if let Some(p) = path {
        // Outside an inline module, `#[path]` is relative to the declaring
        // file's directory; inside one, to the inline module's directory.
        let base = if inline.is_empty() {
            dir.to_path_buf()
        } else {
            inline.iter().fold(module_dir, |b, m| b.join(m))
        };
        return ModFile {
            path: base.join(p),
            mod_rs: true,
        };
    }
    let base = inline.iter().fold(module_dir, |b, m| b.join(m));
    let flat = base.join(format!("{name}.rs"));
    let nested = base.join(name).join("mod.rs");
    match (flat.exists(), nested.exists()) {
        (true, false) => ModFile {
            path: flat,
            mod_rs: false,
        },
        (false, true) => ModFile {
            path: nested,
            mod_rs: true,
        },
        (true, true) => panic!("both {} and {} exist", flat.display(), nested.display()),
        (false, false) => panic!(
            "`mod {name};` in {} resolves to neither {} nor {}",
            from.path.display(),
            flat.display(),
            nested.display()
        ),
    }
}

/// The crate roots of every target in `crate_dir` that is not a test or a
/// benchmark: the library, every binary, the build script and every example,
/// read from the manifest where it names a path and from cargo's default
/// layout where it does not.
fn non_test_roots(crate_dir: &Path) -> Vec<PathBuf> {
    let manifest = read_toml(&crate_dir.join("Cargo.toml"));
    let mut roots = Vec::new();
    let mut push = |p: PathBuf| {
        if p.exists() && !roots.contains(&p) {
            roots.push(p);
        }
    };

    let named = |key: &str| -> Vec<PathBuf> {
        manifest
            .get(key)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|t| t.get("path").and_then(|p| p.as_str()))
            .map(|p| crate_dir.join(p))
            .collect()
    };

    match manifest
        .get("lib")
        .and_then(|l| l.get("path"))
        .and_then(|p| p.as_str())
    {
        Some(p) => push(crate_dir.join(p)),
        None => push(crate_dir.join("src/lib.rs")),
    }
    push(crate_dir.join("src/main.rs"));
    for p in named("bin") {
        push(p);
    }
    for p in named("example") {
        push(p);
    }
    for (sub, also) in [("src/bin", "main.rs"), ("examples", "main.rs")] {
        if let Ok(entries) = fs::read_dir(crate_dir.join(sub)) {
            for entry in entries {
                let p = entry.expect("directory entry").path();
                if p.is_dir() {
                    push(p.join(also));
                } else if p.extension().is_some_and(|e| e == "rs") {
                    push(p);
                }
            }
        }
    }
    match manifest.get("package").and_then(|p| p.get("build")) {
        Some(toml::Value::String(p)) => push(crate_dir.join(p)),
        Some(toml::Value::Boolean(false)) => {}
        _ => push(crate_dir.join("build.rs")),
    }
    roots.sort();
    roots
}

/// Every file of the module tree a non-test build of `crate_dir` compiles,
/// each already stripped of its test-only code.
fn non_test_module_tree(crate_dir: &Path) -> Vec<(PathBuf, syn::File)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut stack: Vec<ModFile> = non_test_roots(crate_dir)
        .into_iter()
        .map(|path| ModFile { path, mod_rs: true })
        .collect();
    while let Some(file) = stack.pop() {
        if !seen.insert(file.path.clone()) {
            continue;
        }
        let parsed = non_test_file(&read(&file.path)).unwrap_or_else(|e| {
            panic!(
                "{} does not parse, so this check cannot say what it compiles: {e}",
                file.path.display()
            )
        });
        let mut mods = Vec::new();
        out_of_line_mods(&parsed.items, &[], &mut mods);
        for (inline, name, path) in mods {
            stack.push(resolve_mod(&file, &inline, &name, path.as_deref()));
        }
        out.push((file.path, parsed));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn cheap_kdf_offenders(crate_dir: &Path) -> Vec<PathBuf> {
    non_test_module_tree(crate_dir)
        .into_iter()
        .filter(|(_, file)| {
            let mut found = Vec::new();
            idents(file.to_token_stream(), &mut found);
            found.iter().any(|i| i == CHEAP_KDF)
        })
        .map(|(path, _)| path)
        .collect()
}

/// `Kdf::CHEAP_FOR_TESTS` derives the store key at a cost chosen to make tests
/// fast, which is the cost an attacker with a stolen store would choose too.
/// Production code uses `Kdf::RECOMMENDED`. This holds every crate's non-test
/// code to never naming the cheap parameters: the walk follows the module
/// tree of every non-test target from its root, as rustc does, and looks at
/// what is left once `#[cfg(test)]`, `#[cfg(all(test, ..))]` and `#[test]`
/// code is removed. Integration tests and benchmarks are not in that tree.
///
/// `spikes/` is outside the walk on purpose: it is phase 1's throwaway
/// feasibility code, which uses the cheap parameters for on-device store
/// checks, and nothing in it is built into a shipped artifact.
#[test]
fn cheap_kdf_appears_only_in_test_code() {
    let crates_dir = workspace_root().join("crates");
    let mut reached = BTreeSet::new();
    let mut offenders = Vec::new();
    for dir in crate_dirs() {
        for (path, _) in non_test_module_tree(&dir) {
            reached.insert(path);
        }
        offenders.extend(cheap_kdf_offenders(&dir));
    }

    // A walk that reached nothing would pass. Each crate's root must be in it.
    for root in [
        "wallet-core/src/lib.rs",
        "app/src/lib.rs",
        "desktop/src/main.rs",
        "mobile/src/lib.rs",
    ] {
        let root = crates_dir.join(root);
        assert!(
            reached.contains(&root),
            "the walk did not reach {}; a check over nothing passes vacuously",
            root.display()
        );
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
        "const K: Kdf = Kdf::r#CHEAP_FOR_TESTS;",
        "fn f(x: u8) -> Kdf { match x { 0 => Kdf::CHEAP_FOR_TESTS, _ => Kdf::RECOMMENDED } }",
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
        "#![cfg(test)] const K: Kdf = Kdf::CHEAP_FOR_TESTS;",
        "fn f() { #[cfg(test)] { use_kdf(Kdf::CHEAP_FOR_TESTS); } }",
        "fn f(x: u8) -> Kdf { match x { #[cfg(test)] 0 => Kdf::CHEAP_FOR_TESTS, _ => Kdf::RECOMMENDED } }",
        "fn f() -> S { S { #[cfg(test)] k: Kdf::CHEAP_FOR_TESTS, r: Kdf::RECOMMENDED } }",
        "const K: Kdf = Kdf::RECOMMENDED;",
    ];
    for source in allowed {
        assert!(
            !names_cheap_kdf_outside_tests(source).expect("test source parses"),
            "the detector flagged test-only code: {source}"
        );
    }
}

/// The module walk, held to a crate laid out on disk where the answer is
/// known: an out-of-line test module's file is not compiled into a non-test
/// build and must not be flagged, and a production module two levels down
/// must be reached and flagged.
#[test]
fn the_module_walk_follows_what_rustc_compiles() {
    let dir = std::env::temp_dir().join(format!(
        "tawara-policy-walk-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let write = |rel: &str, body: &str| {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    };
    write("Cargo.toml", "[package]\nname = \"probe\"\n");
    write(
        "src/lib.rs",
        "mod a;\n#[cfg(test)] mod tests;\n#[path = \"elsewhere/c.rs\"] mod c;\nmod inline { mod d; }\n",
    );
    write("src/tests.rs", "const K: Kdf = Kdf::CHEAP_FOR_TESTS;\n");
    write("src/a.rs", "mod b;\n");
    write("src/a/b.rs", "const K: Kdf = Kdf::CHEAP_FOR_TESTS;\n");
    write("src/elsewhere/c.rs", "const K: Kdf = Kdf::RECOMMENDED;\n");
    write("src/inline/d.rs", "const K: Kdf = Kdf::RECOMMENDED;\n");
    write("tests/it.rs", "const K: Kdf = Kdf::CHEAP_FOR_TESTS;\n");

    let reached: Vec<PathBuf> = non_test_module_tree(&dir)
        .into_iter()
        .map(|(p, _)| p.strip_prefix(&dir).unwrap().to_path_buf())
        .collect();
    let offenders: Vec<PathBuf> = cheap_kdf_offenders(&dir)
        .into_iter()
        .map(|p| p.strip_prefix(&dir).unwrap().to_path_buf())
        .collect();
    let _ = fs::remove_dir_all(&dir);

    for expected in [
        "src/lib.rs",
        "src/a.rs",
        "src/a/b.rs",
        "src/elsewhere/c.rs",
        "src/inline/d.rs",
    ] {
        assert!(
            reached.contains(&PathBuf::from(expected)),
            "the walk did not reach {expected}: {reached:?}"
        );
    }
    assert!(
        !reached.contains(&PathBuf::from("src/tests.rs")),
        "the walk followed a #[cfg(test)] module"
    );
    assert!(
        !reached.contains(&PathBuf::from("tests/it.rs")),
        "the walk treated an integration test as non-test code"
    );
    assert_eq!(offenders, vec![PathBuf::from("src/a/b.rs")]);
}

// ---------------------------------------------------------------------------
// The library pin.
// ---------------------------------------------------------------------------

/// The package a dependency entry resolves to: its `package` key when it
/// renames one, else its key.
fn package_name<'a>(key: &'a str, value: &'a toml::Value) -> &'a str {
    value.get("package").and_then(|v| v.as_str()).unwrap_or(key)
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

/// docs/PLAN.md D1: the library comes from Rep-1 at an exact commit, with
/// `default-features = false` and exactly `native` and `mesh-https`, in the
/// plan's own form, and is never patched, replaced or path-overridden.
/// Upgrading it is a change to `rev` and to nothing else. (deny.toml also
/// refuses `raw-backend` in the resolved graph, whatever route turns it on.)
#[test]
fn library_is_pinned_by_full_commit_hash_and_never_patched() {
    let root = workspace_root();
    let manifest = read_toml(&root.join("Cargo.toml"));

    assert!(
        !manifest.contains_key("patch") && !manifest.contains_key("replace"),
        "the workspace manifest has a [patch] or [replace] section; Rep-2 never modifies the \
         library (docs/PLAN.md D1)"
    );

    let workspace_deps = manifest["workspace"]["dependencies"]
        .as_table()
        .expect("[workspace.dependencies] exists");
    let library_lines: Vec<&String> = workspace_deps
        .iter()
        .filter(|(k, v)| package_name(k, v) == LIBRARY)
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        library_lines,
        vec![LIBRARY],
        "[workspace.dependencies] must name {LIBRARY} exactly once, under its own name"
    );
    let dep = workspace_deps[LIBRARY]
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
    let allowed_keys = BTreeSet::from(["git", "rev", "default-features", "features"]);
    for key in dep.keys() {
        assert!(
            allowed_keys.contains(key.as_str()),
            "mochimo-crypto's line takes the plan's form (git, rev, default-features, features); \
             it also names `{key}`"
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

    // Every crate takes the library from the workspace line, under its own
    // name, adds nothing to it, and forwards none of its features; and no
    // crate manifest carries a [patch] of its own.
    for dir in crate_dirs() {
        let manifest_path = dir.join("Cargo.toml");
        let crate_manifest = read_toml(&manifest_path);
        assert!(
            !crate_manifest.contains_key("patch") && !crate_manifest.contains_key("replace"),
            "{} has a [patch] or [replace] section",
            manifest_path.display()
        );
        for (section, deps) in dependency_tables(&crate_manifest) {
            for (key, d) in deps.iter().filter(|(k, v)| package_name(k, v) == LIBRARY) {
                assert_eq!(
                    key,
                    LIBRARY,
                    "{} renames mochimo-crypto to `{key}` in [{section}]",
                    manifest_path.display()
                );
                let keys: Vec<&String> =
                    d.as_table().map(|t| t.keys().collect()).unwrap_or_default();
                assert!(
                    keys == vec!["workspace"]
                        && d.get("workspace").and_then(|v| v.as_bool()) == Some(true),
                    "{} names mochimo-crypto in [{section}] with a line of its own ({keys:?}); \
                     it must be `mochimo-crypto.workspace = true`, with features decided once, \
                     in the workspace line",
                    manifest_path.display()
                );
            }
        }
        if let Some(feats) = crate_manifest.get("features").and_then(|v| v.as_table()) {
            for (name, list) in feats {
                for entry in list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                {
                    assert!(
                        !entry.starts_with("mochimo-crypto/")
                            && !entry.starts_with("mochimo-crypto?/"),
                        "{} feature `{name}` enables `{entry}`; mochimo-crypto's features are \
                         decided once, in the workspace line",
                        manifest_path.display()
                    );
                }
            }
        }
    }

    // Cargo's own configuration can patch, replace or path-override a source
    // too, from any `.cargo` directory cargo would read in this repository.
    for path in cargo_configs() {
        let config = read_toml(&path);
        for key in ["patch", "source", "paths"] {
            assert!(
                !config.contains_key(key),
                "{} sets `{key}`, which patches, replaces or path-overrides a source",
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
        .filter(|p| p.get("name").and_then(|v| v.as_str()) == Some(LIBRARY))
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

// ---------------------------------------------------------------------------
// The release profile.
// ---------------------------------------------------------------------------

/// Every table under `table`, at any depth, that turns overflow checks off or
/// sets `panic = "abort"`: a profile, a `package.<name>` or `package."*"`
/// override, or a `build-override`.
fn profile_offenders(path: &str, table: &toml::Table, out: &mut Vec<String>) {
    if table.get("overflow-checks").and_then(|v| v.as_bool()) == Some(false) {
        out.push(format!("[{path}] sets overflow-checks = false"));
    }
    if table.get("panic").and_then(|v| v.as_str()) == Some("abort") {
        out.push(format!("[{path}] sets panic = \"abort\""));
    }
    for (key, value) in table {
        if let Some(sub) = value.as_table() {
            profile_offenders(&format!("{path}.{key}"), sub, out);
        }
    }
}

/// Every rustflags value in a cargo configuration, as strings.
fn rustflags(config: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |v: &toml::Value| match v {
        toml::Value::String(s) => out.push(s.clone()),
        toml::Value::Array(a) => out.extend(a.iter().filter_map(|x| x.as_str()).map(String::from)),
        _ => {}
    };
    if let Some(v) = config.get("build").and_then(|b| b.get("rustflags")) {
        push(v);
    }
    if let Some(targets) = config.get("target").and_then(|t| t.as_table()) {
        for body in targets.values() {
            if let Some(v) = body.get("rustflags") {
                push(v);
            }
        }
    }
    out
}

/// Cargo reads `[profile.*]` only from the root of the workspace being built,
/// so Rep-1's release profile does not reach this build. The library's key
/// scrubs run through `Drop`, which `panic = "abort"` skips, and a release
/// build turns overflow checks off unless told otherwise. Both are restated in
/// the root manifest, and this holds them there and holds every other place
/// in the repository that could undo them: any profile or per-package
/// override in the root manifest, the profiles in cargo's configuration
/// files, and the rustflags those files set.
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

    let mut offenders = Vec::new();
    profile_offenders("profile", profiles, &mut offenders);
    for path in cargo_configs() {
        let config = read_toml(&path);
        if let Some(p) = config.get("profile").and_then(|v| v.as_table()) {
            profile_offenders(&format!("{}: profile", path.display()), p, &mut offenders);
        }
        for flags in rustflags(&config) {
            let flat: String = flags
                .to_ascii_lowercase()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            for bad in [
                "overflow-checks=off",
                "overflow-checks=no",
                "overflow-checks=n",
                "overflow-checks=false",
                "panic=abort",
            ] {
                if flat.contains(bad) {
                    offenders.push(format!("{}: rustflags `{flags}`", path.display()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the library's scrubs need panic = \"unwind\" and overflow checks on: {offenders:?}"
    );
}

// ---------------------------------------------------------------------------
// wallet-core and the interface.
// ---------------------------------------------------------------------------

/// Every package in `start`'s dependency closure, as Cargo.lock resolved it.
fn lock_closure(start: &str) -> BTreeSet<String> {
    let lock = read_toml(&workspace_root().join("Cargo.lock"));
    let packages: Vec<&toml::Table> = lock["package"]
        .as_array()
        .expect("Cargo.lock lists packages")
        .iter()
        .filter_map(|p| p.as_table())
        .collect();
    let mut by_name: BTreeMap<&str, Vec<&toml::Table>> = BTreeMap::new();
    for p in &packages {
        by_name
            .entry(p["name"].as_str().expect("a package has a name"))
            .or_default()
            .push(p);
    }
    // A dependency entry is `name`, `name version` or `name version (source)`.
    let find = |spec: &str| -> &toml::Table {
        let mut parts = spec.split(' ');
        let name = parts.next().expect("a dependency entry has a name");
        let version = parts.next();
        let candidates: Vec<&toml::Table> = by_name
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .filter(|p| version.is_none_or(|v| p["version"].as_str() == Some(v)))
            .collect();
        assert_eq!(
            candidates.len(),
            1,
            "Cargo.lock entry `{spec}` resolves to {} packages",
            candidates.len()
        );
        candidates[0]
    };

    let mut seen = BTreeSet::new();
    let mut stack = vec![start.to_string()];
    while let Some(spec) = stack.pop() {
        let package = find(&spec);
        let id = format!(
            "{} {}",
            package["name"].as_str().unwrap(),
            package["version"].as_str().unwrap()
        );
        if !seen.insert(id) {
            continue;
        }
        for dep in package
            .get("dependencies")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
        {
            stack.push(dep.to_string());
        }
    }
    seen.into_iter()
        .map(|id| id.split(' ').next().unwrap().to_string())
        .collect()
}

/// `pub use mochimo_crypto;`, `pub use mochimo_crypto as x;`, and `self` or
/// `*` directly under `mochimo_crypto`, at any depth of a use tree.
fn use_tree_is_whole_library(tree: &syn::UseTree, under_lib: bool) -> bool {
    match tree {
        syn::UseTree::Name(n) => {
            n.ident.unraw() == LIBRARY_IDENT || (under_lib && n.ident.unraw() == "self")
        }
        syn::UseTree::Rename(r) => {
            r.ident.unraw() == LIBRARY_IDENT || (under_lib && r.ident.unraw() == "self")
        }
        syn::UseTree::Glob(_) => under_lib,
        syn::UseTree::Path(p) => {
            use_tree_is_whole_library(&p.tree, p.ident.unraw() == LIBRARY_IDENT)
        }
        syn::UseTree::Group(g) => g
            .items
            .iter()
            .any(|t| use_tree_is_whole_library(t, under_lib)),
    }
}

fn reexports_library(items: &[syn::Item]) -> bool {
    items.iter().any(|item| match item {
        syn::Item::Use(u) if matches!(u.vis, syn::Visibility::Public(_)) => {
            use_tree_is_whole_library(&u.tree, false)
        }
        syn::Item::ExternCrate(e) if matches!(e.vis, syn::Visibility::Public(_)) => {
            e.ident.unraw() == LIBRARY_IDENT
        }
        syn::Item::Mod(m) => m
            .content
            .as_ref()
            .is_some_and(|(_, inner)| reexports_library(inner)),
        _ => false,
    })
}

/// docs/PLAN.md section 3: `wallet-core` has no interface dependency, so the
/// mobile shells can be replaced without touching it; and the interface
/// crates talk to `wallet-core` only, never to the library.
#[test]
fn wallet_core_and_the_interface_stay_apart() {
    let crates_dir = workspace_root().join("crates");

    // Nothing in wallet-core's dependency closure, as Cargo.lock resolved it,
    // is an interface toolkit, a windowing layer, a renderer or the
    // interface itself.
    let closure = lock_closure("tawara-wallet-core");
    assert!(
        closure.contains(LIBRARY),
        "the closure walk did not reach mochimo-crypto from wallet-core; a walk over nothing \
         passes vacuously"
    );
    for name in &closure {
        assert!(
            !INTERFACE_DEPENDENCY_PREFIXES
                .iter()
                .any(|p| name.starts_with(p)),
            "wallet-core's dependency closure holds `{name}`; it must have no interface dependency"
        );
    }

    for krate in INTERFACE_CRATES {
        let dir = crates_dir.join(krate);
        let manifest = read_toml(&dir.join("Cargo.toml"));
        for (section, deps) in dependency_tables(&manifest) {
            for (key, value) in deps {
                assert_ne!(
                    package_name(key, value),
                    LIBRARY,
                    "crates/{krate} depends on mochimo-crypto (as `{key}`) in [{section}]; the \
                     interface talks to wallet-core only"
                );
            }
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
                !found.iter().any(|i| i == LIBRARY_IDENT),
                "{} names mochimo_crypto; the interface talks to wallet-core only",
                path.display()
            );
        }
    }

    // wallet-core must not hand the whole library through to the interface,
    // from any module of what it compiles.
    for (path, file) in non_test_module_tree(&crates_dir.join("wallet-core")) {
        assert!(
            !reexports_library(&file.items),
            "{} re-exports mochimo_crypto wholesale; the interface would then reach the library \
             around the worker",
            path.display()
        );
    }
}

/// The re-export detector above, held to fixed cases.
#[test]
fn the_reexport_detector_sees_what_it_must() {
    let flagged = [
        "pub use mochimo_crypto;",
        "pub use mochimo_crypto as lib;",
        "pub use mochimo_crypto::*;",
        "pub use ::mochimo_crypto;",
        "pub use mochimo_crypto::{self};",
        "pub use mochimo_crypto::{self as lib, keystore};",
        "pub use mochimo_crypto::{*};",
        "pub use r#mochimo_crypto;",
        "pub extern crate mochimo_crypto;",
        "pub mod inner { pub use mochimo_crypto; }",
    ];
    for source in flagged {
        let file = syn::parse_file(source).expect("test source parses");
        assert!(
            reexports_library(&file.items),
            "the detector missed a wholesale re-export: {source}"
        );
    }
    let allowed = [
        "use mochimo_crypto;",
        "pub(crate) use mochimo_crypto;",
        "pub use mochimo_crypto::keystore::Kdf;",
        "pub use mochimo_crypto::{keystore, wallet};",
    ];
    for source in allowed {
        let file = syn::parse_file(source).expect("test source parses");
        assert!(
            !reexports_library(&file.items),
            "the detector flagged something that is not a wholesale re-export: {source}"
        );
    }
}
