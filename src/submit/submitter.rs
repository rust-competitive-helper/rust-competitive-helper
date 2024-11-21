use std::process::Command;

pub(crate) fn submit(url: &str) {
    Command::new("submitter").args([url, "rust", "main/src/main.rs"]).status().unwrap();
}
