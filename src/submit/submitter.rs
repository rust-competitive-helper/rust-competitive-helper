use crate::submit::{check_available, failure};
use std::process::Command;

pub(crate) fn submit(url: &str) {
    if !check_available("submitter") {
        failure("Please install submitter from https://github.com/EgorKulikov/submitter");
        return;
    }
    Command::new("submitter")
        .args([url, "rust", "main/src/main.rs"])
        .status()
        .unwrap();
}
