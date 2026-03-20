mod dmoj;
mod oj;
mod submitter;

use clipboard::{ClipboardContext, ClipboardProvider};
use crossterm::execute;
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use regex::Regex;
use rust_competitive_helper_util::{read_from_file, read_lines};

fn extract_site(url: &str) -> String {
    let url_regex = Regex::new(r"https?://(?:www\.)?([^/]+)").unwrap();
    match url_regex.captures(url) {
        None => String::new(),
        Some(caps) => {
            let host = &caps[1];
            let parts: Vec<&str> = host.split('.').collect();
            // Skip TLD parts from the end to find the second-level domain
            // e.g. "codeforces.com" -> "codeforces", "contest.yandex.com" -> "yandex"
            //      "luogu.com.cn" -> "luogu"
            let tlds = [
                "com", "org", "net", "co", "ac", "me", "ca", "cn", "ru", "jp",
            ];
            let sld = parts
                .iter()
                .rev()
                .find(|p| !tlds.contains(p))
                .unwrap_or(&"");
            sld.to_string()
        }
    }
}

pub fn submit() {
    let file = "main/src/main.rs";
    let url = read_lines(file)
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .split_at(2)
        .1
        .trim()
        .to_string();
    let site = extract_site(&url);
    let quoted_url = if url
        .contains(|c: char| !c.is_ascii_alphanumeric() && !"-._~:/?[]@!$'+,;=%".contains(c))
    {
        format!("\"{}\"", url)
    } else {
        url
    };
    let submitted = match site.as_str() {
        "codeforces" | "codechef" | "ucup" | "eolymp" | "toph" | "yandex" | "uoj" | "kattis"
        | "atcoder" => submitter::submit(&quoted_url),
        "hackerrank" | "yukicoder" => oj::submit(&quoted_url),
        "dmoj" => dmoj::submit(&quoted_url),
        _ => false,
    };
    if !submitted {
        println!(
            "Unsupported site or error submitting, code copied to clipboard: {}",
            site
        );
        let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
        ctx.set_contents(read_from_file("main/src/main.rs").unwrap())
            .unwrap();
    }
}

fn failure(s: &str) {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Red));
    println!("{s}");
    let _ = execute!(stdout, ResetColor);
}

#[cfg(test)]
mod tests {
    use super::extract_site;

    #[test]
    fn test_extract_site() {
        assert_eq!(
            extract_site("https://codeforces.com/contest/123"),
            "codeforces"
        );
        assert_eq!(extract_site("https://contest.yandex.com/foo"), "yandex");
        assert_eq!(
            extract_site("https://open.kattis.com/problems/abc"),
            "kattis"
        );
        assert_eq!(extract_site("https://dmoj.ca/problem/abc"), "dmoj");
        assert_eq!(extract_site("https://codechef.com/abc"), "codechef");
        assert_eq!(extract_site("https://contest.ucup.ac/foo"), "ucup");
        assert_eq!(extract_site("https://eolymp.com/foo"), "eolymp");
        assert_eq!(extract_site("https://toph.co/foo"), "toph");
        assert_eq!(extract_site("https://hackerrank.com/foo"), "hackerrank");
        assert_eq!(extract_site("https://yukicoder.me/foo"), "yukicoder");
        assert_eq!(extract_site("https://www.luogu.com.cn/foo"), "luogu");
        assert_eq!(extract_site("https://uoj.ac/foo"), "uoj");
    }

    #[test]
    fn test_extract_site_no_path() {
        assert_eq!(extract_site("https://codeforces.com"), "codeforces");
        assert_eq!(extract_site("https://dmoj.ca"), "dmoj");
    }

    #[test]
    fn test_extract_site_invalid() {
        assert_eq!(extract_site("not a url"), "");
    }
}
