//! Post-parse rename pass that shortens private-fn names, fn parameters
//! (public fns included), and local `let` bindings so `rustminify` can
//! produce smaller output.
//!
//! Two passes over the aggregated library AST:
//!
//! 1. **Fn renaming.** Every module-level `fn` whose visibility is
//!    `Inherited` or `pub(crate)` gets a short name starting with an
//!    uppercase letter (`A`, `B`, ..., `Z`, `Aa`, `Ab`, ...). Call sites
//!    are rewritten by replacing the last segment of every `Path` (and
//!    matching `UseTree::Name`/`Rename`) whose ident equals the old name.
//!
//! 2. **Locals + params.** For every fn body (regardless of visibility) we
//!    do scope-aware renaming of `Pat::Ident` bindings introduced by fn
//!    params, `let` bindings, `for`/`match`/`if let`/`while let` patterns,
//!    and closure parameters. References are rewritten by matching
//!    single-segment `Expr::Path`s to the innermost enclosing binding.
//!    New names start with a lowercase letter (`a`, `b`, ..., `z`, `aa`,
//!    `ab`, ...).
//!
//! Both name pools use `[a-zA-Z0-9_]` from the second character onwards.
//! The two pools have disjoint first-character classes so a renamed local
//! can never collide with a renamed fn.
//!
//! Macro invocations are opaque to `syn` — `println!("{x}")` and
//! `dbg!(x + y)` keep `x` inside `Macro.tokens` that we can't walk with
//! `VisitMut`. Both passes therefore rewrite tokens (and `{ident}` /
//! `{ident:spec}` captures in string literals) inside macro invocations
//! directly, so bindings and fns referenced from macros still get their
//! short names. Idents preceded by `.` (field access) are left alone;
//! Pass 2 additionally leaves idents preceded by `::` alone since a
//! local var can't live on a path segment.

use proc_macro2::{Spacing, TokenStream, TokenTree};
use std::collections::{HashMap, HashSet};
use syn::__private::ToTokens;
use syn::visit::Visit;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Block, Expr, FnArg, Ident, ImplItem, ImplItemFn, Item, ItemFn, Pat, Path, Signature, Stmt,
    TraitItem, TraitItemFn, UseTree, Visibility,
};

pub fn rename(code: &str) -> Result<String, syn::Error> {
    let mut file: syn::File = syn::parse_str(code)?;

    // ---- Pass 1: locals + params (scope-aware) ----
    // Runs first so that pattern bindings that happen to share a name
    // with a private item (e.g. `match self { Foo(yes, _) => yes }`
    // inside a method `fn yes()`) get their short names before the
    // item pass runs — otherwise the item pass would blindly rewrite
    // the bare `yes` in `=> yes` to the method's new name.
    let mut renamer = LocalRenamer {
        scopes: Vec::new(),
        fn_forbidden: HashSet::new(),
        fn_used_new: HashSet::new(),
        fn_local_counter: 0,
    };
    renamer.visit_file_mut(&mut file);

    // ---- Pass 2: private item renaming (fns + types + methods) ----
    let mut items = ItemNameCollector::default();
    items.visit_file(&file);
    // Avoid every ident that appears anywhere — a rename rewrites the
    // whole file, so any shadow would silently break.
    let all_idents_before = collect_all_idents(&file);
    let item_map = items.build_rename_map(&all_idents_before);
    if !item_map.is_empty() {
        ItemRenamer {
            map: &item_map,
            mask_stack: Vec::new(),
            in_trait_impl: false,
        }
        .visit_file_mut(&mut file);
    }

    Ok(prettyplease::unparse(&file))
}

// ---------------------------------------------------------------------------
// Token/ident scanning helpers
// ---------------------------------------------------------------------------

fn collect_ident_words(stream: TokenStream, out: &mut HashSet<String>) {
    for tt in stream {
        match tt {
            TokenTree::Ident(i) => {
                out.insert(i.to_string());
            }
            TokenTree::Group(g) => collect_ident_words(g.stream(), out),
            TokenTree::Literal(lit) => {
                for w in lit
                    .to_string()
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                {
                    if !w.is_empty() {
                        out.insert(w.to_string());
                    }
                }
            }
            TokenTree::Punct(_) => {}
        }
    }
}

fn collect_all_idents(file: &syn::File) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_ident_words(file.to_token_stream(), &mut out);
    out
}

/// Rewrite ident tokens (and `{ident}` / `{ident:spec}` captures in
/// string literals) inside a macro token stream by consulting `lookup`.
///
/// `skip_after_dot` — if true, idents immediately after `.` are left
/// alone (used by the local-rename pass; a local var can't sit on a
/// field/method access). The item-rename pass sets this to `false` so
/// method calls inside macros get renamed too.
///
/// `skip_after_colon_colon` — same idea for `::` (path segments). Local
/// pass sets true; item pass sets false because `mod::foo`/`Type::method`
/// are valid rewrite sites.
fn rewrite_macro_tokens(
    stream: TokenStream,
    lookup: &dyn Fn(&str) -> Option<String>,
    skip_after_dot: bool,
    skip_after_colon_colon: bool,
) -> TokenStream {
    let tts: Vec<TokenTree> = stream.into_iter().collect();
    let mut out: Vec<TokenTree> = Vec::with_capacity(tts.len());
    for tt in tts {
        let new = match tt {
            TokenTree::Ident(id) => {
                if is_field_or_path_tail(&out, skip_after_dot, skip_after_colon_colon) {
                    TokenTree::Ident(id)
                } else {
                    let name = id.to_string();
                    if let Some(new_name) = lookup(&name) {
                        TokenTree::Ident(proc_macro2::Ident::new(&new_name, id.span()))
                    } else {
                        TokenTree::Ident(id)
                    }
                }
            }
            TokenTree::Group(g) => {
                let inner =
                    rewrite_macro_tokens(g.stream(), lookup, skip_after_dot, skip_after_colon_colon);
                let mut ng = proc_macro2::Group::new(g.delimiter(), inner);
                ng.set_span(g.span());
                TokenTree::Group(ng)
            }
            TokenTree::Literal(lit) => rewrite_str_literal(lit, lookup),
            TokenTree::Punct(p) => TokenTree::Punct(p),
        };
        out.push(new);
    }
    out.into_iter().collect()
}

