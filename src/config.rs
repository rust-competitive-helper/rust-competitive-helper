use itertools::Itertools;
use std::{collections::HashMap, process::Command};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    open_task_command: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        let open_task_command = if cfg!(windows) {
            [
                "..\\clion.cmd",
                "--line",
                "$LINE",
                "--column",
                "$COLUMN",
                "$FILE",
            ]
            .map(|s| s.to_owned())
            .to_vec()
        } else {
            let clion_path = std::env::var("HOME").unwrap() + "/.local/bin/clion";
            [
                &clion_path,
                "--line",
                "$LINE",
                "--column",
                "$COLUMN",
                "$FILE",
            ]
            .map(|s| s.to_owned())
            .to_vec()
        };
        Self { open_task_command }
    }
}

impl Config {
    ///
    /// Default config locations by confy:
    ///
    /// Linux:   /home/alice/.config/barapp
    ///
    /// Windows: C:\Users\Alice\AppData\Roaming\Foo Corp\Bar App
    ///
    /// macOS:   /Users/Alice/Library/Preferences/com.Foo-Corp.Bar-App
    ///
    ///
    pub fn load() -> Self {
        confy::load("rust-competitive-helper")
            .expect("Can't load config for rust-competitive-helper")
    }

    pub fn run_open_task_command(
        &self,
        template_args: &HashMap<String, String>,
    ) -> Result<std::process::Output, String> {
        let terms: Vec<_> = self
            .open_task_command
            .iter()
            .map(|s| {
                let mut s = s.clone();
                for (key, value) in template_args.iter() {
                    s = s.replace(key, value);
                }
                s
            })
            .collect();
        Command::new(&terms[0])
            .args(&terms[1..])
            .output()
            .map_err(|err| {
                format!(
                    "'{}': check config file", // TODO provide path for config file, confy-0.5 will have get_configuration_file_path
                    match err.kind() {
                        std::io::ErrorKind::NotFound => format!("{} not found", &terms[0]),
                        _ => format!(
                            "Couldn't run command '{}': {}",
                            terms.iter().map(|arg| format!("\"{}\"", arg)).join(" "),
                            err
                        ),
                    }
                )
            })
    }
}
