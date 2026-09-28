use crate::file_explorer::{FileExplorer, RealFileExplorer};
use prettyplease::unparse;
use proc_macro2::LineColumn;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use syn::__private::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{visit_item_macro, Visit};
use syn::visit_mut::{visit_item_macro_mut, visit_item_mut, visit_path_mut, VisitMut};
use syn::{Ident, Item, ItemMacro, ItemUse, UsePath, UseTree};

// ---------------------------------------------------------------------------
// Readable-emission plumbing: per-file source text + span-based edit list.
// When we don't need to minify (solution always; library when minimize=false)
// we hand the aggregator the ORIGINAL source with just our AST-driven edits
// applied by byte range, so // comments and macro-invocation whitespace both
// survive intact. When we do minify (library, minimize=true) we still go
// through prettyplease + rustminify on the mutated AST as before.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Edit {
    /// Byte range in the file's source that this edit replaces.
    start: usize,
    end: usize,
    /// Replacement text (empty means delete the range).
    text: String,
}

/// Maps `proc_macro2::LineColumn` (1-indexed line, 0-indexed column) into a
/// byte offset within the original source. Assumes ASCII — competitive
/// programming Rust code is overwhelmingly ASCII; if a source ever needs
/// multi-byte support we can widen this.
struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Self { line_starts: starts }
    }

    fn byte_offset(&self, lc: LineColumn) -> usize {
        // Spans that fall past EOF (rare, but possible for synthesized
        // tokens) clamp to the end of the file so we don't panic.
        let line_idx = lc.line.saturating_sub(1).min(self.line_starts.len() - 1);
        self.line_starts[line_idx] + lc.column
    }
}

fn apply_edits(source: &str, edits: &[Edit]) -> String {
    let mut sorted = edits.to_vec();
    // Apply from the end so earlier byte offsets stay valid.
    sorted.sort_by(|a, b| b.start.cmp(&a.start));
    let mut out = source.to_string();
    for edit in sorted {
        if edit.start > out.len() || edit.end > out.len() || edit.start > edit.end {
            eprintln!(
                "warning: skipping out-of-range edit {}..{} (source len {})",
                edit.start,
                edit.end,
                out.len()
            );
            continue;
        }
        out.replace_range(edit.start..edit.end, &edit.text);
    }
    out
}

#[derive(Clone)]
pub struct Module {
    name: String,
    children: BTreeMap<String, Module>,
    file: Option<syn::File>,
    /// Original source text for this module's file, if any. Populated
    /// during `Library::add_file`. Used by the readable emitter.
    source: Option<String>,
    /// Edits collected during the mutating visit. Applied to `source`
    /// (in reverse-position order) to produce readable output.
    edits: Vec<Edit>,
}

#[derive(Clone)]
pub struct Library {
    macros: HashMap<String, File>,
    root: Module,
}

impl Library {
    fn new(name: &str) -> Self {
        eprintln!("Library: {}", name);
        assert!(Self::is_library(name));
        let path = Self::path(name);
        let mut res = Self {
            macros: HashMap::new(),
            root: Module {
                name: name.to_string(),
                children: Default::default(),
                file: None,
                source: None,
                edits: Vec::new(),
            },
        };
        res.init_macro(path);
        res
    }

    fn path(name: &str) -> String {
        match name {
            "solution" => "src".to_string(),
            _ => format!("../../{}/src", name),
        }
    }

    fn is_library(name: &str) -> bool {
        name == "solution" || Path::new(&format!("../../{}/src", name)).exists()
    }

    fn init_macro(&mut self, path: String) {
        #[derive(Default)]
        struct MacroVisitor(Vec<String>);
        impl Visit<'_> for MacroVisitor {
            fn visit_item_macro(&mut self, i: &ItemMacro) {
                for a in &i.attrs {
                    if a.path().is_ident("macro_export") {
                        self.0.push(i.ident.as_ref().unwrap().to_string());
                    }
                }
                visit_item_macro(self, i);
            }
        }