fn is_field_or_path_tail(
    out: &[TokenTree],
    skip_after_dot: bool,
    skip_after_colon_colon: bool,
) -> bool {
    let last = match out.last() {
        Some(TokenTree::Punct(p)) => p,
        _ => return false,
    };
    if skip_after_dot && last.as_char() == '.' {
        return true;
    }
    if skip_after_colon_colon && last.as_char() == ':' {
        if let Some(TokenTree::Punct(p2)) = out.get(out.len().saturating_sub(2)) {
            if p2.as_char() == ':' && p2.spacing() == Spacing::Joint {
                return true;
            }
        }
    }
    false
}

fn rewrite_str_literal(
    lit: proc_macro2::Literal,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> TokenTree {
    let syn_lit = syn::Lit::new(lit.clone());
    if let syn::Lit::Str(s) = syn_lit {
        let value = s.value();
        let new_value = rewrite_format_captures(&value, lookup);
        if new_value != value {
            let new_s = syn::LitStr::new(&new_value, s.span());
            let mut ts = TokenStream::new();
            new_s.to_tokens(&mut ts);
            if let Some(tt) = ts.into_iter().next() {
                return tt;
            }
        }
    }
    TokenTree::Literal(lit)
}

/// Rewrite `{IDENT}` / `{IDENT:spec}` implicit format captures in a
/// format string. Leaves `{{` / `}}` escapes, positional / index args,
/// and anything after `:` (format spec) alone.
fn rewrite_format_captures(source: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                out.push('{');
                out.push('{');
                chars.next();
            }
            '}' if chars.peek() == Some(&'}') => {
                out.push('}');
                out.push('}');
                chars.next();
            }
            '{' => {
                out.push('{');
                let mut arg = String::new();
                while let Some(&pc) = chars.peek() {
                    if pc == ':' || pc == '}' {
                        break;
                    }
                    arg.push(pc);
                    chars.next();
                }
                if is_ident_str(&arg) {
                    if let Some(new) = lookup(&arg) {
                        out.push_str(&new);
                    } else {
                        out.push_str(&arg);
                    }
                } else {
                    out.push_str(&arg);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn is_ident_str(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ---------------------------------------------------------------------------
// Pass 1: private item renaming (fns + types)
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
enum ItemKind {
    Fn,
    Struct,
    Enum,
    Union,
    TypeAlias,
    Const,
    Static,
    Method, // private inherent-impl fn
}

#[derive(Default)]
struct ItemNameCollector {
    /// Names we must not touch — public items, methods, macro names,
    /// enum variants, modules, and any ident brought in via `use`.
    non_renamable: HashSet<String>,
    /// Candidates for renaming, tagged with their kind so we can
    /// detect same-name/different-kind ambiguity.
    candidates: Vec<(String, ItemKind)>,
}

impl<'ast> Visit<'ast> for ItemNameCollector {
    fn visit_item(&mut self, node: &'ast Item) {
        match node {
            Item::Fn(f) => self.consider(&f.sig.ident, &f.vis, ItemKind::Fn, is_renamable_fn(f)),
            Item::Struct(s) => {
                self.consider(&s.ident, &s.vis, ItemKind::Struct, is_renamable_attrs(&s.attrs));
                for field in s.fields.iter() {
                    if let Some(id) = &field.ident {
                        self.non_renamable.insert(id.to_string());
                    }
                }
            }
            Item::Enum(e) => {
                self.consider(&e.ident, &e.vis, ItemKind::Enum, is_renamable_attrs(&e.attrs));
                for v in &e.variants {
                    self.record(&v.ident);
                    for field in v.fields.iter() {
                        if let Some(id) = &field.ident {
                            self.non_renamable.insert(id.to_string());
                        }
                    }
                }
            }
            Item::Union(u) => {
                self.consider(&u.ident, &u.vis, ItemKind::Union, is_renamable_attrs(&u.attrs));
                for field in u.fields.named.iter() {
                    if let Some(id) = &field.ident {
                        self.non_renamable.insert(id.to_string());
                    }
                }
            }
            Item::Type(t) => self.consider(
                &t.ident,
                &t.vis,
                ItemKind::TypeAlias,
                is_renamable_attrs(&t.attrs),
            ),
            Item::Const(c) => self.consider(
                &c.ident,
                &c.vis,
                ItemKind::Const,
                is_renamable_attrs(&c.attrs),
            ),
            Item::Static(s) => self.consider(
                &s.ident,
                &s.vis,
                ItemKind::Static,
                is_renamable_attrs(&s.attrs),
            ),
            Item::Trait(t) => self.record(&t.ident),
            Item::TraitAlias(t) => self.record(&t.ident),
            Item::Mod(m) => self.record(&m.ident),
            Item::Macro(m) => {
                if let Some(id) = &m.ident {
                    self.record(id);
                }
            }
            Item::Impl(imp) => {
                // Handle impl items ourselves so we can distinguish
                // inherent (private methods → candidates) from trait
                // impls (all items → non_renamable, contract).
                let is_trait_impl = imp.trait_.is_some();
                for impl_item in &imp.items {
                    match impl_item {
                        ImplItem::Fn(f) => {
                            let name = f.sig.ident.to_string();
                            if !is_trait_impl
                                && is_private(&f.vis)
                                && is_renamable_attrs(&f.attrs)
                                && name != "main"
                            {
                                self.candidates.push((name, ItemKind::Method));
                            } else {
                                self.non_renamable.insert(name);
                            }
                        }
                        ImplItem::Const(c) => self.record(&c.ident),
                        ImplItem::Type(t) => self.record(&t.ident),
                        _ => {}
                    }
                }
                // Continue into impl body so nested macros/consts get
                // their idents into non_renamable via visit_item.
                syn::visit::visit_item_impl(self, imp);
                return;
            }
            _ => {}
        }
        syn::visit::visit_item(self, node);
    }

    fn visit_impl_item(&mut self, node: &'ast ImplItem) {
        // Fn/Const/Type here are only reached when descended into via
        // syn::visit::visit_item_impl from `Item::Impl` above; we've
        // already handled them (fns dispatched by trait-vs-inherent,
        // consts/types recorded). Nothing left to do — but keep the
        // recursion so nested macros in impl items are visited too.
        syn::visit::visit_impl_item(self, node);
    }

    fn visit_trait_item(&mut self, node: &'ast TraitItem) {
        match node {
            TraitItem::Fn(f) => self.record(&f.sig.ident),
            TraitItem::Const(c) => self.record(&c.ident),
            TraitItem::Type(t) => self.record(&t.ident),
            _ => {}
        }
        syn::visit::visit_trait_item(self, node);
    }

    fn visit_use_tree(&mut self, node: &'ast UseTree) {
        match node {
            UseTree::Name(n) => {
                self.non_renamable.insert(n.ident.to_string());
            }
            UseTree::Rename(r) => {
                self.non_renamable.insert(r.ident.to_string());
                self.non_renamable.insert(r.rename.to_string());
            }
            _ => {}
        }
        syn::visit::visit_use_tree(self, node);
    }
}

impl ItemNameCollector {
    fn record(&mut self, ident: &Ident) {
        self.non_renamable.insert(ident.to_string());
    }

    fn consider(&mut self, ident: &Ident, vis: &Visibility, kind: ItemKind, renamable_attrs: bool) {
        let name = ident.to_string();
        if is_private(vis) && renamable_attrs {
            self.candidates.push((name, kind));
        } else {
            self.non_renamable.insert(name);
        }
    }

    fn build_rename_map(&self, avoid: &HashSet<String>) -> HashMap<String, String> {
        // Group candidates by name — if a name shows up under two
        // different kinds (e.g. an inherent fn `foo` and a nested
        // struct `foo`), we can't safely rewrite paths ambiguously,
        // so skip both.
        let mut kinds_by_name: HashMap<&str, HashSet<ItemKind>> = HashMap::new();
        for (name, kind) in &self.candidates {
            kinds_by_name.entry(name.as_str()).or_default().insert(*kind);
        }

        let mut map: HashMap<String, String> = HashMap::new();
        let mut counter: u64 = 0;
        let mut used: HashSet<String> = avoid.clone();
        for (name, _kind) in &self.candidates {
            if map.contains_key(name) {
                continue;
            }
            if self.non_renamable.contains(name) {
                continue;
            }
            if kinds_by_name.get(name.as_str()).map_or(0, |s| s.len()) > 1 {
                continue;
            }
            let new = loop {
                counter += 1;
                let candidate = short_name(counter, FIRST_UPPER, REST_ALPHABET);
                if !used.contains(&candidate) {
                    break candidate;
                }
            };
            used.insert(new.clone());
            map.insert(name.clone(), new);
        }
        map
    }
}

fn is_private(vis: &Visibility) -> bool {
    match vis {
        Visibility::Inherited => true,
        Visibility::Restricted(r) => r.path.is_ident("crate"),
        Visibility::Public(_) => false,
    }
}

fn is_renamable_fn(f: &ItemFn) -> bool {
    if f.sig.ident == "main" {
        return false;
    }
    for a in &f.attrs {
        let p = a.path();
        if p.is_ident("no_mangle")
            || p.is_ident("export_name")
            || p.is_ident("used")
            || p.is_ident("proc_macro")
            || p.is_ident("proc_macro_derive")
            || p.is_ident("proc_macro_attribute")
            || p.is_ident("test")
            || p.is_ident("bench")
            || p.is_ident("start")
            || p.is_ident("panic_handler")
        {
            return false;
        }
    }
    true
}

fn is_renamable_attrs(attrs: &[syn::Attribute]) -> bool {
    for a in attrs {
        let p = a.path();
        if p.is_ident("no_mangle") || p.is_ident("export_name") || p.is_ident("used") {
            return false;
        }
    }
    true
}

struct ItemRenamer<'a> {
    map: &'a HashMap<String, String>,
    /// Stack of generic-param names in scope. Rewriting an ident is
    /// suppressed when it matches any masked name, so a generic
    /// parameter like `F` in `fn build<F: ...>(f: F)` doesn't get
    /// substituted with the rename target of a private const named
    /// `F` in the surrounding module.
    mask_stack: Vec<HashSet<String>>,
    /// True while we're inside a trait impl block — its method idents
    /// must match the trait contract, so we never rename them.
    in_trait_impl: bool,
}

impl ItemRenamer<'_> {
    fn is_masked(&self, name: &str) -> bool {
        self.mask_stack.iter().any(|s| s.contains(name))
    }

    fn rename_ident(&self, id: &mut Ident) {
        let name = id.to_string();
        if self.is_masked(&name) {
            return;
        }
        if let Some(new) = self.map.get(&name) {
            *id = Ident::new(new, id.span());
        }
    }

    fn enter_generics(&mut self, generics: &syn::Generics) {
        let mask: HashSet<String> = generics
            .params
            .iter()
            .filter_map(|p| match p {
                syn::GenericParam::Type(t) => Some(t.ident.to_string()),
                syn::GenericParam::Const(c) => Some(c.ident.to_string()),
                syn::GenericParam::Lifetime(_) => None,
            })
            .collect();
        self.mask_stack.push(mask);
    }

    fn exit_generics(&mut self) {
        self.mask_stack.pop();
    }
}

impl VisitMut for ItemRenamer<'_> {
    fn visit_item_fn_mut(&mut self, node: &mut ItemFn) {
        self.rename_ident(&mut node.sig.ident);
        self.enter_generics(&node.sig.generics);
        visit_mut::visit_item_fn_mut(self, node);
        self.exit_generics();
    }

    fn visit_item_struct_mut(&mut self, node: &mut syn::ItemStruct) {
        self.rename_ident(&mut node.ident);
        self.enter_generics(&node.generics);
        visit_mut::visit_item_struct_mut(self, node);
        self.exit_generics();
    }

    fn visit_item_enum_mut(&mut self, node: &mut syn::ItemEnum) {
        self.rename_ident(&mut node.ident);
        self.enter_generics(&node.generics);
        visit_mut::visit_item_enum_mut(self, node);
        self.exit_generics();
    }

    fn visit_item_union_mut(&mut self, node: &mut syn::ItemUnion) {
        self.rename_ident(&mut node.ident);
        self.enter_generics(&node.generics);
        visit_mut::visit_item_union_mut(self, node);
        self.exit_generics();
    }

    fn visit_item_type_mut(&mut self, node: &mut syn::ItemType) {
        self.rename_ident(&mut node.ident);
        self.enter_generics(&node.generics);
        visit_mut::visit_item_type_mut(self, node);
        self.exit_generics();
    }

    fn visit_item_const_mut(&mut self, node: &mut syn::ItemConst) {
        self.rename_ident(&mut node.ident);
        visit_mut::visit_item_const_mut(self, node);
    }

    fn visit_item_static_mut(&mut self, node: &mut syn::ItemStatic) {
        self.rename_ident(&mut node.ident);
        visit_mut::visit_item_static_mut(self, node);
    }

    fn visit_item_impl_mut(&mut self, node: &mut syn::ItemImpl) {
        self.enter_generics(&node.generics);
        let prev = self.in_trait_impl;
        self.in_trait_impl = node.trait_.is_some();
        visit_mut::visit_item_impl_mut(self, node);
        self.in_trait_impl = prev;
        self.exit_generics();
    }

    fn visit_item_trait_mut(&mut self, node: &mut syn::ItemTrait) {
        self.enter_generics(&node.generics);
        visit_mut::visit_item_trait_mut(self, node);
        self.exit_generics();
    }

    fn visit_item_trait_alias_mut(&mut self, node: &mut syn::ItemTraitAlias) {
        self.enter_generics(&node.generics);
        visit_mut::visit_item_trait_alias_mut(self, node);
        self.exit_generics();
    }

    fn visit_impl_item_fn_mut(&mut self, node: &mut ImplItemFn) {
        // Only rename the method DEF if we're in an inherent impl.
        // Trait impls must keep names matching the trait contract.
        if !self.in_trait_impl {
            self.rename_ident(&mut node.sig.ident);
        }
        self.enter_generics(&node.sig.generics);
        visit_mut::visit_impl_item_fn_mut(self, node);
        self.exit_generics();
    }

    fn visit_expr_method_call_mut(&mut self, node: &mut syn::ExprMethodCall) {
        // Method-call sites `x.foo()` — always safe to rewrite,
        // since our rename map only contains method names that
        // passed the ambiguity + field-collision safety filters.
        self.rename_ident(&mut node.method);
        visit_mut::visit_expr_method_call_mut(self, node);
    }

    fn visit_impl_item_type_mut(&mut self, node: &mut syn::ImplItemType) {
        self.enter_generics(&node.generics);
        visit_mut::visit_impl_item_type_mut(self, node);
        self.exit_generics();
    }

    fn visit_trait_item_fn_mut(&mut self, node: &mut TraitItemFn) {
        self.enter_generics(&node.sig.generics);
        visit_mut::visit_trait_item_fn_mut(self, node);
        self.exit_generics();
    }

    fn visit_trait_item_type_mut(&mut self, node: &mut syn::TraitItemType) {
        self.enter_generics(&node.generics);
        visit_mut::visit_trait_item_type_mut(self, node);
        self.exit_generics();
    }

    fn visit_path_mut(&mut self, node: &mut Path) {
        // Rewrite ANY segment matching a private item — types show up
        // in the middle of paths (`MyType::method`, `MyEnum::Variant`),
        // not only at the end.
        for seg in node.segments.iter_mut() {
            self.rename_ident(&mut seg.ident);
        }
        visit_mut::visit_path_mut(self, node);
    }

    fn visit_use_tree_mut(&mut self, node: &mut UseTree) {
        match node {
            UseTree::Name(n) => self.rename_ident(&mut n.ident),
            UseTree::Rename(r) => self.rename_ident(&mut r.ident),
            _ => {}
        }
        visit_mut::visit_use_tree_mut(self, node);
    }

    fn visit_macro_mut(&mut self, node: &mut syn::Macro) {
        // Rewrite item-name refs inside macro token streams. Don't
        // skip after `::` — `mod::foo` / `Type::method` are valid.
        // Respect the generic-param mask so `dbg!(F)` inside a fn
        // with `<F>` doesn't get rewritten.
        let map = self.map;
        let mask_stack = &self.mask_stack;
        let lookup = |name: &str| {
            if mask_stack.iter().any(|s| s.contains(name)) {
                return None;
            }
            map.get(name).cloned()
        };
        let tokens = std::mem::take(&mut node.tokens);
        // Item rename also rewrites `.method` and `::foo` inside macros
        // so private inherent-method renames land there too.
        node.tokens = rewrite_macro_tokens(tokens, &lookup, false, false);
    }
}

// ---------------------------------------------------------------------------
// Pass 2: locals + params (scope-aware)
// ---------------------------------------------------------------------------

struct LocalRenamer {
    scopes: Vec<HashMap<String, String>>,
    /// Per-fn set: every ident that appears anywhere in the current fn,
    /// snapshotted before we start renaming inside it. New short names
    /// must avoid these so we don't collide with a name that stays put.
    fn_forbidden: HashSet<String>,
    /// Per-fn: names we've generated so far inside this fn.
    fn_used_new: HashSet<String>,
    /// Per-fn: monotonic counter for name generation.
    fn_local_counter: u64,
}

impl LocalRenamer {
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }
    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn resolve(&self, name: &str) -> Option<String> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn bind(&mut self, old: &str) -> Option<String> {
        // Names starting with `_` might be intentional; leave them.
        if old.starts_with('_') {
            return None;
        }
        loop {
            self.fn_local_counter += 1;
            let candidate = short_name(self.fn_local_counter, FIRST_LOWER, REST_ALPHABET);
            if self.fn_forbidden.contains(&candidate) || self.fn_used_new.contains(&candidate) {
                continue;
            }
            self.fn_used_new.insert(candidate.clone());
            if let Some(top) = self.scopes.last_mut() {
                top.insert(old.to_string(), candidate.clone());
            }
            return Some(candidate);
        }
    }

    fn rename_pat_binding(&mut self, pat: &mut Pat) {
        match pat {
            Pat::Ident(pi) => {
                // Skip constructor-style idents; they are enum variant
                // patterns like `None` rather than bindings.
                if starts_uppercase(&pi.ident) {
                    if let Some((_, sub)) = &mut pi.subpat {
                        self.rename_pat_binding(sub);
                    }
                    return;
                }
                let old = pi.ident.to_string();
                if let Some(new) = self.bind(&old) {
                    pi.ident = Ident::new(&new, pi.ident.span());
                }
                if let Some((_, sub)) = &mut pi.subpat {
                    self.rename_pat_binding(sub);
                }
            }
            Pat::Tuple(t) => {
                for e in &mut t.elems {
                    self.rename_pat_binding(e);
                }
            }
            Pat::TupleStruct(t) => {
                for e in &mut t.elems {
                    self.rename_pat_binding(e);
                }
            }
            Pat::Struct(s) => {
                for f in &mut s.fields {
                    let member_name = named_member(&f.member);
                    self.rename_pat_binding(&mut f.pat);
                    // Shorthand `Foo { x }` = `Foo { x: x }`; if we renamed
                    // the pat, we must expand to `Foo { x: NEW }` by
                    // inserting the colon so syn stops emitting shorthand.
                    if f.colon_token.is_none() {
                        let pat_name = pat_ident_name(&f.pat);
                        if member_name != pat_name {
                            f.colon_token = Some(Default::default());
                        }
                    }
                }
            }
            Pat::Or(o) => {
                for c in &mut o.cases {
                    self.rename_pat_binding(c);
                }
            }
            Pat::Reference(r) => self.rename_pat_binding(&mut r.pat),
            Pat::Paren(p) => self.rename_pat_binding(&mut p.pat),
            Pat::Slice(s) => {
                for e in &mut s.elems {
                    self.rename_pat_binding(e);
                }
            }
            Pat::Type(t) => self.rename_pat_binding(&mut t.pat),
            _ => {}
        }
    }

    fn rename_expr_here(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Path(p) => {
                if p.qself.is_none()
                    && p.path.leading_colon.is_none()
                    && p.path.segments.len() == 1
                {
                    let seg = &mut p.path.segments[0];
                    if seg.arguments.is_empty() {
                        let name = seg.ident.to_string();
                        if let Some(new) = self.resolve(&name) {
                            seg.ident = Ident::new(&new, seg.ident.span());
                        }
                    }
                }
            }
            Expr::Block(b) => self.rename_block(&mut b.block),
            Expr::If(i) => {
                self.push();
                self.rename_cond(&mut i.cond);
                self.rename_block(&mut i.then_branch);
                self.pop();
                if let Some((_, else_)) = &mut i.else_branch {
                    self.rename_expr_here(else_);
                }
            }
            Expr::While(w) => {
                self.push();
                self.rename_cond(&mut w.cond);
                self.rename_block(&mut w.body);
                self.pop();
            }
            Expr::ForLoop(f) => {
                self.rename_expr_here(&mut f.expr);
                self.push();
                self.rename_pat_binding(&mut f.pat);
                self.rename_block(&mut f.body);
                self.pop();
            }
            Expr::Match(m) => {
                self.rename_expr_here(&mut m.expr);
                for arm in &mut m.arms {
                    self.push();
                    self.rename_pat_binding(&mut arm.pat);
                    if let Some((_, guard)) = &mut arm.guard {
                        self.rename_expr_here(guard);
                    }
                    self.rename_expr_here(&mut arm.body);
                    self.pop();
                }
            }
            Expr::Closure(c) => {
                self.push();
                for input in &mut c.inputs {
                    self.rename_pat_binding(input);
                }
                self.rename_expr_here(&mut c.body);
                self.pop();
            }
            Expr::Loop(l) => {
                self.push();
                self.rename_block(&mut l.body);
                self.pop();
            }
            Expr::Async(a) => {
                self.push();
                self.rename_block(&mut a.block);
                self.pop();
            }
            Expr::Unsafe(u) => self.rename_block(&mut u.block),
            Expr::TryBlock(t) => self.rename_block(&mut t.block),
            Expr::Let(l) => {
                self.rename_expr_here(&mut l.expr);
                self.rename_pat_binding(&mut l.pat);
            }
            Expr::Macro(em) => self.rewrite_macro(&mut em.mac),
            Expr::Struct(s) => {
                for field in s.fields.iter_mut() {
                    let member_name = named_member(&field.member);
                    self.rename_expr_here(&mut field.expr);
                    // Same story as Pat::Struct: expand shorthand if the
                    // value's ident diverged from the member name.
                    if field.colon_token.is_none() {
                        let expr_name = expr_ident_name(&field.expr);
                        if member_name != expr_name {
                            field.colon_token = Some(Default::default());
                        }
                    }
                }
                if let Some(rest) = &mut s.rest {
                    self.rename_expr_here(rest);
                }
            }
            _ => visit_mut::visit_expr_mut(&mut ExprDelegate(self), expr),
        }
    }

    fn rename_cond(&mut self, cond: &mut Expr) {
        if let Expr::Let(l) = cond {
            self.rename_expr_here(&mut l.expr);
            self.rename_pat_binding(&mut l.pat);
        } else {
            self.rename_expr_here(cond);
        }
    }

    fn rename_block(&mut self, block: &mut Block) {
        self.push();
        for stmt in &mut block.stmts {
            match stmt {
                Stmt::Local(local) => {
                    if let Some(init) = &mut local.init {
                        self.rename_expr_here(&mut init.expr);
                        if let Some((_, e)) = &mut init.diverge {
                            self.rename_expr_here(e);
                        }
                    }
                    self.rename_pat_binding(&mut local.pat);
                }
                Stmt::Expr(e, _) => self.rename_expr_here(e),
                Stmt::Item(item) => {
                    self.visit_item_mut(item);
                }
                Stmt::Macro(m) => self.rewrite_macro(&mut m.mac),
            }
        }
        self.pop();
    }

    /// Snapshot per-fn state, prime `fn_forbidden` from the fn's current
    /// tokens, rename bindings + body, then restore state so nested fns
    /// (or the next sibling fn) start fresh.
    fn process_fn(&mut self, sig: &mut Signature, block: &mut Block) {
        let saved_forbidden = std::mem::take(&mut self.fn_forbidden);
        let saved_used_new = std::mem::take(&mut self.fn_used_new);
        let saved_counter = self.fn_local_counter;

        let mut ts = TokenStream::new();
        sig.to_tokens(&mut ts);
        block.to_tokens(&mut ts);
        let mut forbidden = HashSet::new();
        collect_ident_words(ts, &mut forbidden);
        self.fn_forbidden = forbidden;
        self.fn_used_new = HashSet::new();
        self.fn_local_counter = 0;

        self.push();
        for arg in &mut sig.inputs {
            if let FnArg::Typed(pt) = arg {
                self.rename_pat_binding(&mut pt.pat);
            }
        }
        self.rename_block(block);
        self.pop();

        self.fn_forbidden = saved_forbidden;
        self.fn_used_new = saved_used_new;
        self.fn_local_counter = saved_counter;
    }

    /// Rewrite ident tokens + `{ident}` captures inside a macro
    /// invocation according to the current lexical scope. Skips idents
    /// after `.` (field access) or `::` (path segment) since neither
    /// can name a local var.
    fn rewrite_macro(&self, mac: &mut syn::Macro) {
        let resolve = |name: &str| self.resolve(name);
        mac.tokens = rewrite_macro_tokens(std::mem::take(&mut mac.tokens), &resolve, true, true);
    }
}

