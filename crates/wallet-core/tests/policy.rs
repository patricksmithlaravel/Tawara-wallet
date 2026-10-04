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
// The source a non-test build compiles, and what is provably test-only.
// ---------------------------------------------------------------------------

/// A module context: where the `mod name;` declarations of a source file
/// resolve. A crate root, a `mod.rs` or a `#[path]` file resolves them beside
/// itself; any other file in a directory named after it. An `include!`d file
/// shares the context of the file that includes it, since its items expand
/// into that file's module.
#[derive(Clone)]
struct ModFile {
    path: PathBuf,
    mod_rs: bool,
}

/// A source file to read, and the module context its declarations resolve in.
#[derive(Clone)]
struct Source {
    path: PathBuf,
    context: ModFile,
}

/// Whether a `cfg` predicate holds only in test builds: `test`, or
/// `all(test, ..)`. `any(test, ..)` and `not(test)` do not.
fn predicate_is_test_only(meta: &syn::Meta) -> bool {
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

/// One file a module declaration's attributes can select, and whether only
/// test builds select it.
#[derive(Debug)]
struct PathChoice {
    path: String,
    test_only: bool,
}

fn path_value(meta: &syn::Meta) -> Result<Option<String>, String> {
    match meta {
        syn::Meta::NameValue(nv) if nv.path.is_ident("path") => match &nv.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => Ok(Some(s.value())),
            _ => Err("a `path` attribute whose value is not a string literal".into()),
        },
        _ => Ok(None),
    }
}

fn cfg_attr_choices(
    attr_args: &syn::MetaList,
    enclosing_test: bool,
    out: &mut Vec<PathChoice>,
) -> Result<(), String> {
    let args = attr_args
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .map_err(|e| format!("a `cfg_attr` this check cannot read: {e}"))?;
    let mut args = args.into_iter();
    let predicate = args.next().ok_or("a `cfg_attr` with no predicate")?;
    let test_only = enclosing_test || predicate_is_test_only(&predicate);
    for meta in args {
        if let Some(path) = path_value(&meta)? {
            out.push(PathChoice { path, test_only });
        } else if let syn::Meta::List(inner) = &meta
            && inner.path.is_ident("cfg_attr")
        {
            cfg_attr_choices(inner, test_only, out)?;
        }
    }
    Ok(())
}

/// Every file a declaration's attributes can select: a direct `#[path]`, and
/// the `path` of every `#[cfg_attr(..)]`, nested ones included. rustc picks
/// one of them for each configuration; this check takes all of them.
fn path_choices(attrs: &[syn::Attribute]) -> Result<Vec<PathChoice>, String> {
    let mut out = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("path") {
            if let Some(path) = path_value(&attr.meta)? {
                out.push(PathChoice {
                    path,
                    test_only: false,
                });
            } else {
                return Err("a `#[path]` attribute that is not `path = \"...\"`".into());
            }
        } else if attr.path().is_ident("cfg_attr") {
            let syn::Meta::List(list) = &attr.meta else {
                return Err("a `cfg_attr` that is not a list".into());
            };
            cfg_attr_choices(list, false, &mut out)?;
        }
    }
    Ok(out)
}

/// An out-of-line `mod name;` declaration, with the inline modules it sits
/// in, its attributes, and whether it sits in test-only code (its own
/// attributes, or an enclosing inline module's).
struct ModDecl {
    inline: Vec<String>,
    name: String,
    attrs: Vec<syn::Attribute>,
    test_only: bool,
}

fn mod_decls(
    items: &[syn::Item],
    inline: &[String],
    enclosing_test: bool,
    out: &mut Vec<ModDecl>,
) -> Result<(), String> {
    for item in items {
        let syn::Item::Mod(m) = item else { continue };
        let test_only = enclosing_test || test_only(&m.attrs);
        match &m.content {
            None => out.push(ModDecl {
                inline: inline.to_vec(),
                name: m.ident.unraw().to_string(),
                attrs: m.attrs.clone(),
                test_only,
            }),
            Some((_, inner)) => {
                let choices = path_choices(&m.attrs)?;
                if choices.iter().any(|c| !c.test_only) && choices.len() > 1
                    || m.attrs.iter().any(|a| a.path().is_ident("cfg_attr")) && !choices.is_empty()
                {
                    return Err(format!(
                        "inline module `{}` takes a conditional or repeated path, which this \
                         check does not model",
                        m.ident
                    ));
                }
                let segment = choices
                    .first()
                    .map(|c| c.path.clone())
                    .unwrap_or_else(|| m.ident.unraw().to_string());
                let mut nested = inline.to_vec();
                nested.push(segment);
                mod_decls(inner, &nested, test_only, out)?;
            }
        }
    }
    Ok(())
}

