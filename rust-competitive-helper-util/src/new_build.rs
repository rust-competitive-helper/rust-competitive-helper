use crate::file_explorer::{FileExplorer, RealFileExplorer};
use prettyplease::unparse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use syn::__private::ToTokens;
use syn::visit::{visit_item_macro, Visit};
use syn::visit_mut::{visit_item_macro_mut, visit_item_mut, visit_path_mut, VisitMut};
use syn::{Ident, Item, ItemMacro, ItemUse, UsePath, UseTree};

#[derive(Clone)]
pub struct Module {
    name: String,
    children: BTreeMap<String, Module>,
    file: Option<syn::File>,
}

#[derive(Clone)]
pub struct Library {
    macros: HashMap<String, File>,
    root: Module,
}

impl Library {
    fn new(name: &str) -> Self {
        assert!(Self::is_library(name));
        let path = Self::path(name);
        let mut res = Self {
            macros: HashMap::new(),
            root: Module {
                name: name.to_string(),
                children: Default::default(),
                file: None,
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

    fn add_file(&mut self, file_meta: File, file: syn::File) {
        let mut cur = &mut self.root;
        for module in file_meta.fqn.into_iter().skip(1) {
            if !cur.children.contains_key(&module) {
                cur.children.insert(
                    module.clone(),
                    Module {
                        name: module.clone(),
                        children: Default::default(),
                        file: None,
                    },
                );
            }
            cur = cur.children.get_mut(&module).unwrap();
        }
        cur.file = Some(file);
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
}

impl<FE: FileExplorer> VisitMut for Visitor<FE> {
    fn visit_item_mut(&mut self, i: &mut Item) {
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
                    *i = Item::Verbatim(Default::default());
                    return;
                }
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
            *i = Item::Verbatim(Default::default());
        } else {
            for i in (0..attrs.len()).rev() {
                if attrs[i].path().is_ident("cfg") {
                    attrs.swap_remove(i);
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
        if let Some(library) = i.segments.first() {
            let mut library = library.ident.to_string();
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
        }
        visit_path_mut(self, i);
    }

    fn visit_item_macro_mut(&mut self, i: &mut ItemMacro) {
        if i.ident.is_some() {
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
            i.mac.tokens = syn::parse_str(&result).unwrap();
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
            let mut file =
                syn::parse_file(&std::fs::read_to_string(&file_meta.path).expect(&file_meta.path))
                    .unwrap();
            self.cur_library = file_meta.fqn[0].clone();
            self.visit_file_mut(&mut file);
            let library = self.content.get_mut(&file_meta.fqn[0]).unwrap();
            library.add_file(file_meta, file);
            self.in_root = false;
        }
        let mut code = String::new();
        let solution = self.content.remove("solution").unwrap();
        if let Some(task) = crate::parse_task(&self.file_explorer) {
            code.push_str(&format!("// {}\n", task.url));
        }
        code += unparse(solution.root.file.as_ref().unwrap()).as_str();
        if !solution.root.children.is_empty() {
            code += "pub mod solution {\n";
            for module in solution.root.children.values() {
                Self::add_code(&mut code, module);
            }
        }
        println!("cargo:rerun-if-changed=.");
        for library in self.content.clone().into_values() {
            println!("cargo:rerun-if-changed=../../{}", library.root.name);
            Self::add_code(&mut code, &library.root);
        }
        std::fs::File::create("../../main/src/main.rs")
            .unwrap()
            .write_all(code.as_bytes())
            .unwrap();
        if self.minimize {
            let file =
                syn_old::parse_file(&std::fs::read_to_string("../../main/src/main.rs").unwrap())
                    .unwrap();
            let mut minimized = rustminify::minify_file(&file);
            if let Some(task) = crate::parse_task(&self.file_explorer) {
                minimized = format!("// {}\n", task.url) + &minimized;
            }
            std::fs::File::create("../../main/src/main.rs")
                .unwrap()
                .write_all(minimized.to_string().as_bytes())
                .unwrap();
        }
    }

    fn process_item_use_mut(&mut self, i: &mut ItemUse) -> bool {
        if let UseTree::Path(l) = &mut i.tree {
            let mut library = l.ident.to_string();
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
                false
            } else {
                self.in_root
            }
        } else {
            false
        }
    }

    fn add_code(code: &mut String, module: &Module) {
        code.push_str(&format!("pub mod {} {{\n", module.name));
        if let Some(file) = module.file.as_ref() {
            code.push_str(unparse(file).as_str());
        }
        for child in module.children.values() {
            Self::add_code(code, child);
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
