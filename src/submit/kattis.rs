use crate::submit::failure;
use regex::Regex;
use std::process::Command;

pub(crate) fn submit(url: &str) -> bool {
    let url_regex = Regex::new(r".*/(\w+)([?].*)?").unwrap();
    let problem_id = {
        match url_regex.captures(url) {
            None => {
                failure("Unexpected URL for kattis problem");
                return false;
            }
            Some(caps) => caps[1].to_string(),
        }
    };
    match Command::new("kattis")
        .args(["main/src/main.rs", "-p", problem_id.as_str(), "-f"])
        .status()
    {
        Ok(_) => true,
        Err(_) => {
            failure("kattis-cli run was not successful. If it is not installed please install kattis-cli from https://github.com/Kattis/kattis-cli");
            false
        }
    }
}
