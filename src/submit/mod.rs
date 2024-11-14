mod codeforces;
mod oj;
mod kattis;
mod dmoj;

use std::process::Command;
use clipboard::{ClipboardContext, ClipboardProvider};
use crossterm::execute;
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use rust_competitive_helper_util::{read_from_file, read_lines};

pub fn submit() {
    let file = "main/src/main.rs";
    let url = read_lines(file).into_iter().next().unwrap().split_at(2).1.trim().to_string();
    let site = url.split('/').nth(2).unwrap_or("Manual");
    match site {
        "codeforces.com" => {
            codeforces::submit(&url);
        }
        "atcoder.jp" | "www.hackerrank.com" | "yukicoder.me" => {
            oj::submit(&url);
        }
        "open.kattis.com" => {
            kattis::submit(&url);
        }
        "dmoj.ca" => {
            dmoj::submit(&url);
        }
        _ => {
            println!("Unsupported site, code copied to clipboard: {}", site);
            let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
            ctx.set_contents(read_from_file("main/src/main.rs")).unwrap();
        }
    }
}

fn success(s: &str) -> usize {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Green));
    println!("{s}");
    let _ = execute!(stdout, ResetColor);
    s.len()
}

fn failure(s: &str) {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Red));
    println!("{s}");
    let _ = execute!(stdout, ResetColor);
}

fn pending(s: &str) -> usize {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Yellow));
    print!("{s}");
    let _ = execute!(stdout, ResetColor);
    s.len()
}

fn check_available(name: &str) -> bool {
    let which_output = Command::new("which").arg(name).output().unwrap();
    assert!(which_output.status.success());
    !String::from_utf8_lossy(&which_output.stdout).starts_with(&format!("which: no {} in", name))
}
