use crate::{
    file_explorer::{FileExplorer, RealFileExplorer},
    IOEnum, Task,
};
use std::{
    collections::{HashMap, HashSet},
    env,
};

#[derive(Debug)]
struct UsageTree {
    tag: String,
    children: Vec<UsageTree>,
}

#[derive(Debug)]
enum BuildResult {
    Usage(UsageTree),
    Children(Vec<UsageTree>),
}

#[allow(unused)]
fn log(msg: &str) {
    if env::var("LOG").is_ok() {
        println!("cargo:warning={}", msg);
    }
}

fn build_use_tree(usage: &str) -> BuildResult {
    let usage = usage.trim().chars().collect::<Vec<_>>();
    if usage[0usize] == '{' {
        let mut level = 0usize;
        let mut res = Vec::new();
        let mut start = 1usize;
        for i in 1usize..usage.len() - 1usize {
            if usage[i] == '{' {
                level += 1;
            } else if usage[i] == '}' {
                level -= 1;
            } else if usage[i] == ',' && level == 0usize {
                match build_use_tree(usage[start..i].iter().collect::<String>().as_str()) {
                    BuildResult::Usage(usage) => {
                        res.push(usage);
                    }
                    BuildResult::Children(_) => {
                        unreachable!()
                    }
                }
                start = i + 1;
            }
        }
        {
            let children = usage[start..usage.len() - 1usize]
                .iter()
                .cloned()
                .collect::<String>();
            if children.trim().is_empty() {
                // allow trailing comma (e.g. in multiline use statement)
            } else {
                match build_use_tree(&children) {
                    BuildResult::Usage(usage) => {
                        res.push(usage);
                    }
                    BuildResult::Children(_) => {
                        unreachable!()
                    }
                }
            }
        }
        BuildResult::Children(res)
    } else {
        match usage.iter().position(|&r| r == ':') {
            None => BuildResult::Usage(UsageTree {
                tag: usage.iter().collect(),
                children: Vec::new(),
            }),
            Some(pos) => {
                let children = match build_use_tree(
                    usage[pos + 2usize..]
                        .iter()
                        .cloned()
                        .collect::<String>()
                        .as_str(),
                ) {
                    BuildResult::Usage(usage) => {
                        vec![usage]
                    }
                    BuildResult::Children(children) => children,
                };
                BuildResult::Usage(UsageTree {
                    tag: usage[..pos].iter().collect(),
                    children,
                })
            }
        }
    }
}

fn build_use_tree_full_line(line: &str) -> BuildResult {
    assert!(line.starts_with("use "));
    assert!(line.ends_with(';'));
    build_use_tree(&line[4..(line.len() - 1)])
}

#[cfg(test)]
mod build_use_tree_tests {
    use crate::build::build_use_tree_full_line;
    use expect_test::expect;

    #[test]
    fn multiline_use() {
        let line = "use algo_lib::graph::strongly_connected_components::{find_order, find_strongly_connected_component,};";
        let expected = expect![[
            r#"Usage(UsageTree { tag: "algo_lib", children: [UsageTree { tag: "graph", children: [UsageTree { tag: "strongly_connected_components", children: [UsageTree { tag: "find_order", children: [] }, UsageTree { tag: "find_strongly_connected_component", children: [] }] }] }] })"#
        ]];
        expected.assert_eq(&format!("{:?}", build_use_tree_full_line(line)));
    }

    #[test]
    fn random_nested_struct() {
        let line =
            "use algo_lib::{graph::{compressed_graph, edges},io::{input::Input, output},misc,};";
        let expected = expect![[
            r#"Usage(UsageTree { tag: "algo_lib", children: [UsageTree { tag: "graph", children: [UsageTree { tag: "compressed_graph", children: [] }, UsageTree { tag: "edges", children: [] }] }, UsageTree { tag: "io", children: [UsageTree { tag: "input", children: [UsageTree { tag: "Input", children: [] }] }, UsageTree { tag: "output", children: [] }] }, UsageTree { tag: "misc", children: [] }] })"#
        ]];
        expected.assert_eq(&format!("{:?}", build_use_tree_full_line(line)));
    }

    #[test]
    fn simple() {
        let line = "use std::collections::HashSet;";
        let expected = expect![[
            r#"Usage(UsageTree { tag: "std", children: [UsageTree { tag: "collections", children: [UsageTree { tag: "HashSet", children: [] }] }] })"#
        ]];
        expected.assert_eq(&format!("{:?}", build_use_tree_full_line(line)));
    }
}

