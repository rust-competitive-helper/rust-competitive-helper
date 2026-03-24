use itertools::Itertools;
use std::path::Path;
use std::{collections::HashMap, fs, process::Command};

use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "config.toml";

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    open_task_command: Vec<String>,
    pub input_file_extension: String,
    pub output_file_extension: String,
}

impl Default for Config {
    fn default() -> Self {
        let open_task_command = if cfg!(windows) {
            [
                "rustrover.cmd",
                "--line",
                "$LINE",
                "--column",
                "$COLUMN",
                "$FILE",
            ]
            .map(|s| s.to_owned())
            .to_vec()
        } else {
            let clion_path = std::env::var("HOME").unwrap() + "/.local/bin/rustrover";
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
        Self {
            open_task_command,
            input_file_extension: ".in".to_string(),
            output_file_extension: ".out".to_string(),
        }
    }
}

fn global_config_path() -> Option<String> {
    if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(|p| format!("{}/rust-competitive-helper/default-config.toml", p))
    } else {
        std::env::var("HOME")
            .ok()
            .map(|p| format!("{}/.config/rust-competitive-helper/default-config.toml", p))
    }
}

impl Config {
    pub fn load() -> Self {
        if Path::new(CONFIG_FILE).exists() {
            let content =
                fs::read_to_string(CONFIG_FILE).expect("Can't read config.toml");
            Config::from_toml(&content)
        } else {
            // Try to migrate from global confy config
            let config = match global_config_path() {
                Some(path) if Path::new(&path).exists() => {
                    let content =
                        fs::read_to_string(&path).expect("Can't read global config");
                    Config::from_toml(&content)
                }
                _ => Config::default(),
            };
            fs::write(CONFIG_FILE, config.to_toml()).expect("Can't write config.toml");
            config
        }
    }

    pub fn from_toml(content: &str) -> Self {
        toml::from_str(content).expect("Can't parse config")
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("Can't serialize config")
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
                    "'{}': check config.toml",
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

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn test_roundtrip() {
        let config = Config::default();
        let toml = config.to_toml();
        let parsed = Config::from_toml(&toml);
        assert_eq!(parsed.input_file_extension, ".in");
        assert_eq!(parsed.output_file_extension, ".out");
    }

    #[test]
    fn test_parse_custom_extensions() {
        let toml = r#"
open_task_command = ["echo"]
input_file_extension = ".input"
output_file_extension = ".answer"
"#;
        let config = Config::from_toml(toml);
        assert_eq!(config.input_file_extension, ".input");
        assert_eq!(config.output_file_extension, ".answer");
    }

    #[test]
    fn test_parse_old_config_without_extensions() {
        // Old confy configs won't have the new fields - deserialization will fail
        let toml = r#"
open_task_command = ["rustrover", "--line", "$LINE", "--column", "$COLUMN", "$FILE"]
"#;
        let result: Result<Config, _> = toml::from_str(toml);
        assert!(result.is_err());
    }
}