        let files = RealFileExplorer::new().get_all_rs_files(&path);
        for file in files {
            let path = format!("{}/{}", path, file);
            let tokens = file.split('/');
            let mut fqn = vec![self.root.name.clone()];
            for token in tokens {
                if token == "mod.rs" || token == "lib.rs" {
                    continue;
                }
                fqn.push(token.strip_suffix(".rs").unwrap_or(token).to_string());
            }
            let mut visitor = MacroVisitor::default();
            eprintln!("{} {:?}", path, fqn);
            visitor.visit_file(&syn::parse_file(&std::fs::read_to_string(&path).unwrap()).unwrap());
            for macro_name in visitor.0 {
                self.macros.insert(
                    macro_name,
                    File {
                        path: path.clone(),
                        fqn: fqn.clone(),
                    },
                );
            }
        }
    }

    fn add_file(
        &mut self,
        file_meta: File,
        file: syn::File,
        source: String,
        edits: Vec<Edit>,
    ) {
        let mut cur = &mut self.root;
        for module in file_meta.fqn.into_iter().skip(1) {
            if !cur.children.contains_key(&module) {
                cur.children.insert(
                    module.clone(),
                    Module {
                        name: module.clone(),
                        children: Default::default(),
                        file: None,
                        source: None,
                        edits: Vec::new(),
                    },
                );
            }
            cur = cur.children.get_mut(&module).unwrap();
        }
        cur.file = Some(file);
        cur.source = Some(source);
        cur.edits = edits;
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct File {
    path: String,
    fqn: Vec<String>,
}

pub struct Visitor<FE: FileExplorer> {
    minimize: bool,
    queue: Vec<File>,
    files: HashSet<File>,
    content: BTreeMap<String, Library>,
    cur_library: String,
    file_explorer: FE,
    in_root: bool,
    /// Edits accumulated for the file currently being visited. Moved
    /// into the `Module` after `visit_file_mut` returns.
    cur_edits: Vec<Edit>,
    /// Line-index for the file currently being visited (owns the map
    /// from `LineColumn` back to byte offset).
    cur_line_index: Option<LineIndex>,
}

impl<FE: FileExplorer> Visitor<FE> {
    fn record_edit(&mut self, start: LineColumn, end: LineColumn, text: String) {
        let li = match self.cur_line_index.as_ref() {
            Some(li) => li,
            None => return,
        };
        let start_b = li.byte_offset(start);
        let end_b = li.byte_offset(end);
        self.cur_edits.push(Edit {
            start: start_b,
            end: end_b,
            text,
        });
    }

    fn record_delete_span(&mut self, span: proc_macro2::Span) {
        self.record_edit(span.start(), span.end(), String::new());
    }

    fn record_replace_span(&mut self, span: proc_macro2::Span, text: String) {
        self.record_edit(span.start(), span.end(), text);
    }
}

impl<FE: FileExplorer> VisitMut for Visitor<FE> {
    fn visit_item_mut(&mut self, i: &mut Item) {
        let orig_span = i.span();
        let attrs = match i {
            Item::Const(c) => &mut c.attrs,
            Item::Enum(e) => &mut e.attrs,
            Item::ExternCrate(ec) => &mut ec.attrs,
            Item::Fn(f) => &mut f.attrs,
            Item::ForeignMod(fm) => &mut fm.attrs,
            Item::Impl(i) => &mut i.attrs,
            Item::Macro(m) => &mut m.attrs,
            Item::Mod(m) => {
                if m.content.is_none() {
                    // `mod foo;` — filed content is aggregated
                    // elsewhere, so drop the bare declaration.
                    self.record_delete_span(orig_span);
                    *i = Item::Verbatim(Default::default());
                    return;
                }
                &mut m.attrs
            }
            Item::Static(s) => &mut s.attrs,
            Item::Struct(s) => &mut s.attrs,
            Item::Trait(t) => &mut t.attrs,
            Item::TraitAlias(ta) => &mut ta.attrs,
            Item::Type(t) => &mut t.attrs,
            Item::Union(u) => &mut u.attrs,
            Item::Use(u) => {
                if self.process_item_use_mut(u) {
                    self.record_delete_span(orig_span);
                    *i = Item::Verbatim(Default::default());
                    return;
                }
                // process_item_use_mut records its own targeted edits.
                &mut u.attrs
            }
            _ => {
                visit_item_mut(self, i);
                return;
            }
        };
        let mut retain = true;
        for attr in attrs.iter_mut() {
            if attr.path().is_ident("test") {
                retain = false;
            }
            if attr.path().is_ident("cfg") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("test") || meta.path.is_ident("feature") {
                        retain = false;
                    }
                    Ok(())
                });
            }
        }
        if !retain {
            self.record_delete_span(orig_span);
            *i = Item::Verbatim(Default::default());
        } else {
            let mut id = 0;
            while id < attrs.len() {
                if attrs[id].path().is_ident("cfg") || attrs[id].path().is_ident("allow") {
                    self.record_delete_span(attrs[id].span());
                    attrs.swap_remove(id);
                } else {
                    id += 1;
                }
            }
            visit_item_mut(self, i);
        }
    }

    fn visit_path_mut(&mut self, i: &mut syn::Path) {
        if i.segments.len() <= 1 {
            visit_path_mut(self, i);
            return;
        }
        // Capture pre-mutation info: without it we can't produce a
        // minimal text edit that only touches the leading segments.
        let orig_first_ident = i.segments[0].ident.to_string();
        let orig_first_span = i.segments[0].ident.span();
        let orig_second_start = i.segments[1].ident.span().start();

        let mut library = orig_first_ident.clone();
        if library == "crate" {
            library = self.cur_library.clone();
        }
        if !Library::is_library(&library) {
            visit_path_mut(self, i);
            return;
        }
        if !self.content.contains_key(&library) {
            let library = Library::new(&library);
            self.content.insert(library.root.name.clone(), library);
        }
        if i.segments.len() == 2 {
            eprintln!("{} {}", library, i.to_token_stream());
            let macro_name = i.segments[1].ident.to_string();
            if self.has_macro(&library, macro_name.as_str()) {
                i.segments[0] = syn::PathSegment {
                    ident: Ident::new("crate", i.segments[0].ident.span()),
                    arguments: syn::PathArguments::None,
                };
                self.add_macro(&library, &macro_name);
                // Macros don't get a library segment inserted — just
                // rewrite the leading `<lib>` to `crate` when it isn't
                // already `crate`.
                if orig_first_ident != "crate" {
                    self.record_replace_span(orig_first_span, "crate".to_string());
                }
                visit_path_mut(self, i);
                return;
            }
        }
        let mut path = Library::path(&library);
        let mut fqn = vec![library.clone()];
        for segment in i.segments.iter().skip(1) {
            let segment = segment.ident.to_string();
            if self
                .file_explorer
                .file_exists(format!("{}/{}", path, segment).as_str())
            {
                path = format!("{}/{}", path, segment);
                fqn.push(segment);
            } else if self
                .file_explorer
                .file_exists(format!("{}/{}.rs", path, segment).as_str())
            {
                path = format!("{}/{}.rs", path, segment);
                fqn.push(segment);
                break;
            } else if self
                .file_explorer
                .file_exists(format!("{}/mod.rs", path).as_str())
            {
                path = format!("{}/mod.rs", path);
                break;
            } else if self
                .file_explorer
                .file_exists(format!("{}/lib.rs", path).as_str())
            {
                path = format!("{}/lib.rs", path);
                break;
            } else {
                panic!("Invalid path: {}", i.to_token_stream());
            }
        }
        i.segments[0] = syn::PathSegment {
            ident: Ident::new("crate", i.segments[0].ident.span()),
            arguments: syn::PathArguments::None,
        };
        i.segments.insert(
            1,
            syn::PathSegment {
                ident: Ident::new(&library, i.segments[0].ident.span()),
                arguments: syn::PathArguments::None,
            },
        );
        self.add_file(File { path, fqn });
        // Emit a minimal textual insertion so surrounding formatting
        // (indentation, generic args on nested paths) survives. If the
        // path already starts with `crate`, splice `<lib>::` right
        // before the second segment; otherwise turn `<lib>` into
        // `crate::<lib>` by replacing the first segment.
        if orig_first_ident == "crate" {
            self.record_edit(
                orig_second_start,
                orig_second_start,
                format!("{}::", library),
            );
        } else {
            self.record_replace_span(orig_first_span, format!("crate::{}", library));
        }
        visit_path_mut(self, i);
    }

    fn visit_item_macro_mut(&mut self, i: &mut ItemMacro) {
        if i.ident.is_some() {
            let orig_tokens_span = i.mac.tokens.span();
            let body = i.mac.tokens.to_string();
            let mut state = 0;
            let mut result = String::new();
            let mut ident = String::new();
            for token in body.split(' ') {
                if state == 0 && token == "$" {
                    state = 1;
                } else if state == 1 && token == "crate" {
                    state = 2;
                } else if state == 2 && token == "::" {
                    state = 3;
                } else if state == 3 {
                    ident = token.to_string();
                    state = 4;
                    continue;
                } else if state == 4 {
                    if token == "!" {
                        result += &ident;
                        result += " ";
                    } else {
                        result += &self.cur_library;
                        result += "::";
                        result += &ident;
                        result += " ";
                    }
                    state = 0;
                } else {
                    state = 0;
                }
                result += token;
                result += " ";
            }
            if state == 4 {
                result += &self.cur_library;
                result += "::";
                result += &ident;
            }
            let new_tokens: proc_macro2::TokenStream = syn::parse_str(&result).unwrap();
            let new_body_text = new_tokens.to_string();
            i.mac.tokens = new_tokens;
            // Only record an edit when the rewrite actually changed
            // something — otherwise we'd overwrite the source-verbatim
            // macro body with a whitespace-collapsed token restring.
            if new_body_text.trim() != body.trim() {
                self.record_replace_span(orig_tokens_span, new_body_text);
            }
        }
        visit_item_macro_mut(self, i);
    }
}

