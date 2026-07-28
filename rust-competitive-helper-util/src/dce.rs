//! Dead-code elimination for the aggregated library.
//!
//! Approach: mark-and-sweep at item granularity.
//!
//! - **Seed** the reachable set with every item name (fn / struct / enum /
//!   union / trait / trait-alias / type-alias / const / static / macro-def)
//!   that appears as an ident anywhere in the solution portion of the
//!   assembled file (the `entry_idents` argument — a superset of what the
//!   user's `main` and their `pub mod solution` reference).
//! - **Mark** by iterating to fixed point: for each currently-reachable
//!   item, scan its full token stream and add any ident that names an
//!   item in the library.
//! - **Impls**: `impl X { .. }` is reachable iff `X` is reachable.
//!   `impl Trait for X { .. }` is reachable iff `X` or `Trait` is.
//!   When an impl is reachable, its body contributes idents to the
//!   mark set on the next round.
//! - **Modules** are always retained; after the sweep they may become
//!   empty and get dropped in a follow-up pass.
//! - **Use statements** inside pruned scope: any `use crate::…::X` whose
//!   `X` is a known library item that got pruned is removed. `use`s that
//!   bring in external names (std, third-party) are left alone.
//!
//! - **Per-method pruning**: after item sweep, we do a second pass over
//!   the surviving code that collects every `.foo` / `::foo` ident (i.e.
//!   method calls, field access, and associated-item paths) and drops
//!   any inherent-impl fn / const / type whose name isn't referenced.
//!   Trait impls are left alone — their items are part of the trait
//!   contract.
//!
//! Known trade-offs (conservative / never miscompiles):
//!   - Method calls `x.foo()` aren't type-resolved, so all trait impls
//!     of `X` and every method with a used name ride along even if the
//!     specific call resolves to a different type's method.
//!   - Blanket impls `impl<T: Bound> Trait for T` survive whenever
//!     `Trait` is reachable, even if no concrete `T` is.
//!   - `#[derive(...)]` is opaque; deriving on a kept struct works
//!     because the struct itself stays.

use proc_macro2::{Spacing, TokenStream, TokenTree};
use std::collections::HashSet;
use syn::__private::ToTokens;
use syn::{File, ImplItem, Item, ItemImpl, Type, UseTree};

fn walk_all_idents(stream: TokenStream, out: &mut HashSet<String>) {
    for tt in stream {
        match tt {
            TokenTree::Ident(id) => {
                out.insert(id.to_string());
            }
            TokenTree::Group(g) => walk_all_idents(g.stream(), out),
            _ => {}
        }
    }
}

pub fn eliminate(library_code: &str, entry_code: &str) -> Result<String, syn::Error> {
    let (entry_idents, entry_method_refs) = collect_entry_refs(entry_code);
    let mut file: File = syn::parse_str(library_code)?;

    let mut item_names: HashSet<String> = HashSet::new();
    collect_item_names(&file.items, &mut item_names);

    // One unified set of "names in scope". Contains every ident we've
    // seen — library items, entry-set names, transitively marked idents,
    // and phantom-type names introduced by macro-generated `impl X { .. }`
    // blocks. Used both to decide which library items to keep (item name
    // must be in `seen`) and to gate impl blocks (self type OR trait
    // name must be in `seen`).
    //
    // Seed:
    //   1. every ident referenced by the entry (solution + main).
    //   2. every trait defined in the library — extension traits are
    //      called via `.method()` without ever naming the trait, so
    //      without type inference we can't tell which trait a method
    //      call resolves to. Keeping trait defs (and, via
    //      `impl_is_reachable`, their impls) is safe conservative.
    let mut seen: HashSet<String> = entry_idents.clone();
    collect_trait_names(&file.items, &mut seen);

    // Fixed-point mark. On each round scan every reachable item (regular
    // items whose name is in `seen`, and impls whose Self/Trait is in
    // `seen`) and add every ident from its tokens to `seen`. Stop when
    // a round adds nothing new.
    loop {
        let before = seen.len();
        let mut new_names: HashSet<String> = HashSet::new();
        mark_pass(&file.items, &seen, &mut new_names);
        seen.extend(new_names);
        if seen.len() == before {
            break;
        }
    }

    sweep_items(&mut file.items, &seen, &item_names);

    // Second sweep — per-method inside inherent impls. Iterate to
    // fixed point: dropping method `is_subset` may make `is_superset`
    // (only called from `is_subset`) unreferenced too. Recompute
    // `method_refs` from the surviving code each round.
    loop {
        let mut method_refs: HashSet<String> = entry_method_refs.clone();
        collect_dot_and_path_refs(file.to_token_stream(), &mut method_refs);
        let before = count_impl_items(&file.items);
        prune_impl_items(&mut file.items, &method_refs);
        let after = count_impl_items(&file.items);
        if after == before {
            break;
        }
    }
    drop_empty_impls(&mut file.items);

    Ok(prettyplease::unparse(&file))
}