/// Every `include!(..)` invocation in a syntax tree: `Some(path)` for a
/// string literal, `None` for anything else (`concat!`, `env!`, a macro
/// variable), which this check cannot resolve.
#[derive(Default)]
struct Includes(Vec<Option<String>>);

impl<'ast> syn::visit::Visit<'ast> for Includes {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "include")
        {
            self.0.push(
                syn::parse2::<syn::LitStr>(mac.tokens.clone())
                    .ok()
                    .map(|l| l.value()),
            );
        }
        syn::visit::visit_macro(self, mac);
    }
}

/// A source file's content as rustc may meet it: a file of items, or (for an
/// `include!`d file) a single expression.
enum Parsed {
    File(syn::File),
    Expr(syn::Expr),
}

fn parse_source(path: &Path) -> Result<Parsed, String> {
    let text = read(path);
    match syn::parse_file(&text) {
        Ok(f) => Ok(Parsed::File(f)),
        Err(file_err) => syn::parse_str::<syn::Expr>(&text)
            .map(Parsed::Expr)
            .map_err(|_| {
                format!(
                    "{} does not parse, so this check cannot say what it compiles: {file_err}",
                    path.display()
                )
            }),
    }
}

fn includes_in(parsed: &Parsed) -> Vec<Option<String>> {
    use syn::visit::Visit;
    let mut found = Includes::default();
    match parsed {
        Parsed::File(f) => found.visit_file(f),
        Parsed::Expr(e) => found.visit_expr(e),
    }
    found.0
}

/// Routes into source that the walk does not follow, in code a non-test
/// build compiles. rustc accepts an out-of-line `mod` inside a block (a
/// function body, a `const` initialiser) when it carries a `#[path]`, and the
/// walk reads `mod` only at item level; a macro's tokens are raw to `syn`, so a
/// `mod` or `include!` it expands to is not seen; and a macro from a
/// dependency can expand to `include!` of a path it is given. Each of these is
/// refused rather than modelled, which keeps the walk's set of production
/// files complete, and the exemption of test-only files sound.
#[derive(Default)]
struct Unfollowed {
    block_depth: usize,
    found: Vec<String>,
}

impl<'ast> syn::visit::Visit<'ast> for Unfollowed {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.block_depth += 1;
        syn::visit::visit_block(self, block);
        self.block_depth -= 1;
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if self.block_depth > 0 && m.content.is_none() {
            self.found.push(format!(
                "an out-of-line `mod {};` inside a block",
                m.ident.unraw()
            ));
        }
        syn::visit::visit_item_mod(self, m);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let is_include = mac
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "include");
        if !is_include {
            let name = mac.path.to_token_stream().to_string().replace(' ', "");
            let mut words = Vec::new();
            idents(mac.tokens.clone(), &mut words);
            if words.iter().any(|w| w == "mod" || w == "include") {
                self.found.push(format!(
                    "a `{name}!` whose tokens declare a module or use `include!`"
                ));
            } else if names_rust_file(mac.tokens.clone()) {
                self.found
                    .push(format!("a `{name}!` given the name of a `.rs` file"));
            }
        }
        syn::visit::visit_macro(self, mac);
    }
}

/// Whether a token stream holds a string literal naming a `.rs` file.
fn names_rust_file(tokens: proc_macro2::TokenStream) -> bool {
    tokens.into_iter().any(|tt| match tt {
        proc_macro2::TokenTree::Literal(lit) => lit
            .to_string()
            .trim_end_matches('#')
            .trim_end_matches('"')
            .ends_with(".rs"),
        proc_macro2::TokenTree::Group(g) => names_rust_file(g.stream()),
        _ => false,
    })
}