impl<FE: FileExplorer> Visitor<FE> {
    pub fn new(minimize: bool, fe: FE) -> Self {
        let root = File {
            path: "src/main.rs".to_string(),
            fqn: vec!["solution".to_string()],
        };
        let mut res = Self {
            minimize,
            queue: vec![root.clone()],
            files: HashSet::new(),
            content: Default::default(),
            file_explorer: fe,
            cur_library: "solution".to_string(),
            in_root: true,
            cur_edits: Vec::new(),
            cur_line_index: None,
        };
        res.files.insert(root);
        res
    }

    pub fn build(&mut self) {
        while let Some(file_meta) = self.queue.pop() {
            if !self.content.contains_key(&file_meta.fqn[0]) {
                let library = Library::new(&file_meta.fqn[0]);
                self.content.insert(file_meta.fqn[0].clone(), library);
            }
            eprintln!("{} {:?}", file_meta.path, file_meta.fqn);
            let source = std::fs::read_to_string(&file_meta.path).expect(&file_meta.path);
            let mut file = syn::parse_file(&source).unwrap();
            self.cur_library = file_meta.fqn[0].clone();
            self.cur_edits.clear();
            self.cur_line_index = Some(LineIndex::new(&source));
            self.visit_file_mut(&mut file);
            let edits = std::mem::take(&mut self.cur_edits);
            self.cur_line_index = None;
            let library = self.content.get_mut(&file_meta.fqn[0]).unwrap();
            library.add_file(file_meta, file, source, edits);
            self.in_root = false;
        }
        let mut code = String::new();
        let solution = self.content.remove("solution").unwrap();
        if let Some(task) = crate::parse_task(&self.file_explorer) {
            if let Ok(json) = serde_json::to_string_pretty(&task) {
                let _ = std::fs::write("../../main/task.json", json);
            }
        }
        // Solution always uses the readable emitter — the user's own
        // main.rs never goes through the minifier and its comments /
        // macro layout should survive verbatim.
        code += &render_module_file(&solution.root, true);
        if !solution.root.children.is_empty() {
            code += "pub mod solution {\n";
            for module in solution.root.children.values() {
                Self::add_code(&mut code, module, true);
            }
        }
        let mut library_code = String::new();
        println!("cargo:rerun-if-changed=.");
        // Library: readable output when we're not minifying, so the
        // aggregated library keeps its comments and macro whitespace;
        // when minimize=true the code below still goes through
        // unparse+rustminify and produces the current one-liner form.
        let readable_library = !self.minimize;
        for library in self.content.values() {
            println!("cargo:rerun-if-changed=../../{}", library.root.name);
            Self::add_code(&mut library_code, &library.root, readable_library);
        }
        if self.minimize {
            // DCE uses the solution + main source to seed reachability.
            match crate::dce::eliminate(&library_code, &code) {
                Ok(pruned) => library_code = pruned,
                Err(e) => eprintln!("Skipping DCE: {}", e),
            }
            match crate::minimize::rename(&library_code) {
                Ok(renamed) => library_code = renamed,
                Err(e) => eprintln!("Skipping identifier renaming: {}", e),
            }
            let file = syn_old::parse_file(&library_code).unwrap();
            library_code = rustminify::minify_file(&file).to_string();
        }
        code += &library_code;
        std::fs::File::create("../../main/src/main.rs")
            .unwrap()
            .write_all(code.as_bytes())
            .unwrap();
    }

