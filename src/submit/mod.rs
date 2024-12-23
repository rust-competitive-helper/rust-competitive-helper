mod dmoj;
mod kattis;
mod oj;
mod submitter;

use clipboard::{ClipboardContext, ClipboardProvider};
use crossterm::execute;
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use regex::Regex;
use rust_competitive_helper_util::{read_from_file, read_lines};
use std::process::Command;

pub fn submit() {
    let file = "main/src/main.rs";
    let url = read_lines(file)
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .split_at(2)
        .1
        .trim()
        .to_string();
    let url_regex = Regex::new(r"https?://(?:www\.)?([^/]+).*").unwrap();
    let site = {
        match url_regex.captures(&url) {
            None => String::new(),
            Some(caps) => caps[1].to_string(),
        }
    };
    match site.as_str() {
        "atcoder.jp" | "codeforces.com" | "codechef.com" | "contest.yandex.com"
        | "contest.ucup.ac" | "luogu.com.cn" | "toph.co" => {
            submitter::submit(&url);
        }
        "hackerrank.com" | "yukicoder.me" => {
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
            ctx.set_contents(read_from_file("main/src/main.rs").unwrap())
                .unwrap();
        }
    }
}

fn failure(s: &str) {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Red));
    println!("{s}");
    let _ = execute!(stdout, ResetColor);
}

fn check_available(name: &str) -> bool {
    let which_output = Command::new("which").arg(name).output().unwrap();
    assert!(which_output.status.success());
    !String::from_utf8_lossy(&which_output.stdout).starts_with(&format!("which: no {} in", name))
}
