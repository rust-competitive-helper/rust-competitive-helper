use std::process::Command;
use regex::Regex;
use crate::submit::{check_available, failure};

pub(crate) fn submit(url: &str) {
    let url_regex = Regex::new(r".*/(\w+)([?].*)?").unwrap();
    let problem_id = {
        match url_regex.captures(url) {
            None => {
                failure("Unexpected URL for kattis problem");
                return;
            }
            Some(caps) => caps[1].to_string(),
        }
    };
    if !check_available("kattis") {
        failure("Please install kattis-cli from https://github.com/Kattis/kattis-cli");
        return;
    }
    Command::new("kattis").args(&["main/src/main.rs", "-p", problem_id.as_str(), "-f"]).status().unwrap();
}
