mod submitter;

use crate::config::Config;
use clipboard::{ClipboardContext, ClipboardProvider};
use crossterm::execute;
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use rust_competitive_helper_util::{read_from_file, read_lines};

const MAIN_FILE: &str = "main/src/main.rs";

pub fn submit() {
    let config = Config::load();
    let Some(server) = config.domjudge_server.as_ref() else {
        failure("domjudge_server not set in config.toml");
        return;
    };
    let Some(name) = read_first_line_comment(MAIN_FILE) else {
        failure(&format!("Could not read problem name from {}", MAIN_FILE));
        return;
    };
    let Some(letter) = name.chars().find(|c| c.is_ascii_alphabetic()) else {
        failure(&format!("No letter in problem name: '{}'", name));
        return;
    };
    let url = format!(
        "{}/{}",
        server.trim_end_matches('/'),
        letter.to_ascii_uppercase(),
    );
    if !submitter::submit(&url) {
        failure("Submitter failed; copying source to clipboard");
        copy_to_clipboard();
    }
}

fn read_first_line_comment(file: &str) -> Option<String> {
    let line = read_lines(file).ok()?.into_iter().next()?;
    Some(line.split_at(2).1.trim().to_string())
}

fn copy_to_clipboard() {
    if let (Ok(mut ctx), Some(content)) = (
        <ClipboardContext as ClipboardProvider>::new(),
        read_from_file(MAIN_FILE),
    ) {
        let _ = ctx.set_contents(content);
    }
}

fn failure(s: &str) {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Red));
    println!("{s}");
    let _ = execute!(stdout, ResetColor);
}
