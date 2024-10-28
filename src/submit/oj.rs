use std::process::Command;
use crate::submit::{check_available, failure};

pub(crate) fn submit(url: &str) {
    if !check_available("oj") {
        failure("Please install online judge tools from https://github.com/online-judge-tools/oj");
        return;
    }
    Command::new("oj").args(&["login", url]).status().unwrap();
    Command::new("oj").args(&["submit", url, "main/src/main.rs", "--yes", "--wait=0"]).status().unwrap();
}
