use crate::config::Config;
use rust_competitive_helper_util::{read_lines, Task};
use std::path::Path;
use std::process::Command;

const MAIN_FILE: &str = "main/src/main.rs";

pub fn print() {
    let config = Config::load();
    let Some(server) = config.domjudge_server.as_ref() else {
        eprintln!("domjudge_server not set in config.toml");
        return;
    };
    let Some(name) = read_first_line_comment(MAIN_FILE) else {
        eprintln!("Could not read problem name from {}", MAIN_FILE);
        return;
    };
    let Some(letter) = name.chars().find(|c| c.is_ascii_alphabetic()) else {
        eprintln!("No letter in problem name: '{}'", name);
        return;
    };
    let Some(task_name) = find_task_dir(&name) else {
        eprintln!("No task in tasks/ matches '{}'", name);
        return;
    };
    let url = format!(
        "{}/{}",
        server.trim_end_matches('/'),
        letter.to_ascii_uppercase(),
    );
    let file = format!("tasks/{}/src/main.rs", task_name);
    match Command::new("submitter").args(["print", &url, &file]).status() {
        Ok(_) => {}
        Err(e) => eprintln!("Failed to run submitter: {}", e),
    }
}

fn read_first_line_comment(path: &str) -> Option<String> {
    let line = read_lines(path).ok()?.into_iter().next()?;
    Some(line.split_at(2).1.trim().to_string())
}

fn find_task_dir(name: &str) -> Option<String> {
    let tasks_dir = Path::new("tasks");
    if !tasks_dir.is_dir() {
        return None;
    }
    for entry in std::fs::read_dir(tasks_dir).ok()?.flatten() {
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        let task_name = entry.file_name().to_string_lossy().into_owned();
        let main_path = format!("tasks/{}/src/main.rs", task_name);
        if !Path::new(&main_path).is_file() {
            continue;
        }
        let lines = match read_lines(&main_path) {
            Ok(l) => l,
            Err(_) => continue,
        };
        let Some(first) = lines.first() else { continue };
        let json_str = first.strip_prefix("//").unwrap_or("").trim();
        let task: Task = match serde_json::from_str(json_str) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if task.name == name {
            return Some(task_name);
        }
    }
    None
}
