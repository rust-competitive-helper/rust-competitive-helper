use regex::Regex;
use std::time::Duration;

pub fn extract_base(url: &str) -> Option<String> {
    let url = url.split('?').next().unwrap_or(url);
    let url = url.split('#').next().unwrap_or(url);
    let url = url.trim_end_matches('/');
    let dj_re = Regex::new(
        r"^(https?://[^/]+(?:/[^/]+)*?)/(?:team|public|jury|domjudge)/(?:problems|submit|submissions)/[^/]+",
    )
    .ok()?;
    if let Some(caps) = dj_re.captures(url) {
        return Some(caps[1].to_string());
    }
    let host_re = Regex::new(r"^(https?://[^/]+)").ok()?;
    host_re.captures(url).map(|c| c[1].to_string())
}

pub fn is_domjudge(url: &str) -> bool {
    let Some(base) = extract_base(url) else {
        return false;
    };
    let endpoint = format!("{}/api/v4/info", base);
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();
    let body = match agent.get(&endpoint).call().and_then(|r| Ok(r.into_string())) {
        Ok(Ok(b)) => b,
        _ => return false,
    };
    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(j) => j,
        Err(_) => return false,
    };
    json.get("domjudge").is_some()
}

#[cfg(test)]
mod tests {
    use super::extract_base;

    #[test]
    fn extract_base_team_problem() {
        assert_eq!(
            extract_base("https://demo.domjudge.org/team/problems/3").as_deref(),
            Some("https://demo.domjudge.org"),
        );
    }

    #[test]
    fn extract_base_subpath_install() {
        assert_eq!(
            extract_base("https://domjudge.iti.kit.edu/main/team/problems/3").as_deref(),
            Some("https://domjudge.iti.kit.edu/main"),
        );
    }

    #[test]
    fn extract_base_with_query_and_fragment() {
        assert_eq!(
            extract_base("https://j.example.com/public/problems/abc?lang=en#x").as_deref(),
            Some("https://j.example.com"),
        );
    }

    #[test]
    fn extract_base_falls_back_to_host() {
        assert_eq!(
            extract_base("https://judge.example.com/foo/bar").as_deref(),
            Some("https://judge.example.com"),
        );
    }

    #[test]
    fn extract_base_invalid() {
        assert_eq!(extract_base("not a url"), None);
    }
}