/// Returns file names and fqn paths
fn all_files_impl(
    usages: &[UsageTree],
    prefix: String,
    fqn_path: Vec<String>,
    root: bool,
    all_macro: &HashMap<String, (String, Vec<String>)>,
    file_explorer: &impl FileExplorer,
) -> Vec<(String, Vec<String>)> {
    let mut res = Vec::new();
    let this_file = format!("{}.rs", prefix);
    if file_explorer.file_exists(&this_file) {
        res.push((this_file, fqn_path));
        return res;
    }
    let this_mod = format!("{}/{}.rs", prefix, if root { "lib" } else { "mod" });
    if file_explorer.file_exists(&this_mod) {
        res.push((this_mod, fqn_path.clone()));
    }
    for usage in usages.iter() {
        if root {
            if let Some((file_name, fqn)) = all_macro.get(&usage.tag) {
                res.push((file_name.clone(), fqn.clone()));
                continue;
            }
        }
        if !usage.children.is_empty() {
            let mut fqn_path = fqn_path.clone();
            fqn_path.push(usage.tag.clone());
            res.append(&mut all_files_impl(
                &usage.children,
                format!("{}/{}", prefix, usage.tag),
                fqn_path,
                false,
                all_macro,
                file_explorer,
            ));
        }
    }
    res
}

/// Returns file names and fqn paths
fn all_files(
    usage_tree: &UsageTree,
    all_macro: &HashMap<String, (String, Vec<String>)>,
    library: &Option<String>,
    file_explorer: &impl FileExplorer,
) -> Vec<(String, Vec<String>)> {
    let (prefix, fqn_path) = match library {
        Some(library) => (format!("../../{}/src", library), vec![library.clone()]),
        None => ("src".to_owned(), vec![]),
    };
    all_files_impl(&usage_tree.children, prefix, fqn_path, true, all_macro, file_explorer)
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Clone)]
struct CodeFile {
    fqn: Vec<String>,
    content: Vec<String>,
}