    fn process_item_use_mut(&mut self, i: &mut ItemUse) -> bool {
        if let UseTree::Path(l) = &mut i.tree {
            let orig_first_ident = l.ident.to_string();
            let orig_first_span = l.ident.span();
            let orig_inner_start = l.tree.span().start();

            let mut library = orig_first_ident.clone();
            if library == "crate" {
                library = self.cur_library.clone();
            }
            if !Library::is_library(&library) {
                return false;
            }
            if !self.content.contains_key(&library) {
                let library = Library::new(&library);
                self.content.insert(library.root.name.clone(), library);
            }
            l.ident = Ident::new("crate", l.ident.span());
            if !self.add_use(
                &library,
                Library::path(&library),
                vec![library.to_string()],
                l.tree.as_mut(),
            ) {
                l.tree = Box::new(UseTree::Path(UsePath {
                    ident: Ident::new(&library, l.ident.span()),
                    colon2_token: l.colon2_token,
                    tree: l.tree.clone(),
                }));
                // Targeted text edit — same shape as visit_path_mut:
                // splice `<lib>::` after `crate::`, or rewrite the
                // leading `<lib>` to `crate::<lib>`.
                if orig_first_ident == "crate" {
                    self.record_edit(
                        orig_inner_start,
                        orig_inner_start,
                        format!("{}::", library),
                    );
                } else {
                    self.record_replace_span(orig_first_span, format!("crate::{}", library));
                }
                false
            } else {
                // Macro `use`: no library segment inserted, just the
                // leading rename to `crate` (no-op if it already was).
                if orig_first_ident != "crate" {
                    self.record_replace_span(orig_first_span, "crate".to_string());
                }
                self.in_root
            }
        } else {
            false
        }
    }

