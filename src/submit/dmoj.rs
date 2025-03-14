use crate::submit::failure;
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
    match Command::new("dmoj-submit")
        .args([
            "submit",
            "--problem",
            problem_id.as_str(),
            "--language",
            "rust",
            "main/src/main.rs",
        ])
        .status()
    {
        Ok(_) => true,
        Err(_) => {
            failure("dmoj-submit run was not successful. If it is not installed please install dmoj-submit from https://github.com/nils-emmenegger/dmoj-submit");
            false
        }
    }
}
