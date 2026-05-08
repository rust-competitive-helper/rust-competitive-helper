use crate::domjudge::{extract_base, is_domjudge, DomjudgeClient};
use rust_competitive_helper_util::{read_lines, Task};
use std::fs;
use std::path::Path;

const LANGUAGE: &str = "Rust";
const MAIN_FILE: &str = "main/src/main.rs";

pub fn print() {
    let url = match read_url(MAIN_FILE) {
        Some(u) => u,
        None => {
            eprintln!("Could not read URL from {}", MAIN_FILE);
            return;
        }
    };
    let Some(base) = extract_base(&url) else {
        eprintln!("Could not extract base URL from {}", url);
        return;
    };
    if !is_domjudge(&url) {
        eprintln!("URL {} does not point to a DOMjudge instance", url);
        return;
    }
    let (task_name, source) = match find_task_source(&url) {
        Some(p) => p,
        None => {
            eprintln!("No task in tasks/ has matching URL {}", url);
            return;
        }
    };
    let mut client = DomjudgeClient::new(&base);
    if let Err(e) = client.ensure_login() {
        eprintln!("Login failed: {}", e);
        return;
    }
    let filename = format!("{}.rs", task_name);
    match client.print(&source, &filename, LANGUAGE) {
        Ok(out) => {
            println!("Print job submitted as {}", filename);
            let trimmed = out.trim();
            if !trimmed.is_empty() {
                println!("{}", trimmed);
            }
        }
        Err(e) => eprintln!("{}", e),
    }
}

fn read_url(path: &str) -> Option<String> {
    let line = read_lines(path).ok()?.into_iter().next()?;
    Some(line.split_at(2).1.trim().to_string())
}

fn find_task_source(url: &str) -> Option<(String, String)> {
    let tasks_dir = Path::new("tasks");
    if !tasks_dir.is_dir() {
        return None;
    }
    for entry in fs::read_dir(tasks_dir).ok()?.flatten() {
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
        if task.url == url {
            let body = lines[1..].join("\n");
            return Some((task_name, body));
        }
    }
    None
}
