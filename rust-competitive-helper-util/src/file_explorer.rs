use std::collections::{HashMap, HashSet};

use crate::read_lines;

pub trait FileExplorer {
    // TODO: add error handling
    // TODO: path - not string?
    fn read_file(&self, filename: &str) -> Result<Vec<String>, String>;

    // returns list of relative paths
    // [path_prefix] should end with '/'
    fn get_all_rs_files(&self, path_prefix: &str) -> Vec<String>;

    fn file_exists(&self, path: &str) -> bool;
}

pub struct RealFileExplorer {}

impl RealFileExplorer {
    pub fn new() -> Self {
        Self {}
    }

    #[allow(clippy::only_used_in_recursion)]
    fn get_all_rs_files_impl(
        &self,
        path_prefix: &str,
        cur_dir: &str,
        result: &mut HashSet<String>,
    ) {
        let paths: Vec<_> = std::fs::read_dir(format!("{}/{}", path_prefix, cur_dir))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for path in paths {
            if path.file_type().unwrap().is_file() {
                let filename = path.file_name();
                let filename = filename.to_str().unwrap();
                if filename.ends_with(".rs") {
                    // TODO: cur_dir == empty?
                    result.insert(format!("{}{}", cur_dir, filename));
                }
            } else if path.file_type().unwrap().is_dir() {
                self.get_all_rs_files_impl(
                    path_prefix,
                    &format!("{}{}/", cur_dir, path.file_name().to_str().unwrap()),
                    result,
                );
            }
        }
    }
}

impl FileExplorer for RealFileExplorer {
    fn read_file(&self, filename: &str) -> Result<Vec<String>, String> {
        read_lines(filename)
    }

    fn get_all_rs_files(&self, path_prefix: &str) -> Vec<String> {
        let mut result = HashSet::new();
        self.get_all_rs_files_impl(path_prefix, "", &mut result);
        result.into_iter().collect()
    }

    fn file_exists(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
}

pub struct FakeFileExplorer {
    files: HashMap<String, Vec<String>>,
}

impl FileExplorer for FakeFileExplorer {
    fn read_file(&self, filename: &str) -> Result<Vec<String>, String> {
        Ok(self
            .files
            .get(filename)
            .unwrap_or_else(|| panic!("Can't open file: {}", &filename))
            .clone())
    }

    fn get_all_rs_files(&self, path_prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter_map(|name| name.strip_prefix(path_prefix))
            .map(str::to_string)
            .collect()
    }

    fn file_exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }
}

#[cfg(test)]
impl FakeFileExplorer {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    pub fn add_file(&mut self, filename: &str, content: &str) {
        self.files.insert(
            filename.to_owned(),
            content.lines().map(|s| s.to_string()).collect(),
        );
    }
}
