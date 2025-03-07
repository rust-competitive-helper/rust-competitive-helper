use crate::submit::{check_available, failure};
use std::process::Command;

pub(crate) fn submit(url: &str) -> bool {
    if !check_available("oj") {
        failure("Please install online judge tools from https://github.com/online-judge-tools/oj");
        return false;
    }
    if !Command::new("oj").args(["login", url]).status().is_ok() {
        return false;
    }
    Command::new("oj")
        .args(["submit", url, "main/src/main.rs", "--yes", "--wait=0"])
        .status()
        .is_ok()
}
