use crate::{read_lines, IOEnum, Task};
use std::collections::HashSet;
use std::io::Write;

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
    assert!(line.ends_with(";"));
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
        let expected = expect![[r#"Usage(UsageTree { tag: "std", children: [UsageTree { tag: "collections", children: [UsageTree { tag: "HashSet", children: [] }] }] })"#]];
        expected.assert_eq(&format!("{:?}", build_use_tree_full_line(line)));
    }
}

fn all_files_impl(usages: &Vec<UsageTree>, prefix: String, root: bool) -> Vec<String> {
    let mut res = Vec::new();
    let mut add = false;
    for usage in usages.iter() {
        if usage.children.is_empty() {
            add = true;
        } else {
            res.append(&mut all_files_impl(
                &usage.children,
                format!("{}/{}", prefix, usage.tag),
                false,
            ));
        }
    }
    if add && !root {
        res.push(prefix + ".rs");
    }
    res
}

fn all_files(usage_tree: &UsageTree) -> Vec<String> {
    all_files_impl(&usage_tree.children, "../algo_lib/src".to_string(), true)
}

fn all_usages_impl(usages: &Vec<UsageTree>, prefix: String) -> Vec<String> {
    let mut res = Vec::new();
    for usage in usages.iter() {
        if usage.children.is_empty() {
            res.push(format!("{}::{};", prefix, usage.tag));
        } else {
            res.append(&mut all_usages_impl(
                &usage.children,
                format!("{}::{}", prefix, usage.tag),
            ));
        }
    }
    res
}

fn all_usages(usage_tree: &UsageTree) -> Vec<String> {
    all_usages_impl(&usage_tree.children, format!("use {}", usage_tree.tag))
}

fn find_usages_and_code(
    file: &str,
    prefix: &str,
    processed: &mut HashSet<String>,
) -> (HashSet<String>, Vec<String>, Option<Task>) {
    let mut usages = HashSet::new();
    let mut code = Vec::new();
    let mut main = false;
    let mut task = None;

    let mut lines = read_lines(file).into_iter();
    if prefix == "algo_lib" {
        let task_json = lines.next().unwrap().chars().skip(2).collect::<String>();
        task = Some(serde_json::from_str::<Task>(task_json.as_str()).unwrap());
    }
    while let Some(mut line) = lines.next() {
        if line.starts_with("use") {
            while !line.ends_with(";") {
                let next_line = lines
                    .next()
                    .expect("expect ; in the end of `use` line, end of file found");
                line += next_line.trim();
            }
            match build_use_tree_full_line(&line) {
                BuildResult::Usage(usage) => {
                    if usage.tag.as_str() == prefix {
                        let all = all_files(&usage);
                        for file in all {
                            if !processed.contains(&file) {
                                processed.insert(file.clone());
                                let (call_usages, mut call_code, ..) =
                                    find_usages_and_code(file.as_str(), "crate", processed);
                                usages.extend(call_usages);
                                code.append(&mut call_code);
                            }
                        }
                    } else {
                        usages.extend(all_usages(&usage));
                    }
                }
                BuildResult::Children(_) => {
                    unreachable!()
                }
            }
        } else {
            if line.as_str() == "//START MAIN" {
                main = true;
            }
            if !main {
                code.push(line.clone());
            }
            if line.as_str() == "//END MAIN" {
                main = false;
            }
        }
    }

    (usages, code, task)
}

pub fn build() {
    let (usages, mut code, task) =
        find_usages_and_code("src/main.rs", "algo_lib", &mut HashSet::new());
    code.push("fn main() {".to_string());
    let task = task.unwrap();
    match task.input.io_type {
        IOEnum::StdIn | IOEnum::Regex => {
            code.push("    let mut sin = std::io::stdin();".to_string());
            if task.interactive {
                code.push("    let input = Input::new_with_size(&mut sin, 1);".to_string());
            } else {
                code.push("    let input = Input::new(&mut sin);".to_string());
            }
        }
        IOEnum::File => {
            code.push(format!(
                "    let mut in_file = std::fs::File::open(\"{}\").unwrap();",
                task.input.file_name.unwrap()
            ));
            if task.interactive {
                code.push("    let input = Input::new_with_size(&mut in_file, 1);".to_string());
            } else {
                code.push("    let input = Input::new(&mut in_file);".to_string());
            }
        }
        _ => {
            unreachable!()
        }
    }
    match task.output.io_type {
        IOEnum::StdOut => {
            code.push("    unsafe {".to_string());
            if task.interactive {
                code.push(format!(
                    "        OUTPUT = Some(Output::new_with_auto_flush(Box::new(std::io::stdout())));"
                ));
            } else {
                code.push(format!(
                    "        OUTPUT = Some(Output::new(Box::new(std::io::stdout())));"
                ));
            }
            code.push("    }".to_string());
        }
        IOEnum::File => {
            code.push(format!(
                "    let out_file = std::fs::File::create(\"{}\").unwrap();",
                task.output.file_name.unwrap()
            ));
            code.push("    unsafe {".to_string());
            if task.interactive {
                code.push(format!(
                    "        OUTPUT = Some(Output::new_with_auto_flush(Box::new(out_file)));"
                ));
            } else {
                code.push(format!(
                    "        OUTPUT = Some(Output::new(Box::new(out_file)));"
                ));
            }
            code.push("    }".to_string());
        }
        _ => {
            unreachable!()
        }
    }
    code.push("    run(input);".to_string());
    code.push("}".to_string());
    let mut out = std::fs::File::create("../main/src/main.rs").unwrap();
    for usage in usages {
        out.write(usage.as_bytes()).unwrap();
        out.write("\n".as_bytes()).unwrap();
    }
    for line in code {
        out.write(line.as_bytes()).unwrap();
        out.write("\n".as_bytes()).unwrap();
    }
    out.flush().unwrap();
}