fn count_impl_items(items: &[Item]) -> usize {
    let mut n = 0;
    for item in items {
        match item {
            Item::Mod(m) => {
                if let Some((_, sub)) = &m.content {
                    n += count_impl_items(sub);
                }
            }
            Item::Impl(imp) if imp.trait_.is_none() => n += imp.items.len(),
            _ => {}
        }
    }
    n
}

/// Extract entry-code metadata: the flat ident set (used to seed
/// reachability) and the set of names that appear as method / assoc
/// calls (`.foo`, `::foo`) — used to keep matching impl items alive.
fn collect_entry_refs(entry_code: &str) -> (HashSet<String>, HashSet<String>) {
    let mut idents = HashSet::new();
    let mut method_refs = HashSet::new();
    if let Ok(f) = syn::parse_file(entry_code) {
        let ts = f.to_token_stream();
        walk_all_idents(ts.clone(), &mut idents);
        collect_dot_and_path_refs(ts, &mut method_refs);
    }
    (idents, method_refs)
}

/// Walk a token stream and collect every ident that appears immediately
/// after `.` (field/method access) or `::` (path segment). These are
/// the names an impl item might have been referenced through.
fn collect_dot_and_path_refs(stream: TokenStream, out: &mut HashSet<String>) {
    let tts: Vec<TokenTree> = stream.into_iter().collect();
    for (i, tt) in tts.iter().enumerate() {
        if let TokenTree::Ident(id) = tt {
            if is_after_dot_or_colon(&tts, i) {
                out.insert(id.to_string());
            }
        }
        if let TokenTree::Group(g) = tt {
            collect_dot_and_path_refs(g.stream(), out);
        }
    }
}

