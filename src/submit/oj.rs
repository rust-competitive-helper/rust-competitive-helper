use crate::submit::failure;
use std::process::Command;

pub(crate) fn submit(url: &str) -> bool {
    if Command::new("oj").args(["login", url]).status().is_err() {
        failure("Online judge tools run was not successful. If it is not installed please install online judge tools from https://github.com/online-judge-tools/oj");
        return false;
    }
    Command::new("oj")
        .args(["submit", url, "main/src/main.rs", "--yes", "--wait=0"])
        .status()
        .is_ok()
}
