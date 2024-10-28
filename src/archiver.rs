use chrono::{Datelike, Utc};
use dialoguer::console::Term;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;
use itertools::Itertools;
use rust_competitive_helper_util::Task;
use std::collections::BTreeMap;
use std::fs;
use std::fs::{remove_dir_all, rename};
use std::io::{BufRead, BufReader};
use std::iter::once;

fn contest_name(group: &str) -> String {
    match group.find('-') {
        None => group.to_string(),
        Some(at) => group.split_at(at + 1).1.trim().to_string(),
    }
}

fn contest_list() -> Vec<(String, Vec<String>)> {
    let lines = rust_competitive_helper_util::read_lines("Cargo.toml");
    let mut result = BTreeMap::new();
    for line in lines {
        if !line.starts_with("    ") {
            continue;
        }
        let line = line.trim().as_bytes();
        let task_name = String::from_utf8_lossy(&line[1..line.len() - 2]).to_string();
        let main = fs::File::open(format!("{}/src/main.rs", task_name));
        if main.is_err() {
            continue;
        }
        let main = main.unwrap();
        let mut content = BufReader::new(main).lines();
        let first_line = content.next();
        if first_line.is_none() {
            continue;
        }
        let first_line = first_line.unwrap().unwrap();
        if !first_line.starts_with("//") {
            continue;
        }
        let json = first_line.chars().skip(2).collect::<String>();
        if let Ok(task) = serde_json::from_str::<Task>(json.as_str()) {
            let contest_name = contest_name(&task.group);
            if !result.contains_key(&contest_name) {
                result.insert(contest_name.clone(), Vec::new());
            }
            result.get_mut(&contest_name).unwrap().push(task_name);
        }
    }
    result.into_iter().collect()
}

const OPTIONS: [&str; 4] = ["Skip", "Delete", "Archive only", "Archive and tests"];

fn find_additional_solution_files(task_name: &str) -> Vec<String> {
    rust_competitive_helper_util::all_rs_files_in_dir(format!("{}/src", task_name))
        .into_iter()
        .filter(|file| file != "main.rs" && file != "tester.rs")
        .collect()
}

fn ask_archive(task_name: String, selection: usize) {
    if selection == 0 {
        return;
    }
    if selection >= 2 {
        let now = Utc::now();
        let mut main =
            rust_competitive_helper_util::read_lines(format!("{}/src/main.rs", task_name));
        let task =
            serde_json::from_str::<Task>(main[0].chars().skip(2).collect::<String>().as_str())
                .unwrap();
        let path = format!(
            "archive/{}/{:02}/{:02}.{:02}.{} - {}",
            now.year(),
            now.month(),
            now.day(),
            now.month(),
            now.year(),
            contest_name(&task.group),
        );
        let path = path.replace(':', "_");
        fs::create_dir_all(path.clone()).unwrap();
        rust_competitive_helper_util::write_lines(
            format!("{}/{}.rs", path, task_name),
            main.clone(),
        );
        for file in find_additional_solution_files(&task_name) {
            let content =
                rust_competitive_helper_util::read_lines(format!("{}/src/{}", task_name, file));
            rust_competitive_helper_util::write_lines(format!("{}/{}.rs", path, file), content);
        }
        if selection == 3 {
            let tester =
                rust_competitive_helper_util::read_lines(format!("{}/src/tester.rs", task_name));
            main.push("mod tester {".to_string());
            main.extend_from_slice(tester.as_slice());
            main.push("}".to_string());
            let mut test_lines = Vec::new();
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
            let from = format!("{}/tests", task_name);
            rename(from, format!("algo_lib/tests/{}", task_name)).unwrap();
        }
    }
    remove_dir_all(format!("{}/", task_name)).unwrap();

    let lines = rust_competitive_helper_util::read_lines("Cargo.toml")
        .into_iter()
        .filter(|line| line != &format!("    \"{}\",", task_name))
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

    let mut selection = vec![2; tasks.len()];

    let mut last = 0;

    loop {
        let id = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Change:")
            .default(last)
            .items(once("Done".to_string()).chain(once("Default".to_string()).chain(tasks.iter().enumerate().map(|(i, s)| format!("{} ({})", s, OPTIONS[selection[i]])))).collect::<Vec<_>>().as_slice())
            .interact_on(&Term::stdout())
            .unwrap();
        let option = match id {
            0 => break,
            _ => {
                Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("Select option:")
                    .default(if id > 1 { selection[id - 2] } else {
                        if selection.iter().all(|&x| x == selection[0]) { selection[0] } else { 2 }
                    })
                    .items(&OPTIONS[..])
                    .interact_on(&Term::stdout())
                    .unwrap()
            }
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