fn is_after_dot_or_colon(tts: &[TokenTree], i: usize) -> bool {
    if i == 0 {
        return false;
    }
    if let TokenTree::Punct(p) = &tts[i - 1] {
        if p.as_char() == '.' {
            return true;
        }
        if p.as_char() == ':' {
            // `::` = two `:` puncts with the first Joint.
            if i >= 2 {
                if let TokenTree::Punct(p2) = &tts[i - 2] {
                    if p2.as_char() == ':' && p2.spacing() == Spacing::Joint {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Drop items inside inherent (non-trait) impls whose name isn't in
/// `used`. Trait-impl items are left alone (part of the trait contract).
fn prune_impl_items(items: &mut Vec<Item>, used: &HashSet<String>) {
    for item in items.iter_mut() {
        match item {
            Item::Mod(m) => {
                if let Some((_, sub)) = &mut m.content {
                    prune_impl_items(sub, used);
                }
            }
            Item::Impl(imp) if imp.trait_.is_none() => {
                imp.items.retain(|impl_item| match impl_item {
                    ImplItem::Fn(f) => used.contains(&f.sig.ident.to_string()),
                    ImplItem::Const(c) => used.contains(&c.ident.to_string()),
                    ImplItem::Type(t) => used.contains(&t.ident.to_string()),
                    // Macros inside impls / verbatim items are opaque —
                    // keep them.
                    _ => true,
                });
            }
            _ => {}
        }
    }
}

fn drop_empty_impls(items: &mut Vec<Item>) {
    for item in items.iter_mut() {
        if let Item::Mod(m) = item {
            if let Some((_, sub)) = &mut m.content {
                drop_empty_impls(sub);
            }
        }
    }
    items.retain(|item| match item {
        Item::Impl(imp) if imp.trait_.is_none() => !imp.items.is_empty(),
        Item::Mod(m) => match &m.content {
            Some((_, sub)) => !sub.is_empty(),
            None => true,
        },
        _ => true,
    });
}

// ---------------------------------------------------------------------------
// Collection
// ---------------------------------------------------------------------------

fn collect_item_names(items: &[Item], out: &mut HashSet<String>) {
    for item in items {
        match item {
            Item::Fn(f) => {
                out.insert(f.sig.ident.to_string());
            }
            Item::Struct(s) => {
                out.insert(s.ident.to_string());
            }
            Item::Enum(e) => {
                out.insert(e.ident.to_string());
            }
            Item::Union(u) => {
                out.insert(u.ident.to_string());
            }
            Item::Trait(t) => {
                out.insert(t.ident.to_string());
            }
            Item::TraitAlias(t) => {
                out.insert(t.ident.to_string());
            }
            Item::Type(t) => {
                out.insert(t.ident.to_string());
            }
            Item::Const(c) => {
                out.insert(c.ident.to_string());
            }
            Item::Static(s) => {
                out.insert(s.ident.to_string());
            }
            Item::Macro(m) => {
                if let Some(id) = &m.ident {
                    out.insert(id.to_string());
                }
            }
            Item::Mod(m) => {
                if let Some((_, sub)) = &m.content {
                    collect_item_names(sub, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_trait_names(items: &[Item], out: &mut HashSet<String>) {
    for item in items {
        match item {
            Item::Trait(t) => {
                out.insert(t.ident.to_string());
            }
            Item::TraitAlias(t) => {
                out.insert(t.ident.to_string());
            }
            Item::Mod(m) => {
                if let Some((_, sub)) = &m.content {
                    collect_trait_names(sub, out);
                }
            }
            _ => {}
        }
    }
}

fn item_own_name(item: &Item) -> Option<String> {
    match item {
        Item::Fn(f) => Some(f.sig.ident.to_string()),
        Item::Struct(s) => Some(s.ident.to_string()),
        Item::Enum(e) => Some(e.ident.to_string()),
        Item::Union(u) => Some(u.ident.to_string()),
        Item::Trait(t) => Some(t.ident.to_string()),
        Item::TraitAlias(t) => Some(t.ident.to_string()),
        Item::Type(t) => Some(t.ident.to_string()),
        Item::Const(c) => Some(c.ident.to_string()),
        Item::Static(s) => Some(s.ident.to_string()),
        Item::Macro(m) => m.ident.as_ref().map(|id| id.to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Mark pass
// ---------------------------------------------------------------------------

fn mark_pass(items: &[Item], seen: &HashSet<String>, new_names: &mut HashSet<String>) {
    for item in items {
        // Don't scan modules as a unit — that would pull in idents from
        // their `use` statements and from unreachable sibling items in
        // the same module. Just recurse.
        if let Item::Mod(m) = item {
            if let Some((_, sub)) = &m.content {
                mark_pass(sub, seen, new_names);
            }
            continue;
        }
        let should_scan = match item {
            Item::Impl(imp) => impl_is_reachable(imp, seen),
            // Module-level macro *invocations* (no ident) expand to
            // items at compile time — we can't know what they emit, so
            // we always scan them (marks the invoked macro as reachable)
            // and always keep them (see sweep).
            Item::Macro(m) if m.ident.is_none() => true,
            other => item_own_name(other)
                .map(|n| seen.contains(&n))
                .unwrap_or(false),
        };
        if should_scan {
            collect_all_idents_from_tokens(item.to_token_stream(), seen, new_names);
        }
    }
}

fn collect_all_idents_from_tokens(
    stream: TokenStream,
    seen: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    for tt in stream {
        match tt {
            proc_macro2::TokenTree::Ident(id) => {
                let s = id.to_string();
                if !seen.contains(&s) {
                    out.insert(s);
                }
            }
            proc_macro2::TokenTree::Group(g) => {
                collect_all_idents_from_tokens(g.stream(), seen, out);
            }
            _ => {}
        }
    }
}

fn impl_is_reachable(imp: &ItemImpl, seen: &HashSet<String>) -> bool {
    let self_name = type_head_name(&imp.self_ty);
    let trait_name = imp
        .trait_
        .as_ref()
        .and_then(|(_, p, _)| p.segments.last())
        .map(|s| s.ident.to_string());
    self_name.map_or(false, |n| seen.contains(&n))
        || trait_name.map_or(false, |n| seen.contains(&n))
}

fn type_head_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => type_head_name(&r.elem),
        Type::Paren(p) => type_head_name(&p.elem),
        Type::Group(g) => type_head_name(&g.elem),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Sweep
// ---------------------------------------------------------------------------

fn sweep_items(items: &mut Vec<Item>, seen: &HashSet<String>, item_names: &HashSet<String>) {
    // Recurse into modules first so we can drop empty ones on the way up.
    for item in items.iter_mut() {
        if let Item::Mod(m) = item {
            if let Some((_, sub)) = &mut m.content {
                sweep_items(sub, seen, item_names);
            }
        }
    }
    items.retain_mut(|item| match item {
        Item::Impl(imp) => impl_is_reachable(imp, seen),
        Item::Mod(m) => match &m.content {
            Some((_, sub)) => !sub.is_empty(),
            None => true,
        },
        Item::Use(u) => prune_use_tree(&mut u.tree, seen, item_names),
        // Module-level macro invocations must survive — their expansion
        // is what creates the trait impls / items downstream code needs.
        Item::Macro(m) if m.ident.is_none() => true,
        Item::ExternCrate(_) | Item::ForeignMod(_) => true,
        other => item_own_name(other)
            .map(|n| seen.contains(&n))
            .unwrap_or(true),
    });
}

/// Recursively prune a `use` tree: drop leaves that name pruned library
/// items. Returns `true` if the tree still has something to import.
fn prune_use_tree(
    tree: &mut UseTree,
    seen: &HashSet<String>,
    item_names: &HashSet<String>,
) -> bool {
    match tree {
        UseTree::Name(n) => {
            let name = n.ident.to_string();
            // If the name is a library item and it got pruned, drop this
            // leaf. External names (not in item_names) stay.
            !(item_names.contains(&name) && !seen.contains(&name))
        }
        UseTree::Rename(r) => {
            let name = r.ident.to_string();
            !(item_names.contains(&name) && !seen.contains(&name))
        }
        UseTree::Glob(_) => true,
        UseTree::Path(p) => prune_use_tree(&mut p.tree, seen, item_names),
        UseTree::Group(g) => {
            let taken = std::mem::take(&mut g.items);
            let mut kept = Vec::new();
            for mut item in taken {
                if prune_use_tree(&mut item, seen, item_names) {
                    kept.push(item);
                }
            }
            g.items = kept.into_iter().collect();
            !g.items.is_empty()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::eliminate;

    #[test]
    fn drops_unreachable_fn() {
        let out = eliminate(
            r#"
            pub fn used() {}
            pub fn unused() {}
        "#,
            "fn main() { used(); }",
        )
        .unwrap();
        assert!(out.contains("fn used"), "output:\n{}", out);
        assert!(!out.contains("fn unused"), "output:\n{}", out);
    }

    #[test]
    fn keeps_transitively_reachable() {
        let out = eliminate(
            r#"
            pub fn used() { helper() }
            pub fn helper() { deep() }
            pub fn deep() {}
            pub fn unused() {}
        "#,
            "fn main() { used(); }",
        )
        .unwrap();
        assert!(out.contains("fn used"), "output:\n{}", out);
        assert!(out.contains("fn helper"), "output:\n{}", out);
        assert!(out.contains("fn deep"), "output:\n{}", out);
        assert!(!out.contains("fn unused"), "output:\n{}", out);
    }

    #[test]
    fn keeps_impls_of_reachable_type() {
        let out = eliminate(
            r#"
            pub struct Foo;
            impl Foo { pub fn method(&self) {} pub fn other(&self) {} }
            pub struct Bar;
            impl Bar { pub fn method(&self) {} }
        "#,
            "fn main() { let f = Foo; f.method(); f.other(); }",
        )
        .unwrap();
        assert!(out.contains("struct Foo"), "output:\n{}", out);
        assert!(out.contains("impl Foo"), "output:\n{}", out);
        assert!(!out.contains("struct Bar"), "output:\n{}", out);
        assert!(!out.contains("impl Bar"), "output:\n{}", out);
    }

    #[test]
    fn keeps_trait_impls_of_reachable_type() {
        let out = eliminate(
            r#"
            pub struct Foo;
            pub trait Show { fn show(&self); }
            impl Show for Foo { fn show(&self) {} }
            pub struct Bar;
            impl Show for Bar { fn show(&self) {} }
        "#,
            "fn main() { let f = Foo; f.show(); }",
        )
        .unwrap();
        // Foo's Show impl stays because Foo is reachable → marks Show →
        // marks Bar's Show impl in the next round → marks Bar.
        assert!(out.contains("struct Foo"), "output:\n{}", out);
        assert!(out.contains("trait Show"), "output:\n{}", out);
        assert!(out.contains("struct Bar"), "output:\n{}", out);
    }

    #[test]
    fn drops_empty_module() {
        let out = eliminate(
            r#"
            pub mod used_mod {
                pub fn hi() {}
            }
            pub mod dead_mod {
                pub fn gone() {}
                pub fn also_gone() {}
            }
        "#,
            "fn main() { hi(); }",
        )
        .unwrap();
        assert!(out.contains("mod used_mod"), "output:\n{}", out);
        assert!(!out.contains("dead_mod"), "output:\n{}", out);
    }

    #[test]
    fn prunes_use_of_dropped_item() {
        let out = eliminate(
            r#"
            pub mod outer {
                use crate::algo::helper;
                use std::collections::HashMap;
                pub fn used() -> HashMap<i32, i32> {
                    helper();
                    HashMap::new()
                }
            }
            pub mod algo {
                pub fn helper() {}
            }
        "#,
            "fn main() { used(); }",
        )
        .unwrap();
        // std::HashMap use stays; crate::algo::helper use is used → stays.
        assert!(out.contains("HashMap"), "output:\n{}", out);
        assert!(out.contains("helper"), "output:\n{}", out);
    }

    #[test]
    fn keeps_impls_of_phantom_type_from_macro() {
        let out = eliminate(
            r#"
            transparent_wrapper!(Str = Vec<u8>);
            impl Str { pub fn new() -> Self { Self(Vec::new()) } }
        "#,
            "fn main() { let s = Str::new(); }",
        )
        .unwrap();
        assert!(out.contains("impl Str"), "output:\n{}", out);
        assert!(out.contains("fn new"), "output:\n{}", out);
        assert!(out.contains("transparent_wrapper"), "output:\n{}", out);
    }

    #[test]
    fn prunes_use_when_target_gone() {
        let out = eliminate(
            r#"
            pub mod outer {
                use crate::algo::gone;
                pub fn used() {}
            }
            pub mod algo {
                pub fn gone() {}
                pub fn also_used() {}
            }
        "#,
            "fn main() { used(); also_used(); }",
        )
        .unwrap();
        assert!(!out.contains("gone"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn drops_uncalled_inherent_method() {
        // `BitSet::flip` is defined but never referenced anywhere in
        // the reachable code — per-method sweep must drop it.
        let out = eliminate(
            r#"
            pub struct BitSet;
            impl BitSet {
                pub fn new() -> Self { Self }
                pub fn set(&mut self) {}
                pub fn flip(&mut self) {}
            }
        "#,
            "fn main() { let mut b = BitSet::new(); b.set(); }",
        )
        .unwrap();
        assert!(out.contains("fn new"), "output:\n{}", out);
        assert!(out.contains("fn set"), "output:\n{}", out);
        assert!(!out.contains("fn flip"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn drops_method_only_called_from_dropped_method() {
        // `is_subset` calls `is_superset`; nothing calls `is_subset`.
        // First round removes `is_subset`. Second round removes
        // `is_superset` because its only referrer is gone. Needs
        // iteration to fixed point.
        let out = eliminate(
            r#"
            pub struct BitSet;
            impl BitSet {
                pub fn new() -> Self { Self }
                pub fn is_superset(&self, _o: &Self) -> bool { true }
                pub fn is_subset(&self, o: &Self) -> bool { o.is_superset(self) }
            }
        "#,
            "fn main() { let _b = BitSet::new(); }",
        )
        .unwrap();
        assert!(out.contains("fn new"), "output:\n{}", out);
        assert!(!out.contains("is_subset"), "output:\n{}", out);
        assert!(!out.contains("is_superset"), "output:\n{}", out);
        syn::parse_file(&out).unwrap();
    }

    #[test]
    fn keeps_trait_impl_methods_untouched() {
        // Trait impl items are part of the trait contract — even if
        // the specific method isn't called through this type in
        // reachable code, dropping it would break the impl.
        let out = eliminate(
            r#"
            pub struct Foo;
            pub trait T { fn a(&self); fn b(&self); }
            impl T for Foo { fn a(&self) {} fn b(&self) {} }
        "#,
            "fn main() { let f = Foo; f.a(); }",
        )
        .unwrap();
        // Both trait methods survive on the impl.
        assert!(out.contains("fn a"), "output:\n{}", out);
        assert!(out.contains("fn b"), "output:\n{}", out);
    }
}