    fn add_code(code: &mut String, module: &Module, readable: bool) {
        code.push_str(&format!("pub mod {} {{\n", module.name));
        code.push_str(&render_module_file(module, readable));
        for child in module.children.values() {
            Self::add_code(code, child, readable);
        }
        code.push_str("}\n");
    }

    fn add_macro(&mut self, library: &str, name: &str) {
        let library = self.content.get_mut(library).unwrap();
        let file = library.macros.get(name).unwrap().clone();
        self.add_file(file);
    }

    fn has_macro(&mut self, library: &str, name: &str) -> bool {
        let library = self.content.get(library).unwrap();
        library.macros.contains_key(name)
    }

    fn add_file(&mut self, file: File) {
        if !self.files.contains(&file) {
            self.files.insert(file.clone());
            self.queue.push(file);
        }
    }

    fn add_use_impl(&mut self, mut path: String, mut fqn: Vec<String>, tree: &UseTree) {
        match tree {
            UseTree::Path(p) => {
                let segment = p.ident.to_string();
                if self
                    .file_explorer
                    .file_exists(format!("{}/{}", path, segment).as_str())
                {
                    path = format!("{}/{}", path, segment);
                    fqn.push(segment);
                } else if self
                    .file_explorer
                    .file_exists(format!("{}/{}.rs", path, segment).as_str())
                {
                    path = format!("{}/{}.rs", path, segment);
                    fqn.push(segment);
                    self.add_file(File { path, fqn });
                    return;
                } else if self
                    .file_explorer
                    .file_exists(format!("{}/mod.rs", path).as_str())
                {
                    path = format!("{}/mod.rs", path);
                    self.add_file(File { path, fqn });
                    return;
                } else if self
                    .file_explorer
                    .file_exists(format!("{}/lib.rs", path).as_str())
                {
                    path = format!("{}/lib.rs", path);
                    self.add_file(File { path, fqn });
                    return;
                } else {
                    panic!("Invalid path: {}", tree.to_token_stream());
                }
                self.add_use_impl(path, fqn, p.tree.as_ref());
            }
            UseTree::Name(_) | UseTree::Rename(_) => {
                if self
                    .file_explorer
                    .file_exists(format!("{}/mod.rs", path).as_str())
                {
                    path = format!("{}/mod.rs", path);
                    self.add_file(File { path, fqn });
                } else if self
                    .file_explorer
                    .file_exists(format!("{}/lib.rs", path).as_str())
                {
                    path = format!("{}/lib.rs", path);
                    self.add_file(File { path, fqn });
                } else {
                    panic!("Invalid path: {}", tree.to_token_stream());
                }
            }
            UseTree::Glob(_) => {
                panic!("Can't use glob imports: {}", tree.to_token_stream());
            }
            UseTree::Group(group) => {
                for item in group.items.iter() {
                    self.add_use_impl(path.clone(), fqn.clone(), item);
                }
            }
        }
    }

