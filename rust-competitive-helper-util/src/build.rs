use crate::read_lines;
use std::collections::{HashMap, HashSet};

const LIB_NAME: &str = "algo_lib";

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
                match build_use_tree(usage[start..i].iter().cloned().collect::<String>().as_str()) {
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
                tag: usage.iter().cloned().collect(),
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
                    tag: usage[..pos].iter().cloned().collect(),
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
) -> Vec<(String, Vec<String>)> {
    let mut res = Vec::new();
    let mut add = false;
    for usage in usages.iter() {
        if usage.children.is_empty() {
            if root {
                let (file_name, fqn) = all_macro.get(&usage.tag).unwrap();
                res.push((file_name.clone(), fqn.clone()));
            } else {
                add = true;
            }
        } else {
            let mut fqn_path = fqn_path.clone();
            fqn_path.push(usage.tag.clone());
            res.append(&mut all_files_impl(
                &usage.children,
                format!("{}/{}", prefix, usage.tag),
                fqn_path,
                false,
                all_macro,
            ));
        }
    }
    if add {
        res.push((prefix + ".rs", fqn_path));
    }
    res
}

/// Returns file names and fqn paths
fn all_files(
    usage_tree: &UsageTree,
    all_macro: &HashMap<String, (String, Vec<String>)>,
) -> Vec<(String, Vec<String>)> {
    all_files_impl(
        &usage_tree.children,
        format!("../{}/src", LIB_NAME),
        Vec::new(),
        true,
        all_macro,
    )
}

#[derive(Ord, PartialOrd, Eq, PartialEq)]
struct CodeFile {
    fqn: Vec<String>,
    content: Vec<String>,
}

fn find_usages_and_code(
    file: &str,
    prefix: &str,
    fqn_path: Vec<String>,
    processed: &mut HashSet<String>,
    all_macro: &HashMap<String, (String, Vec<String>)>,
) -> Vec<CodeFile> {
    let mut code = Vec::new();
    let mut all_code = Vec::new();
    let mut main = false;

    let mut lines = read_lines(file).into_iter();
    while let Some(mut line) = lines.next() {
        if line.as_str() == "//START MAIN" {
            main = true;
            continue;
        }
        if line.as_str() == "//END MAIN" {
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
            code.push(line.replace(LIB_NAME, "crate"));
            match build_use_tree_full_line(&line) {
                BuildResult::Usage(usage) => {
                    if usage.tag.as_str() == prefix {
                        let all = all_files(&usage, all_macro);
                        for (file, fqn_path) in all {
                            if !processed.contains(&file) {
                                processed.insert(file.clone());
                                let call_code = find_usages_and_code(
                                    file.as_str(),
                                    "crate",
                                    fqn_path,
                                    processed,
                                    all_macro,
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
        } else {
            code.push(line.clone());
        }
    }

    all_code.push(CodeFile {
        content: code,
        fqn: fqn_path,
    });

    all_code
}

fn build_code(mut prefix: Vec<String>, mut to_add: &mut [CodeFile], code: &mut Vec<String>) {
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

fn find_macro_impl(
    path: String,
    fqn: Vec<String>,
    res: &mut HashMap<String, (String, Vec<String>)>,
) {
    let mut paths = std::fs::read_dir(path)
        .unwrap()
        .map(|res| res.unwrap())
        .collect::<Vec<_>>();
    paths.sort_by_key(|a| a.path());
    for path in paths {
        if path.file_type().unwrap().is_file() {
            let filename = path.file_name();
            let filename = filename.to_str().unwrap();
            if let Some(filename) = filename.strip_suffix(".rs") {
                let text = crate::read_from_file(path.path());
                let mut text = text.as_str();
                while let Some(pos) = text.find("#[macro_export]") {
                    text = &text[pos..];
                    let pos = text.find("macro_rules!").unwrap();
                    text = &text[(pos + "macro_rules!".len())..];
                    let pos = text.find('{').unwrap();
                    let macro_name = &text[..pos].trim();
                    let mut fqn = fqn.clone();
                    fqn.push(filename.to_string());
                    res.insert(
                        macro_name.to_string(),
                        (
                            path.path().to_str().unwrap().replace('\\', "/").to_string(),
                            fqn,
                        ),
                    );
                }
            }
        } else if path.file_type().unwrap().is_dir() {
            let mut fqn = fqn.clone();
            fqn.push(path.file_name().to_str().unwrap().to_string());
            find_macro_impl(path.path().to_str().unwrap().to_string(), fqn, res);
        }
    }
}

fn find_macro() -> HashMap<String, (String, Vec<String>)> {
    let root = format!("../{}/src", LIB_NAME);
    let mut res = HashMap::new();
    find_macro_impl(root, Vec::new(), &mut res);
    res
}

fn add_rerun_if_changed_instructions() {
    let add = |file: &str| {
        println!("cargo:rerun-if-changed={}", file);
    };
    add(".");
    add(&format!("../{}", LIB_NAME));
}

pub fn build() {
    let all_macro = find_macro();
    let mut all_code = find_usages_and_code(
        "src/main.rs",
        LIB_NAME,
        Vec::new(),
        &mut HashSet::new(),
        &all_macro,
    );
    let mut code = Vec::new();
    all_code.sort();
    build_code(Vec::new(), all_code.as_mut_slice(), &mut code);
    code.push("fn main() {".to_string());
    code.push("    crate::solution::submit();".to_string());
    code.push("}".to_string());
    crate::write_lines("../main/src/main.rs", code);
    add_rerun_if_changed_instructions();
}