impl VisitMut for LocalRenamer {
    fn visit_item_fn_mut(&mut self, node: &mut ItemFn) {
        self.process_fn(&mut node.sig, &mut node.block);
    }
    fn visit_impl_item_fn_mut(&mut self, node: &mut ImplItemFn) {
        self.process_fn(&mut node.sig, &mut node.block);
    }
    fn visit_trait_item_fn_mut(&mut self, node: &mut TraitItemFn) {
        if let Some(block) = &mut node.default {
            self.process_fn(&mut node.sig, block);
        }
    }
}

// Delegate wrapper so we can call `visit_expr_mut` on subexpressions that
// don't need scope tracking without infinite-recursing on the outer node.
struct ExprDelegate<'a>(&'a mut LocalRenamer);
impl VisitMut for ExprDelegate<'_> {
    fn visit_expr_mut(&mut self, e: &mut Expr) {
        self.0.rename_expr_here(e);
    }
    fn visit_item_mut(&mut self, i: &mut Item) {
        self.0.visit_item_mut(i);
    }
}

// ---------------------------------------------------------------------------
// Short-name generator
// ---------------------------------------------------------------------------

const FIRST_LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const FIRST_UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const REST_ALPHABET: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_";

/// Generate the `n`-th (1-indexed) short name using `first_alphabet` for
/// the first character and `REST_ALPHABET` for every character after that.
///
/// Ordering: all 1-char names in `first_alphabet` order, then all 2-char
/// names in (first, rest) major-order, then 3-char, etc.
fn short_name(idx: u64, first_alphabet: &[u8], rest_alphabet: &[u8]) -> String {
    let f = first_alphabet.len() as u64;
    let r = rest_alphabet.len() as u64;
    let mut n = idx;
    let mut count = f; // number of names of the current length
    let mut len = 1u32;
    while n > count {
        n -= count;
        count = count.checked_mul(r).expect("short-name index overflowed");
        len += 1;
    }
    // n is now 1..=count for length `len`; 0-index it.
    n -= 1;
    let rest_len = len - 1;
    let mut power = 1u64;
    for _ in 0..rest_len {
        power *= r;
    }
    let first_idx = (n / power) as usize;
    let mut rest = n % power;
    let mut suffix: Vec<char> = Vec::with_capacity(rest_len as usize);
    for _ in 0..rest_len {
        suffix.push(rest_alphabet[(rest % r) as usize] as char);
        rest /= r;
    }
    let mut s = String::with_capacity(len as usize);
    s.push(first_alphabet[first_idx] as char);
    for c in suffix.into_iter().rev() {
        s.push(c);
    }
    s
}