fn unfollowed_in(parsed: &Parsed) -> Vec<String> {
    use syn::visit::Visit;
    let mut found = Unfollowed::default();
    match parsed {
        Parsed::File(f) => found.visit_file(f),
        Parsed::Expr(e) => found.visit_expr(e),
    }
    found.found
}

/// Out-of-line `mod` declarations inside blocks, with the inline modules
/// (outside any block) they sit in and their attributes, where the walk can
/// resolve them: directly in a block, not under an inline module inside one
/// (rustc resolves those by other rules). Once `Unfollowed` has refused them in
/// non-test code, only test code holds them, and a test fixture loaded this way
/// is then exempt.
#[derive(Default)]
struct BlockMods {
    block_depth: usize,
    inline_in_block: usize,
    inline: Vec<String>,
    found: Vec<(Vec<String>, Vec<syn::Attribute>)>,
}

impl<'ast> syn::visit::Visit<'ast> for BlockMods {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.block_depth += 1;
        syn::visit::visit_block(self, block);
        self.block_depth -= 1;
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        match (&m.content, self.block_depth > 0) {
            (None, true) if self.inline_in_block == 0 => {
                self.found.push((self.inline.clone(), m.attrs.clone()));
            }
            (None, _) => {}
            (Some(_), true) => {
                self.inline_in_block += 1;
                syn::visit::visit_item_mod(self, m);
                self.inline_in_block -= 1;
            }
            (Some(_), false) => {
                let segment = path_choices(&m.attrs)
                    .ok()
                    .and_then(|c| c.into_iter().next())
                    .map(|c| c.path)
                    .unwrap_or_else(|| m.ident.unraw().to_string());
                self.inline.push(segment);
                syn::visit::visit_item_mod(self, m);
                self.inline.pop();
            }
        }
    }
}

fn block_mods_in(parsed: &Parsed) -> Vec<(Vec<String>, Vec<syn::Attribute>)> {
    use syn::visit::Visit;
    let mut found = BlockMods::default();
    match parsed {
        Parsed::File(f) => found.visit_file(f),
        Parsed::Expr(e) => found.visit_expr(e),
    }
    found.found
}

/// The files a `#[path]` on a `mod` inside a block can load: relative to the
/// enclosing module's directory, and to the declaring file's. Only test code
/// reaches this, and every production route is followed or refused, so taking
/// both cannot exempt a file a non-test build compiles.
fn block_mod_targets(
    context: &ModFile,
    here: &Path,
    inline: &[String],
    attrs: &[syn::Attribute],
) -> Result<Vec<Source>, String> {
    let (module_dir, _) = module_dirs(context, inline);
    let mut out = Vec::new();
    for choice in path_choices(attrs)? {
        for base in [module_dir.as_path(), here] {
            let file = ModFile {
                path: base.join(&choice.path),
                mod_rs: true,
            };
            out.push(Source {
                path: file.path.clone(),
                context: file,
            });
        }
    }
    Ok(out)
}

fn strip(parsed: &Parsed) -> Parsed {
    match parsed {
        Parsed::File(f) => {
            let mut f = f.clone();
            StripTestCode.visit_file_mut(&mut f);
            Parsed::File(f)
        }
        Parsed::Expr(e) => {
            let mut e = e.clone();
            StripTestCode.visit_expr_mut(&mut e);
            Parsed::Expr(e)
        }
    }
}

fn idents_of(parsed: &Parsed) -> Vec<String> {
    let mut out = Vec::new();
    match parsed {
        Parsed::File(f) => idents(f.to_token_stream(), &mut out),
        Parsed::Expr(e) => idents(e.to_token_stream(), &mut out),
    }
    out
}

