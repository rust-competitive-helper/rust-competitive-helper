use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use dialoguer::console::Term;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Password};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;

const COOKIE_FILE: &str = ".dj_cookies.json";

#[derive(Serialize, Deserialize, Default)]
struct CookieStore {
    hosts: HashMap<String, Credentials>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Credentials {
    user: String,
    pass: String,
}

impl CookieStore {
    fn load() -> Self {
        fs::read_to_string(COOKIE_FILE)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            if let Err(e) = fs::write(COOKIE_FILE, s) {
                eprintln!("Warning: failed to save {}: {}", COOKIE_FILE, e);
            }
        }
    }
}

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

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(timeout).build()
}

pub fn is_domjudge(url: &str) -> bool {
    let Some(base) = extract_base(url) else {
        return false;
    };
    let endpoint = format!("{}/api/v4/info", base);
    let resp = match agent(Duration::from_secs(5)).get(&endpoint).call() {
        Ok(r) => r,
        Err(_) => return false,
    };
    let body = match resp.into_string() {
        Ok(b) => b,
        Err(_) => return false,
    };
    serde_json::from_str::<serde_json::Value>(&body)
        .map(|j| j.get("domjudge").is_some())
        .unwrap_or(false)
}

fn basic_auth_header(user: &str, pass: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{}:{}", user, pass)))
}

pub struct DomjudgeClient {
    base: String,
    agent: ureq::Agent,
    auth: Option<String>,
}

impl DomjudgeClient {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent: agent(Duration::from_secs(30)),
            auth: None,
        }
    }

    fn check_credentials(&mut self, user: &str, pass: &str) -> Result<(), String> {
        let header = basic_auth_header(user, pass);
        let endpoint = format!("{}/api/v4/user", self.base);
        match self.agent.get(&endpoint).set("Authorization", &header).call() {
            Ok(_) => {
                self.auth = Some(header);
                Ok(())
            }
            Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
                Err("invalid username or password".to_string())
            }
            Err(ureq::Error::Status(code, _)) => {
                Err(format!("auth check failed (HTTP {})", code))
            }
            Err(e) => Err(format!("auth check failed: {}", e)),
        }
    }

    pub fn ensure_login(&mut self) -> Result<(), String> {
        let mut store = CookieStore::load();
        if let Some(creds) = store.hosts.get(&self.base).cloned() {
            if self.check_credentials(&creds.user, &creds.pass).is_ok() {
                return Ok(());
            }
            eprintln!("Stored credentials for {} no longer work; re-authenticating.", self.base);
        }
        let user: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Username for {}", self.base))
            .interact_on(&Term::stdout())
            .map_err(|e| format!("input failed: {}", e))?;
        let pass: String = Password::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Password for {}", self.base))
            .interact_on(&Term::stdout())
            .map_err(|e| format!("input failed: {}", e))?;
        self.check_credentials(&user, &pass)?;
        store.hosts.insert(self.base.clone(), Credentials { user, pass });
        store.save();
        Ok(())
    }

    pub fn print(
        &mut self,
        source: &str,
        filename: &str,
        language: &str,
    ) -> Result<String, String> {
        let auth = self.auth.clone().ok_or_else(|| "not logged in".to_string())?;
        let endpoint = format!("{}/api/v4/printing/team", self.base);
        let body = serde_json::json!({
            "language": language,
            "fileContents": STANDARD.encode(source),
            "originalName": filename,
        });
        let resp = self
            .agent
            .post(&endpoint)
            .set("Authorization", &auth)
            .set("Content-Type", "application/json")
            .send_string(&body.to_string());
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                let text = r.into_string().unwrap_or_default();
                return Err(format!("print failed (HTTP {}): {}", code, text));
            }
            Err(e) => return Err(format!("print request failed: {}", e)),
        };
        let body = resp
            .into_string()
            .map_err(|e| format!("failed to read print response: {}", e))?;
        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| format!("invalid print response: {}: {}", e, body))?;
        let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        let output = json
            .get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !success {
            return Err(format!("print failed: {}", output));
        }
        Ok(output)
    }
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