// Inside solution we replace:
// ``use algo_lib::`` with ``use crate::algo_lib``
//
// Inside [algo_lib] we replace
// ``use crate::`` with ``use crate::algo_lib``
//
// Also special case for macros, we replace them with
// ``use crate::*macro*``
//
fn add_usages_to_code(
    code: &mut Vec<String>,
    usage_tree: &UsageTree,
    mut path: Vec<String>,
    all_macro: &HashMap<String, (String, Vec<String>)>,
    libraries: &[String],
) {
    path.push(usage_tree.tag.clone());
    if usage_tree.children.is_empty() {
        if !path.is_empty() && libraries.contains(&path[0]) {
            if path.len() == 2 && all_macro.contains_key(&path[1]) {
                // special case for macros
                code.push(format!("use crate::{};", path[1]));
            } else {
                code.push(format!("use crate::{};", path.join("::")));
            }
        } else {
            // common code (e.g. standard library)
            // also multi-file solutions goes this path
            // with path[0] == "crate"
            code.push(format!("use {};", path.join("::")));
        }
    } else {
        for child in usage_tree.children.iter() {
            add_usages_to_code(code, child, path.clone(), all_macro, libraries)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn find_usages_and_code<F: FileExplorer>(
    file: &str,
    current_lib: Option<String>,
    fqn_path: Vec<String>,
    processed: &mut HashSet<String>,
    all_macro: &HashMap<String, (String, Vec<String>)>,
    libraries: &[String],
    file_explorer: &F,
    minimize: bool,
) -> Vec<CodeFile> {
    let mut code = Vec::new();
    let mut all_code = Vec::new();
    let mut main = false;

    log(&format!("Parsing file {}...", file));

    let mut lines = file_explorer.read_file(file).into_iter();
    while let Some(mut line) = lines.next() {
        if line.contains("//START MAIN") {
            main = true;
            continue;
        }
        if line.contains("//END MAIN") {
            main = false;
            continue;
        }
        if main {
            continue;
        }
        if line.trim().starts_with("use ") {
            line = line.trim().to_string();
            while !line.ends_with(';') {
                let next_line = lines
                    .next()
                    .expect("expect ; in the end of `use` line, end of file found");
                line += next_line.trim();
            }
            log(&format!("In file {}, see line: {}", file, line));
            match build_use_tree_full_line(&line) {
                BuildResult::Usage(usage) => {
                    if usage.tag == "crate" {
                        let path = match &current_lib {
                            None => vec!["crate".to_owned()],
                            Some(lib) => vec![lib.clone()],
                        };
                        for child in usage.children.iter() {
                            add_usages_to_code(&mut code, child, path.clone(), all_macro, libraries)
                        }
                    } else {
                        add_usages_to_code(&mut code, &usage, vec![], all_macro, libraries);
                    };
                    // TODO: support `usage.tag` == "super"
                    if usage.tag == "crate" || libraries.contains(&usage.tag) {
                        log(&format!("fqn path = {:?}", fqn_path));
                        let library = if usage.tag == "crate" {
                            current_lib.clone()
                        } else {
                            Some(usage.tag.clone())
                        };
                        let all = all_files(&usage, all_macro, &library, file_explorer);
                        log(&format!(
                            "Usage: {:?}, need to check recursively: {:?}",
                            &usage, all
                        ));
                        for (file, fqn_path) in all {
                            if !processed.contains(&file) {
                                processed.insert(file.clone());
                                let call_code = find_usages_and_code(
                                    file.as_str(),
                                    library.clone(),
                                    fqn_path,
                                    processed,
                                    all_macro,
                                    libraries,
                                    file_explorer,
                                    minimize,
                                );
                                all_code.extend(call_code);
                            }
                        }
                    }
                }
                BuildResult::Children(_) => {
                    unreachable!()
                }
            }
        } else if (line.trim().starts_with("mod ") || line.trim().starts_with("pub mod ")) && line.trim().ends_with(';') {
            if let Some(last_line) = code.last() {
                if last_line.trim() == "#[cfg(test)]" {
                    code.pop();
                }
            }
            // In case of multi-file solution, [main.rs] could register other files
            // with "mod ...;". As we put everything into one file, we don't need to
            // do it.
        } else {
            if let Some(library) = &current_lib {
                line = line.replace("$crate::", &format!("$crate::{}::", library));
            }
            let line = if minimize {
                line.trim().to_owned()
            } else {
                line
            };
            code.push(line);
        }
    }

    all_code.push(CodeFile {
        content: code,
        fqn: fqn_path,
    });

    all_code
}

fn build_code(mut prefix: Vec<String>, mut to_add: &mut [CodeFile], code: &mut Vec<String>) {
    if prefix.is_empty() {
        log("Build code:");
        for code_file in to_add.iter() {
            log(&format!("{:?}", code_file.fqn));
        }
    }

    if to_add[0].fqn == prefix {
        if prefix.is_empty() {
            code.push("pub mod solution {".to_string());
        }
        code.append(&mut to_add[0].content);
        if prefix.is_empty() {
            code.push("}".to_string());
        }
        to_add = &mut to_add[1..];
    }
    if to_add.is_empty() {
        return;
    }
    let index = prefix.len();
    loop {
        let mut found = false;
        for i in 1..to_add.len() {
            if to_add[i].fqn[index] != to_add[i - 1].fqn[index] {
                let mut prefix = prefix.clone();
                let mod_name = to_add[i - 1].fqn[index].clone();
                prefix.push(mod_name.clone());
                code.push(format!("pub mod {} {{", mod_name));
                build_code(prefix, &mut to_add[..i], code);
                code.push("}".to_string());
                found = true;
                to_add = &mut to_add[i..];
                break;
            }
        }
        if !found {
            let mod_name = to_add[0].fqn[index].clone();
            prefix.push(mod_name.clone());
            code.push(format!("pub mod {} {{", mod_name));
            build_code(prefix, to_add, code);
            code.push("}".to_string());
            return;
        }
    }
}

fn gen_fqn_by_path(path: &str) -> Vec<String> {
    path.split('/')
        .map(|s| s.strip_suffix(".rs").unwrap_or(s))
        .map(str::to_string)
        .collect()
}

fn find_macro_impl<F: FileExplorer>(
    path_prefix: &str,
    lib_name: &str,
    res: &mut HashMap<String, (String, Vec<String>)>,
    file_explorer: &F,
) {
    let rs_files = file_explorer.get_all_rs_files(path_prefix);
    for path in rs_files.iter() {
        let full_text = file_explorer
            .read_file(&format!("{}{}", path_prefix, path))
            .concat()
            .to_string();
        let mut text = &full_text[..];
        while let Some(pos) = text.find("#[macro_export]") {
            text = &text[pos..];
            let pos = text.find("macro_rules!").unwrap();
            text = &text[(pos + "macro_rules!".len())..];
            let pos = text.find('{').unwrap();
            let macro_name = &text[..pos].trim();
            let mut fqn = gen_fqn_by_path(path);
            fqn.insert(0, lib_name.to_owned());
            res.insert(
                macro_name.to_string(),
                (
                    format!("{}{}", path_prefix, path)
                        .replace('\\', "/")
                        .to_string(),
                    fqn,
                ),
            );
        }
    }
}

fn find_macro<F: FileExplorer>(
    libraries: &[String],
    file_explorer: &F,
) -> HashMap<String, (String, Vec<String>)> {
    log("Find all macros...");
    let mut res = HashMap::new();
    for lib in libraries.iter() {
        let root = format!("../../{}/src/", lib);
        find_macro_impl(&root, lib, &mut res, file_explorer);
    }
    log(&format!("Found macros: {:?}", res));
    res
}

fn add_rerun_if_changed_instructions(libraries: &[String]) {
    let add = |file: &str| {
        println!("cargo:rerun-if-changed={}", file);
    };
    add(".");
    for lib in libraries.iter() {
        add(&format!("../../{}", lib));
    }
}

fn parse_task<F: FileExplorer>(file_explorer: &F) -> Option<Task> {
    // Task json should be written in the first line of the main.rs
    let first_line = file_explorer
        .read_file("src/main.rs")
        .into_iter()
        .find(|s| !s.trim().is_empty())?;
    let first_line = first_line.trim();
    if !first_line.starts_with("//") {
        return None;
    }
    let first_line = &first_line[2..];
    serde_json::from_str::<Task>(first_line).ok()
}

fn build_main_fun<F: FileExplorer>(file_explorer: &F) -> String {
    if !file_explorer.file_exists("../../templates/main/main.rs") {
        // fallback to the old mode
        return "fn main() {\n    crate::solution::submit();\n}".to_string();
    }

    let read_file = |filename: &str| -> String { file_explorer.read_file(filename).join("\n") };

    let mut main = read_file("../../templates/main/main.rs");
    let task = parse_task(file_explorer).expect("Can't parse task json");

    match task.input.io_type {
        IOEnum::StdIn => {
            main = main.replace("$INPUT", &read_file("../../templates/main/stdin.rs"));
        }
        IOEnum::Regex => {
            main = main.replace("$INPUT", &read_file("../../templates/main/regex.rs"));
        }
        IOEnum::File => {
            main = main.replace("$INPUT", &read_file("../../templates/main/file_in.rs"));
        }
        IOEnum::StdOut => {
            unreachable!()
        }
    }
    match task.output.io_type {
        IOEnum::StdOut => {
            main = main.replace("$OUTPUT", &read_file("../../templates/main/stdout.rs"));
        }
        IOEnum::File => {
            main = main.replace("$OUTPUT", &read_file("../../templates/main/file_out.rs"));
        }
        IOEnum::Regex | IOEnum::StdIn => {
            unreachable!()
        }
    }
    if task.interactive {
        main = main.replace(
            "$INTERACTIVE",
            &read_file("../../templates/interactive.rs"),
        );
    } else {
        main = main.replace(
            "$INTERACTIVE",
            &read_file("../../templates/classic.rs"),
        );
    }
    if let Some(in_file) = task.input.file_name {
        main = main.replace("$IN_FILE", in_file.as_str());
    }
    if let Some(pattern) = task.input.pattern {
        main = main.replace("$PATTERN", pattern.as_str());
    }
    if let Some(out_file) = task.output.file_name {
        main = main.replace("$OUT_FILE", out_file.as_str());
    }
    main
}

pub(crate) fn build_several_libraries_impl<F: FileExplorer>(
    libraries: &[String],
    file_explorer: &F,
    minimize: bool,
) -> Vec<String> {
    let all_macro = find_macro(libraries, file_explorer);
    let mut all_code = find_usages_and_code(
        "src/main.rs",
        None,
        Vec::new(),
        &mut HashSet::new(),
        &all_macro,
        libraries,
        file_explorer,
        minimize,
    );
    let mut code = Vec::new();
    if let Some(task) = parse_task(file_explorer) {
        code.push(format!("// {}", task.url));
    }

    // try to put real new code on top of the generated file
    all_code.sort_by_key(|code_file| -> (bool, CodeFile) {
        let is_library_code = match code_file.fqn.get(0) {
            Some(module) => libraries.contains(module),
            None => false,
        };
        (is_library_code, code_file.clone())
    });
    build_code(Vec::new(), all_code.as_mut_slice(), &mut code);
    let main = build_main_fun(file_explorer);
    code.push(main);
    code
}

pub fn build_several_libraries(libraries: &[String], minimize: bool) {
    let file_explorer = RealFileExplorer::new();
    let code = build_several_libraries_impl(libraries, &file_explorer, minimize);

    crate::write_lines("../../main/src/main.rs", code);
    add_rerun_if_changed_instructions(libraries);
}

pub fn build() {
    build_several_libraries(&["algo_lib".to_owned()], false);
}