fn normalise(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// The directory a declaration in `context`, inside `inline` modules,
/// resolves a default `name.rs` / `name/mod.rs` in, and a `#[path]` in.
fn module_dirs(context: &ModFile, inline: &[String]) -> (PathBuf, PathBuf) {
    let dir = context
        .path
        .parent()
        .expect("a source file has a directory");
    let module_dir = if context.mod_rs {
        dir.to_path_buf()
    } else {
        dir.join(context.path.file_stem().expect("a source file has a stem"))
    };
    let nested = inline.iter().fold(module_dir, |b, m| b.join(m));
    // Outside an inline module, `#[path]` is relative to the declaring file's
    // directory; inside one, to the inline module's directory.
    let path_base = if inline.is_empty() {
        dir.to_path_buf()
    } else {
        nested.clone()
    };
    (nested, path_base)
}

/// The file `mod name;` loads by default, if exactly one candidate exists.
fn default_mod_file(base: &Path, name: &str) -> Result<Option<ModFile>, String> {
    let flat = base.join(format!("{name}.rs"));
    let nested = base.join(name).join("mod.rs");
    match (flat.exists(), nested.exists()) {
        (true, false) => Ok(Some(ModFile {
            path: flat,
            mod_rs: false,
        })),
        (false, true) => Ok(Some(ModFile {
            path: nested,
            mod_rs: true,
        })),
        (true, true) => Err(format!(
            "both {} and {} exist",
            flat.display(),
            nested.display()
        )),
        (false, false) => Ok(None),
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

/// What a crate compiles in a non-test build, and what only its tests compile.
struct SourceWalk {
    /// Every file the module tree of a non-test target reaches, through `mod`
    /// (with every `#[path]` and `cfg_attr` path) and `include!`.
    reached: BTreeSet<PathBuf>,
    /// Files reachable only through test-only declarations: these, and only
    /// these, are exempt from the scan.
    exempt: BTreeSet<PathBuf>,
    /// Every file the scan reads: all Rust files under the crate except its
    /// top-level `tests/` and `benches/`, plus every file `reached` names,
    /// minus `exempt`. A file nothing declares is scanned as production code.
    scanned: BTreeSet<PathBuf>,
}

fn walk_sources(crate_dir: &Path) -> Result<SourceWalk, String> {
    let mut reached = BTreeSet::new();
    let mut test_targets: Vec<Source> = Vec::new();
    let mut stack: Vec<Source> = non_test_roots(crate_dir)
        .into_iter()
        .map(|path| Source {
            context: ModFile {
                path: path.clone(),
                mod_rs: true,
            },
            path,
        })
        .collect();

    while let Some(source) = stack.pop() {
        if !reached.insert(normalise(&source.path)) {
            continue;
        }
        let raw = parse_source(&source.path)?;
        let stripped = strip(&raw);
        let here = source.path.parent().expect("a source file has a directory");

        if let Some(what) = unfollowed_in(&stripped).first() {
            return Err(format!(
                "{} has {what} in non-test code; this check cannot see what it loads",
                source.path.display()
            ));
        }
        // What is left of `mod` inside blocks is test code: a fixture it
        // loads by `#[path]` is a test target.
        for (inline, attrs) in block_mods_in(&raw) {
            test_targets.extend(block_mod_targets(&source.context, here, &inline, &attrs)?);
        }

        // Out-of-line modules: every non-test choice is compiled in some
        // configuration; a test-only choice is a test target.
        let mut decls = Vec::new();
        if let Parsed::File(f) = &raw {
            mod_decls(&f.items, &[], false, &mut decls)?;
        }
        for decl in decls {
            let (default_base, path_base) = module_dirs(&source.context, &decl.inline);
            let choices = path_choices(&decl.attrs)?;
            let has_direct = decl.attrs.iter().any(|a| a.path().is_ident("path"));
            for choice in &choices {
                let file = ModFile {
                    path: path_base.join(&choice.path),
                    mod_rs: true,
                };
                let target = Source {
                    path: file.path.clone(),
                    context: file,
                };
                // A chosen file that does not exist is one rustc cannot compile
                // either, in the configurations that choose it.
                if decl.test_only || choice.test_only {
                    test_targets.push(target);
                } else if target.path.exists() {
                    stack.push(target);
                }
            }
            if !has_direct {
                match default_mod_file(&default_base, &decl.name)? {
                    Some(file) => {
                        let target = Source {
                            path: file.path.clone(),
                            context: file,
                        };
                        if decl.test_only {
                            test_targets.push(target);
                        } else {
                            stack.push(target);
                        }
                    }
                    None if choices.is_empty() && !decl.test_only => {
                        return Err(format!(
                            "`mod {};` in {} resolves to neither {name}.rs nor {name}/mod.rs \
                             under {}",
                            decl.name,
                            source.path.display(),
                            default_base.display(),
                            name = decl.name
                        ));
                    }
                    None => {}
                }
            }
        }

        // `include!`: in non-test code it must be a literal this check can
        // follow; in test-only code it is a test target.
        let non_test_includes = includes_in(&stripped);
        for inc in &non_test_includes {
            let Some(rel) = inc else {
                return Err(format!(
                    "{} uses `include!` with an argument that is not a string literal; this \
                     check cannot see what it loads",
                    source.path.display()
                ));
            };
            stack.push(Source {
                path: here.join(rel),
                context: source.context.clone(),
            });
        }
        let mut test_includes = includes_in(&raw);
        for inc in &non_test_includes {
            if let Some(pos) = test_includes.iter().position(|t| t == inc) {
                test_includes.remove(pos);
            }
        }
        for rel in test_includes.into_iter().flatten() {
            test_targets.push(Source {
                path: here.join(rel),
                context: source.context.clone(),
            });
        }
    }

    // Everything reachable from a test target, through any declaration.
    let mut test_reached = BTreeSet::new();
    while let Some(source) = test_targets.pop() {
        if !source.path.exists() || !test_reached.insert(normalise(&source.path)) {
            continue;
        }
        let Ok(raw) = parse_source(&source.path) else {
            continue;
        };
        let here = source.path.parent().expect("a source file has a directory");
        let mut decls = Vec::new();
        if let Parsed::File(f) = &raw {
            mod_decls(&f.items, &[], true, &mut decls)?;
        }
        for decl in decls {
            let (default_base, path_base) = module_dirs(&source.context, &decl.inline);
            for choice in path_choices(&decl.attrs)? {
                let file = ModFile {
                    path: path_base.join(&choice.path),
                    mod_rs: true,
                };
                test_targets.push(Source {
                    path: file.path.clone(),
                    context: file,
                });
            }
            if let Some(file) = default_mod_file(&default_base, &decl.name)? {
                test_targets.push(Source {
                    path: file.path.clone(),
                    context: file,
                });
            }
        }
        for rel in includes_in(&raw).into_iter().flatten() {
            test_targets.push(Source {
                path: here.join(rel),
                context: source.context.clone(),
            });
        }
        for (inline, attrs) in block_mods_in(&raw) {
            test_targets.extend(block_mod_targets(&source.context, here, &inline, &attrs)?);
        }
    }

    let exempt: BTreeSet<PathBuf> = test_reached.difference(&reached).cloned().collect();
    let mut scanned: BTreeSet<PathBuf> = rust_files(crate_dir)
        .into_iter()
        .filter(|p| {
            let mut parts = p
                .strip_prefix(crate_dir)
                .expect("walked from the crate directory")
                .components();
            !matches!(parts.next(), Some(c) if c.as_os_str() == "tests" || c.as_os_str() == "benches")
        })
        .map(|p| normalise(&p))
        .collect();
    scanned.extend(reached.iter().cloned());
    for e in &exempt {
        scanned.remove(e);
    }
    Ok(SourceWalk {
        reached,
        exempt,
        scanned,
    })
}

/// Every scanned file of `crate_dir`, parsed and stripped of test-only code.
fn scanned_sources(crate_dir: &Path) -> Vec<(PathBuf, Parsed)> {
    let walk = walk_sources(crate_dir).unwrap_or_else(|e| panic!("{e}"));
    walk.scanned
        .into_iter()
        .map(|path| {
            let parsed = parse_source(&path).unwrap_or_else(|e| panic!("{e}"));
            let stripped = strip(&parsed);
            (path, stripped)
        })
        .collect()
}

fn cheap_kdf_offenders(crate_dir: &Path) -> Vec<PathBuf> {
    scanned_sources(crate_dir)
        .into_iter()
        .filter(|(_, parsed)| idents_of(parsed).iter().any(|i| i == CHEAP_KDF))
        .map(|(path, _)| path)
        .collect()
}

/// `Kdf::CHEAP_FOR_TESTS` derives the store key at a cost chosen to make tests
/// fast, which is the cost an attacker with a stolen store would choose too.
/// Production code uses `Kdf::RECOMMENDED`. This holds every crate's non-test
/// code to never naming the cheap parameters.
///
/// The scan is conservative by construction. It reads every Rust file under a
/// crate except its top-level `tests/` and `benches/` (integration tests and
/// benchmarks), plus every file the module walk reaches, wherever it lives.
/// It exempts a file only when it is reachable solely through test-only
/// declarations (`#[cfg(test)] mod`, a `cfg_attr(test, path = ..)`, a module
/// nested in a test-only one, an `include!` in test-only code). The walk
/// follows `mod`, every `#[path]` and `cfg_attr` path, and literal
/// `include!`s; a non-literal `include!` in non-test code is refused, since
/// what it loads cannot be read. In every file, code under `#[cfg(test)]`,
/// `#[cfg(all(test, ..))]` or `#[test]` is removed before looking.
///
/// `spikes/` is outside the scan on purpose: it is phase 1's throwaway
/// feasibility code, which uses the cheap parameters for on-device store
/// checks, and nothing in it is built into a shipped artifact.
#[test]
fn cheap_kdf_appears_only_in_test_code() {
    let crates_dir = workspace_root().join("crates");
    let mut reached = BTreeSet::new();
    let mut offenders = Vec::new();
    for dir in crate_dirs() {
        let walk = walk_sources(&dir).unwrap_or_else(|e| panic!("{e}"));
        reached.extend(walk.reached);
        offenders.extend(cheap_kdf_offenders(&dir));
    }

    // A walk that reached nothing would pass. Each crate's root must be in it.
    for root in [
        "wallet-core/src/lib.rs",
        "app/src/lib.rs",
        "desktop/src/main.rs",
        "mobile/src/lib.rs",
    ] {
        let root = normalise(&crates_dir.join(root));
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

/// Lays out a probe crate under the temporary directory and returns its root.
fn probe_crate(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tawara-policy-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    for (rel, body) in files {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
    normalise(&dir)
}

/// The walk and the scan, held to a crate laid out on disk where the answer
/// is known. Production code reached through a nested module, an
/// `include!`, a `cfg_attr` path, or nothing at all is flagged; code reached
/// only through test-only declarations is exempt; an integration test is not
/// scanned.
#[test]
fn the_source_walk_scans_what_rustc_may_compile() {
    const CHEAP: &str = "const K: Kdf = Kdf::CHEAP_FOR_TESTS;\n";
    const SAFE: &str = "const K: Kdf = Kdf::RECOMMENDED;\n";
    let dir = probe_crate(
        "walk",
        &[
            ("Cargo.toml", "[package]\nname = \"probe\"\n"),
            (
                "src/lib.rs",
                "mod a;\n\
                 #[cfg(test)] mod tests;\n\
                 #[path = \"elsewhere/c.rs\"] mod c;\n\
                 mod inline { mod d; }\n\
                 include!(\"kdf_impl.rs\");\n\
                 #[cfg_attr(unix, path = \"unix.rs\")] mod platform;\n\
                 #[cfg_attr(windows, path = \"win_only.rs\")] mod plat2;\n\
                 #[cfg_attr(test, path = \"test_path.rs\")] mod tp;\n\
                 #[cfg(test)] mod t2 { mod deep; }\n\
                 #[cfg(test)] mod t3 { include!(\"fixture.rs\"); }\n\
                 #[test] fn fx() { #[path = \"fixture_fn.rs\"] mod f; }\n\
                 #[cfg(test)] mod t5 { #[test] fn fx() { #[path = \"fixture_t5.rs\"] mod f; } }\n\
                 mod shared;\n\
                 #[cfg(test)] #[path = \"shared.rs\"] mod shared_t;\n\
                 #[path = \"d.rs\"] #[cfg_attr(unix, path = \"missing.rs\")] mod dd;\n",
            ),
            ("src/tests.rs", CHEAP),
            ("src/a.rs", "mod b;\n"),
            ("src/a/b.rs", CHEAP),
            ("src/elsewhere/c.rs", SAFE),
            ("src/inline/d.rs", SAFE),
            ("src/kdf_impl.rs", CHEAP),
            ("src/platform.rs", SAFE),
            ("src/unix.rs", CHEAP),
            ("src/win_only.rs", SAFE),
            ("src/tp.rs", SAFE),
            ("src/test_path.rs", CHEAP),
            ("src/t2/deep.rs", CHEAP),
            ("src/fixture.rs", CHEAP),
            ("src/fixture_fn.rs", CHEAP),
            ("src/t5/fixture_t5.rs", CHEAP),
            ("src/shared.rs", CHEAP),
            ("src/d.rs", SAFE),
            ("src/stray.rs", CHEAP),
            ("tests/it.rs", CHEAP),
        ],
    );
    let rel = |set: &BTreeSet<PathBuf>| -> BTreeSet<String> {
        set.iter()
            .map(|p| {
                p.strip_prefix(&dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    };
    let walk = walk_sources(&dir).expect("the probe crate walks");
    let reached = rel(&walk.reached);
    let exempt = rel(&walk.exempt);
    let offenders = rel(&cheap_kdf_offenders(&dir).into_iter().collect());
    let _ = fs::remove_dir_all(&dir);

    for expected in [
        "src/lib.rs",
        "src/a.rs",
        "src/a/b.rs",
        "src/elsewhere/c.rs",
        "src/inline/d.rs",
        "src/kdf_impl.rs",
        "src/platform.rs",
        "src/unix.rs",
        "src/win_only.rs",
        "src/tp.rs",
        "src/shared.rs",
        "src/d.rs",
    ] {
        assert!(
            reached.contains(expected),
            "the walk did not reach {expected}: {reached:?}"
        );
    }
    assert_eq!(
        exempt,
        BTreeSet::from(
            [
                "src/tests.rs",
                "src/test_path.rs",
                "src/t2/deep.rs",
                "src/fixture.rs",
                "src/fixture_fn.rs",
                "src/t5/fixture_t5.rs"
            ]
            .map(String::from)
        ),
        "only files reached solely through test-only declarations are exempt"
    );
    assert_eq!(
        offenders,
        BTreeSet::from(
            [
                "src/a/b.rs",
                "src/kdf_impl.rs",
                "src/unix.rs",
                "src/stray.rs",
                "src/shared.rs"
            ]
            .map(String::from)
        ),
        "production code reached by any route, or by none, is flagged, even when a test also \
         reaches it; tests/ is not scanned"
    );
}

/// What the walk cannot read, it refuses rather than passes.
#[test]
fn the_source_walk_refuses_what_it_cannot_read() {
    let cases: [(&str, &str); 6] = [
        (
            "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));\n",
            "not a string literal",
        ),
        (
            "#[cfg_attr(unix, path = \"u\")] mod inline_cond { }\n",
            "conditional or repeated path",
        ),
        (
            "pub fn f() -> u32 { #[path = \"../../outside.rs\"] mod imp; 0 }\n",
            "inside a block",
        ),
        (
            "macro_rules! m { () => { #[path = \"x.rs\"] mod imp; }; }\n",
            "declare a module or use `include!`",
        ),
        (
            "macro_rules! l { () => { include!(\"x.rs\"); }; }\nl!();\n",
            "declare a module or use `include!`",
        ),
        (
            "some_dep::load!(\"../../outside.rs\");\n",
            "given the name of a `.rs` file",
        ),
    ];
    for (i, (lib, expected)) in cases.into_iter().enumerate() {
        let dir = probe_crate(
            &format!("refuse{i}"),
            &[
                ("Cargo.toml", "[package]\nname = \"probe\"\n"),
                ("src/lib.rs", lib),
            ],
        );
        let result = walk_sources(&dir).map(|_| ());
        let _ = fs::remove_dir_all(&dir);
        match result {
            Err(e) => assert!(e.contains(expected), "refused for the wrong reason: {e}"),
            Ok(()) => panic!("the walk accepted what it cannot read: {lib}"),
        }
    }
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
    // from any file the source scan reads.
    for (path, parsed) in scanned_sources(&crates_dir.join("wallet-core")) {
        let Parsed::File(file) = parsed else { continue };
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
