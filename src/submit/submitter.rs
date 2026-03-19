use crate::submit::failure;
use std::process::Command;

pub(crate) fn submit(url: &str) -> bool {
    match Command::new("submitter")
        .args([url, "rust", "main/src/main.rs"])
        .status()
    {
        Ok(_) => true,
        Err(_) => {
            failure("Submitter run was not successful. If it is not installed please install submitter from https://github.com/EgorKulikov/submitter");
            false
        }
    }
}