    fn add_use(&mut self, library: &str, path: String, fqn: Vec<String>, tree: &UseTree) -> bool {
        let lib = self.content.get(library).unwrap();
        match tree {
            UseTree::Name(name) => {
                let ident = name.ident.to_string();
                if lib.macros.contains_key(&ident) {
                    self.add_macro(library, &ident);
                    return true;
                }
            }
            UseTree::Rename(rename) => {
                let ident = rename.ident.to_string();
                if lib.macros.contains_key(&ident) {
                    self.add_macro(library, &ident);
                    return true;
                }
            }
            UseTree::Group(group) => {
                let mut has_macro = false;
                let mut has_non_macro = false;
                for item in group.items.iter() {
                    if self.add_use(library, path.clone(), fqn.clone(), item) {
                        has_macro = true;
                    } else {
                        has_non_macro = true;
                    }
                }
                if has_macro && has_non_macro {
                    panic!(
                        "Can't mix macros and non-macros in one group: {}",
                        tree.to_token_stream()
                    );
                }
                return has_macro;
            }
            _ => {}
        }
        self.add_use_impl(path, fqn, tree);
        false
    }
}

/// Render this module's own file (children are handled by the caller).
/// In `readable` mode we apply the collected edits to the original
/// source; otherwise we fall back to `prettyplease` on the mutated AST.
fn render_module_file(module: &Module, readable: bool) -> String {
    let Some(file) = module.file.as_ref() else {
        return String::new();
    };
    if readable {
        if let Some(source) = &module.source {
            return apply_edits(source, &module.edits);
        }
    }
    unparse(file)
}
