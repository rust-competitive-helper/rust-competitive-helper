mod codeforces;

use rust_competitive_helper_util::read_lines;

pub fn submit() {
    let file = "main/src/main.rs";
    let url = read_lines(file).into_iter().next().unwrap().split_at(2).1.trim().to_string();
    let site = url.split('/').nth(2).unwrap();
    match site {
        "codeforces.com" => {
            codeforces::submit(&url);
        }
        _ => {
            println!("Unsupported site: {}", site);
        }
    }
}
