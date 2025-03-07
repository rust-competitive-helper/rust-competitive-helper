use crate::submit::{check_available, failure};
use regex::Regex;
use std::process::Command;

pub(crate) fn submit(url: &str) -> bool {
    let url_regex = Regex::new(r".*/problem/(\w+)([?].*)?").unwrap();
    let problem_id = {
        match url_regex.captures(url) {
            None => {
                failure("Unexpected URL for dmoj problem");
                return false;
            }
            Some(caps) => caps[1].to_string(),
        }
    };
    if !check_available("dmoj-submit") {
        failure("Please install dmoj-submit from https://github.com/nils-emmenegger/dmoj-submit");
        return false;
    }
    Command::new("dmoj-submit")
        .args([
            "submit",
            "--problem",
            problem_id.as_str(),
            "--language",
            "rust",
            "main/src/main.rs",
        ])
        .status()
        .is_ok()
}
