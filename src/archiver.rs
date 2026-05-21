use chrono::{Datelike, Utc};
use dialoguer::console::Term;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;
use itertools::Itertools;
use rust_competitive_helper_util::{load_task, Task};
use std::collections::BTreeMap;
use std::fs;
use std::fs::{read_dir, remove_dir_all, rename};
use std::iter::once;

fn contest_name(group: &str) -> String {
    match group.find('-') {
        None => group.to_string(),
        Some(at) => group.split_at(at + 1).1.trim().to_string(),
    }
}

pub fn contest_list() -> Vec<(String, Vec<String>)> {
    let mut result = BTreeMap::new();
    for task_name in read_dir("tasks").unwrap().map(|entry| {
        entry
            .unwrap()
            .path()
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    }) {
        let Some(task) = load_task(format!("tasks/{}", task_name)) else {
            continue;
        };
        let contest_name = contest_name(&task.group);
        result.entry(contest_name).or_insert_with(Vec::new).push(task_name);
    }
    result.into_iter().collect()
}

const OPTIONS: [&str; 4] = ["Skip", "Delete", "Archive only", "Archive and tests"];

fn find_additional_solution_files(task_name: &str) -> Vec<String> {
    rust_competitive_helper_util::all_rs_files_in_dir(format!("tasks/{}/src", task_name))
        .into_iter()
        .filter(|file| file != "main.rs" && file != "tester.rs")
        .collect()
}

pub fn ask_archive(task_name: String, selection: usize) {
    if selection == 0 {
        return;
    }
    if selection >= 2 {
        let now = Utc::now();
        let mut main =
            rust_competitive_helper_util::read_lines(format!("tasks/{}/src/main.rs", task_name))
                .unwrap();
        let task: Task = load_task(format!("tasks/{}", task_name))
            .expect("task config missing");
        let path = format!(
            "archive/{}/{:02}/{}.{:02}.{:02} - {}",
            now.year(),
            now.month(),
            now.year(),
            now.month(),
            now.day(),
            contest_name(&task.group),
        );
        let path = path.replace(':', "_");
        fs::create_dir_all(path.clone()).unwrap();
        rust_competitive_helper_util::write_lines(
            format!("{}/{}.rs", path, task_name),
            main.clone(),
        );
        for file in find_additional_solution_files(&task_name) {
            let content = rust_competitive_helper_util::read_lines(format!(
                "tasks/{}/src/{}",
                task_name, file
            ))
            .unwrap();
            rust_competitive_helper_util::write_lines(format!("{}/{}.rs", path, file), content);
        }
        if selection == 3 {
            let tester = rust_competitive_helper_util::read_lines(format!(
                "tasks/{}/src/tester.rs",
                task_name
            ))
            .unwrap();
            main.push("mod tester {".to_string());
            main.extend_from_slice(tester.as_slice());
            main.push("}".to_string());
            let mut test_lines = Vec::new();
            test_lines.push("#![allow(unexpected_cfgs)]".to_string());
            let mut in_main = false;
            for mut line in main {
                if line
                    .trim()
                    .starts_with("let mut paths = std::fs::read_dir(")
                {
                    line = format!(
                        "    let mut paths = std::fs::read_dir(\"./tests/{}/\")",
                        task_name,
                    );
                }
                if line == *"//START MAIN" {
                    in_main = true;
                }
                if !in_main {
                    test_lines.push(line.clone());
                }
                if line == *"//END MAIN" {
                    in_main = false;
                }
            }
            test_lines.push("#[test]".to_string());
            test_lines.push(format!("fn {}() {{", task_name));
            test_lines.push("    assert!(tester::run_tests());".to_string());
            test_lines.push("}".to_string());
            rust_competitive_helper_util::write_lines(
                format!("algo_lib/tests/{}.rs", task_name),
                test_lines,
            );
            let from = format!("tasks/{}/tests", task_name);
            rename(from, format!("algo_lib/tests/{}", task_name)).unwrap();
        }
    }
    remove_dir_all(format!("tasks/{}/", task_name)).unwrap();

    let lines = rust_competitive_helper_util::read_lines("Cargo.toml")
        .unwrap()
        .into_iter()
        .filter(|line| line != &format!("    \"tasks/{}\",", task_name))
        .collect_vec();
    rust_competitive_helper_util::write_lines("Cargo.toml", lines);
}

pub fn archive() {
    let contest_list = contest_list();
    if contest_list.is_empty() {
        eprintln!("No tasks");
        return;
    }
    let contests = contest_list
        .iter()
        .map(|(contest, _)| contest.clone())
        .collect_vec();
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select contest:")
        .default(0)
        .items(contests.as_slice())
        .interact_on_opt(&Term::stdout())
        .unwrap();

    if selection.is_none() {
        return;
    }

    let selection = selection.unwrap();
    let mut tasks = contest_list[selection].1.clone();
    tasks.sort();

    if tasks.len() == 1 {
        let option = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("{}:", tasks[0]))
            .default(2)
            .items(&OPTIONS[..])
            .interact_on(&Term::stdout())
            .unwrap();
        ask_archive(tasks.remove(0), option);
        return;
    }

    let mut selection = vec![2; tasks.len()];

    let mut last = 0;

    loop {
        let id = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Change:")
            .default(last)
            .items(
                once("Done".to_string())
                    .chain(
                        once("Default".to_string()).chain(
                            tasks
                                .iter()
                                .enumerate()
                                .map(|(i, s)| format!("{} ({})", s, OPTIONS[selection[i]])),
                        ),
                    )
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .interact_on(&Term::stdout())
            .unwrap();
        let option = match id {
            0 => break,
            _ => Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select option:")
                .default(if id > 1 {
                    selection[id - 2]
                } else if selection.iter().all(|&x| x == selection[0]) {
                    selection[0]
                } else {
                    2
                })
                .items(&OPTIONS[..])
                .interact_on(&Term::stdout())
                .unwrap(),
        };
        if id == 1 {
            selection.fill(option);
        } else {
            selection[id - 2] = option;
        }
        last = id;
    }

    for (task, opt) in tasks.into_iter().zip(selection) {
        ask_archive(task, opt);
    }
}