fn starts_uppercase(id: &Ident) -> bool {
    id.to_string()
        .chars()
        .next()
        .map_or(false, |c| c.is_uppercase())
}

fn named_member(m: &syn::Member) -> Option<String> {
    match m {
        syn::Member::Named(id) => Some(id.to_string()),
        syn::Member::Unnamed(_) => None,
    }
}

fn pat_ident_name(p: &Pat) -> Option<String> {
    match p {
        Pat::Ident(pi) if pi.subpat.is_none() => Some(pi.ident.to_string()),
        _ => None,
    }
}

fn expr_ident_name(e: &Expr) -> Option<String> {
    if let Expr::Path(p) = e {
        if p.qself.is_none()
            && p.path.leading_colon.is_none()
            && p.path.segments.len() == 1
            && p.path.segments[0].arguments.is_empty()
        {
            return Some(p.path.segments[0].ident.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{rename, short_name, FIRST_LOWER, FIRST_UPPER, REST_ALPHABET};

    #[test]
    fn short_name_ordering_lower() {
        assert_eq!(short_name(1, FIRST_LOWER, REST_ALPHABET), "a");
        assert_eq!(short_name(2, FIRST_LOWER, REST_ALPHABET), "b");
        assert_eq!(short_name(26, FIRST_LOWER, REST_ALPHABET), "z");
        assert_eq!(short_name(27, FIRST_LOWER, REST_ALPHABET), "aa");
        assert_eq!(short_name(28, FIRST_LOWER, REST_ALPHABET), "ab");
        // 26 + 63 = 89 → last 2-char with first='a', last suffix char '_'
        assert_eq!(short_name(89, FIRST_LOWER, REST_ALPHABET), "a_");
        assert_eq!(short_name(90, FIRST_LOWER, REST_ALPHABET), "ba");
    }

    #[test]
    fn short_name_upper_starts_uppercase() {
        for i in 1u64..=26 {
            let n = short_name(i, FIRST_UPPER, REST_ALPHABET);
            assert!(n.chars().next().unwrap().is_ascii_uppercase(), "{}", n);
            assert_eq!(n.len(), 1);
        }
        assert_eq!(short_name(27, FIRST_UPPER, REST_ALPHABET), "Aa");
    }

    #[test]
    fn renames_private_fn_and_call_site() {
        let out = rename(
            r#"
            pub mod algo_lib {
                fn helper(v: i32) -> i32 { v + 1 }
                pub fn entry() -> i32 { helper(2) }
            }
        "#,
        )
        .unwrap();
        assert!(out.contains("fn A"), "output:\n{}", out);
        assert!(out.contains("A(2)"), "output:\n{}", out);
        assert!(!out.contains("helper"), "output:\n{}", out);
    }

    #[test]
    fn renames_params_of_public_fn() {
        let out = rename(
            r#"
            pub mod algo_lib {
                pub fn public_api(x: i32) -> i32 { x + 1 }
            }
        "#,
        )
        .unwrap();
        assert!(out.contains("fn public_api"), "output:\n{}", out);
        // param renamed
        assert!(!out.contains("(x: i32)"), "output:\n{}", out);
        // and its use in body renamed
        assert!(!out.contains(" x + "), "output:\n{}", out);
    }

    #[test]
    fn rewrites_fn_ref_inside_macro() {
        // Aggressive rewrite: fn refs inside macro tokens follow the
        // rename. `stringify!(helper)` becomes `stringify!(A)` — the
        // printed name changes (accepted trade-off).
        let out = rename(
            r#"
            pub mod algo_lib {
                fn helper() {}
                pub fn entry() { println!("{}", stringify!(helper)); helper(); }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("helper"), "output:\n{}", out);
        assert!(out.contains("fn A"), "output:\n{}", out);
    }

    #[test]
    fn rewrites_local_ref_inside_format_capture() {
        // `println!("{count}")` — `count` is an implicit format capture
        // inside the string literal. We parse the capture and rewrite
        // to the new binding name.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub fn entry(count: usize) { println!("{count}"); }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("count"), "output:\n{}", out);
        // The renamed param name must appear in the format-string capture.
        assert!(out.contains("(a: usize)"), "output:\n{}", out);
        assert!(out.contains("\"{a}\""), "output:\n{}", out);
    }

    #[test]
    fn scope_aware_shadowing() {
        let out = rename(
            r#"
            pub mod algo_lib {
                pub fn entry(v: i32) -> i32 {
                    let w = v;
                    let z = {
                        let w = 7;
                        w
                    };
                    w + z
                }
            }
        "#,
        )
        .unwrap();
        assert!(out.contains("+"), "output:\n{}", out);
        // must still parse
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn avoids_existing_local_name() {
        // Fn already uses `a` and `b`; renamer must not clobber them by
        // reassigning `count` → `a` (or `b`) — that would parse but
        // silently redirect arithmetic through the wrong binding.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub fn entry(count: usize) -> usize {
                    let a = 1;
                    let b = 2;
                    a + b + count
                }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("entry(a:"), "output:\n{}", out);
        assert!(!out.contains("entry(b:"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn does_not_rename_struct() {
        let out = rename(
            r#"
            pub mod algo_lib {
                pub struct MyStruct { pub x: i32 }
                fn helper(s: MyStruct) -> i32 { s.x }
            }
        "#,
        )
        .unwrap();
        assert!(out.contains("MyStruct"), "output:\n{}", out);
    }

    #[test]
    fn parses_after_rename() {
        let out = rename(
            r#"
            pub mod algo_lib {
                pub fn entry(mut v: Vec<i32>) -> i32 {
                    let mut acc = 0;
                    for x in &v {
                        acc += *x;
                    }
                    if let Some(first) = v.first() {
                        acc += *first;
                    }
                    match acc {
                        0 => 0,
                        n => n,
                    }
                }
            }
        "#,
        )
        .unwrap();
        syn::parse_file(&out).expect("output must parse");
    }

    #[test]
    fn renames_through_struct_shorthand() {
        // Regression: `Self { data: vec![0; data_len], len }` shorthand
        // must expand when the value expr gets renamed.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub struct Foo { pub data: Vec<i32>, pub len: usize }
                impl Foo {
                    pub fn new(len: usize) -> Self {
                        let data_len = if len == 0 { 0 } else { Self::index(len - 1) + 1 };
                        Self { data: vec![0; data_len], len }
                    }
                    fn index(x: usize) -> usize { x + 1 }
                }
            }
        "#,
        )
        .unwrap();
        // Param `len` should have been renamed away.
        assert!(!out.contains("(len: usize)"), "output:\n{}", out);
        // And the shorthand must have been expanded, not silently kept.
        // We look for the pattern `len:` at the value position to confirm
        // the colon appeared.
        assert!(out.contains("len:"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn per_fn_scope_is_independent() {
        // `len` in `debug` is macro-referenced, but with aggressive
        // rewriting it still gets renamed and the `{len}` capture is
        // rewritten too. Different fns get independent short-name pools.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub fn debug(len: usize) { println!("{len}"); }
                pub fn use_len(len: usize) -> usize { len + 1 }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("(len:"), "output:\n{}", out);
        assert!(!out.contains("{len}"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn rewrites_ident_token_inside_macro() {
        // Regression for the user's original example: `vec![0; data_len]`
        // and `Self { ..., len }` both need renaming.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub struct Foo { pub data: Vec<i32>, pub len: usize }
                impl Foo {
                    pub fn new(len: usize) -> Self {
                        let data_len = if len == 0 { 0 } else { Self::index(len - 1) + 1 };
                        Self { data: vec![0; data_len], len }
                    }
                    fn index(x: usize) -> usize { x + 1 }
                }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("(len: usize)"), "output:\n{}", out);
        assert!(!out.contains("data_len"), "output:\n{}", out);
        // vec![0; data_len] token must be rewritten to the new local name.
        assert!(out.contains("vec![0"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn skips_field_and_path_tail_inside_macro() {
        // `foo.x` (field) and `mod::x` (path segment): the trailing `x`
        // must NOT be rewritten as a local ref. Locals only, path is
        // safe for fn renaming which has its own visitor.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub struct S { pub x: i32 }
                pub fn entry(x: i32, s: S) {
                    println!("{} {} {}", x, s.x, S { x: 1 }.x);
                }
            }
        "#,
        )
        .unwrap();
        // Struct field `.x` must survive; local `x` must be renamed.
        assert!(out.contains(".x"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn renames_private_struct() {
        let out = rename(
            r#"
            pub mod algo_lib {
                struct Helper { pub x: i32 }
                pub fn make() -> Helper { Helper { x: 1 } }
                pub fn read(h: Helper) -> i32 { h.x }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("Helper"), "output:\n{}", out);
        // struct def, both use sites, and struct literal all consistent
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn renames_private_enum_but_keeps_variants() {
        let out = rename(
            r#"
            pub mod algo_lib {
                enum Op { Add, Sub }
                pub fn eval(o: Op, a: i32, b: i32) -> i32 {
                    match o { Op::Add => a + b, Op::Sub => a - b }
                }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("Op "), "output:\n{}", out);
        assert!(!out.contains("Op::"), "output:\n{}", out);
        // Variants stay named (they'd need per-enum rewriting we don't do)
        assert!(out.contains("::Add"), "output:\n{}", out);
        assert!(out.contains("::Sub"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn renames_private_type_alias_and_const() {
        let out = rename(
            r#"
            pub mod algo_lib {
                type Idx = usize;
                const CAP: usize = 100;
                pub fn build() -> Vec<Idx> { vec![0; CAP] }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("Idx"), "output:\n{}", out);
        assert!(!out.contains("CAP"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn rewrites_type_through_associated_call() {
        // `Helper::new()` — `Helper` is a middle path segment, not last.
        let out = rename(
            r#"
            pub mod algo_lib {
                struct Helper { x: i32 }
                impl Helper { pub fn new() -> Helper { Helper { x: 1 } } }
                pub fn build() -> Helper { Helper::new() }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("Helper"), "output:\n{}", out);
        // Method call itself preserved (last segment)
        assert!(out.contains("::new"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn renames_private_inherent_method() {
        let out = rename(
            r#"
            pub mod algo_lib {
                pub struct BitSet { data: Vec<u64> }
                impl BitSet {
                    pub fn new() -> Self { let x = Self::index(0); Self { data: vec![0; x] } }
                    fn index(a: usize) -> usize { a >> 6 }
                    fn fix_last(&mut self) { self.data.fill(0); }
                }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("index"), "output:\n{}", out);
        assert!(!out.contains("fix_last"), "output:\n{}", out);
        // Public method `new` and the struct field `data` survive.
        assert!(out.contains("fn new"), "output:\n{}", out);
        assert!(out.contains("data"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn method_name_shadowed_by_pattern_binding() {
        // Regression: `fn yes(&self)` has a body that matches
        // `Custom(yes, _) => yes` — `yes` in the arm body is a
        // reference to the LOCAL pattern binding, not the method.
        // The local pass must rename the binding before the item
        // pass rewrites method names, otherwise `=> yes` would be
        // turned into a call to the renamed method.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub enum Kind { Custom(&'static str, &'static str) }
                impl Kind {
                    fn yes(&self) -> &'static str {
                        match self { Kind::Custom(yes, _) => yes }
                    }
                    fn no(&self) -> &'static str {
                        match self { Kind::Custom(_, no) => no }
                    }
                }
            }
        "#,
        )
        .unwrap();
        // Output must compile: `yes` in `=> yes` must resolve to the
        // pattern binding, not to a method call.
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn leaves_trait_impl_methods_alone() {
        // Even though `foo` is a name we'd otherwise pick to rename,
        // trait impls must keep the exact name from the trait def.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub trait T { fn foo(&self); }
                pub struct S;
                impl T for S { fn foo(&self) {} }
            }
        "#,
        )
        .unwrap();
        assert!(out.contains("fn foo"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn skips_method_name_collision_with_field() {
        // `field_name` is both a struct field AND a private method name.
        // Method rename must not fire — otherwise field access breaks.
        let out = rename(
            r#"
            pub mod algo_lib {
                pub struct A { pub field_name: i32 }
                pub struct B;
                impl B {
                    fn field_name(&self) -> i32 { 0 }
                }
                pub fn touch(a: A, b: B) -> i32 { a.field_name + b.field_name() }
            }
        "#,
        )
        .unwrap();
        // Method not renamed → field access still works.
        assert!(out.contains("field_name"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn does_not_rename_public_type() {
        let out = rename(
            r#"
            pub mod algo_lib {
                pub struct MyStruct { pub x: i32 }
                pub const CAP: usize = 100;
            }
        "#,
        )
        .unwrap();
        assert!(out.contains("MyStruct"), "output:\n{}", out);
        assert!(out.contains("CAP"), "output:\n{}", out);
    }

    #[test]
    fn generic_param_shadows_private_item_name() {
        // Regression: a private `const F` renamed to `Ab` must NOT
        // rewrite the generic `<F: FnMut(...)>` inside the treap fn.
        // The generic-param `F` is scoped to the fn, shadowing the
        // outer const.
        let out = rename(
            r#"
            pub mod algo_lib {
                const F: u64 = 6364136223846793005;
                pub fn build<F: FnMut(usize) -> i32>(mut f: F, from: usize, to: usize) -> (i32, F) {
                    let mid = (from + to) / 2;
                    let x = f(mid);
                    (x, f)
                }
                pub fn seed() -> u64 { F }
            }
        "#,
        )
        .unwrap();
        // The const gets renamed, but the generic `F` inside `build`
        // stays intact.
        assert!(out.contains("F: FnMut"), "output:\n{}", out);
        assert!(out.contains(": F,"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn generic_masking_extends_to_macros() {
        // `dbg!(F)` inside a fn with generic `<F>` must NOT rewrite
        // `F` even though a private const `F` is renamed elsewhere.
        let out = rename(
            r#"
            pub mod algo_lib {
                const F: u64 = 42;
                pub fn wrap<F: Clone>(x: F) -> F {
                    let y = x.clone();
                    dbg!(std::any::type_name::<F>());
                    y
                }
                pub fn seed() -> u64 { F }
            }
        "#,
        )
        .unwrap();
        // Macro token `F` inside the generic scope stays put — the
        // rewrite would have turned it into the const's short name.
        let compact: String = out.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains("::<F>()"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn skips_type_named_same_as_used_std() {
        // `HashMap` is our private struct AND imported from std elsewhere
        // in the file → both bound to the same name, can't safely rewrite.
        let out = rename(
            r#"
            pub mod algo_lib {
                use std::collections::HashMap;
                fn other() -> HashMap<i32, i32> { HashMap::new() }
                struct HashMap { x: i32 }
            }
        "#,
        )
        .unwrap();
        // We conservatively skip — HashMap survives.
        assert!(out.contains("HashMap"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn long_local_names_shorten() {
        let out = rename(
            r#"
            pub mod algo_lib {
                pub fn entry(count: usize) -> usize {
                    let mut result = 0;
                    for i in 0..count {
                        result += i;
                    }
                    result
                }
            }
        "#,
        )
        .unwrap();
        assert!(!out.contains("count"), "output:\n{}", out);
        assert!(!out.contains("result"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }
}
