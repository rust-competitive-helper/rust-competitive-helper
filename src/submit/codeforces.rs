use crossterm::execute;
use crossterm::style::Color;
use crossterm::style::ResetColor;
use crossterm::style::SetForegroundColor;
use regex::Regex;

// Url should be https://codeforces.com/$contest_type/$contest_id/problem/$problem_id
pub(crate) fn submit(url: &str) {
    let url_regex = Regex::new(r"https://codeforces.com/(\w+)/(\d+)/problem/(\w+)").unwrap();
    let (contest_type, contest_id, problem_id) = {
        match url_regex.captures(url) {
            None => {
                failure("Unexpected URL for codeforces problem");
                return;
            }
            Some(caps) => (
                caps[1].to_string(),
                caps[2].to_string().parse::<u32>().unwrap(),
                caps[3].to_string(),
            ),
        }
    };
    let mut client = client::WebClient::new();
    if client.login().is_err() {
        return;
    }
    if let Ok(id) = client.submit(contest_id, &problem_id, &contest_type) {
        let mut last_len = 0;
        loop {
            if let Ok(body) = client.get_url(&format!(
                "https://codeforces.com/{}/{}/submission/{}",
                contest_type, contest_id, id
            )) {
                let outcome =
                    Regex::new(r"<span class='verdict-(\w*)'>(([^<]|<s)*)</span>").unwrap();
                let caps = outcome.captures(&body).unwrap();
                let outcome_type = caps[1].to_string();
                let outcome = caps[2]
                    .to_string()
                    .replace("<span class=\"verdict-format-judged\">", "");
                for _ in 0..last_len {
                    print!("{}", 8u8 as char);
                }
                for _ in 0..last_len {
                    print!(" ");
                }
                for _ in 0..last_len {
                    print!("{}", 8u8 as char);
                }
                last_len = match outcome_type.as_str() {
                    "waiting" => pending(&outcome.to_string()),
                    "accepted" => {
                        success(&outcome.to_string());
                        return;
                    }
                    "rejected" => {
                        failure(&outcome.to_string());
                        return;
                    }
                    _ => {
                        failure(&format!("Unknown: {}", outcome));
                        return;
                    }
                };
                std::thread::sleep(std::time::Duration::from_secs(1));
            } else {
                return;
            }
        }
    }
}

fn success(s: &str) -> usize {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Green));
    println!("{s}");
    let _ = execute!(stdout, ResetColor);
    s.len()
}

fn failure(s: &str) {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Red));
    println!("{s}");
    let _ = execute!(stdout, ResetColor);
}

fn pending(s: &str) -> usize {
    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, SetForegroundColor(Color::Yellow));
    print!("{s}");
    let _ = execute!(stdout, ResetColor);
    s.len()
}

// This was mostly written by woshiluo, I just fixed protocol due to codeforces changes
mod client {
    use crate::submit::codeforces::{failure, success};
    use dialoguer::console::Term;
    use dialoguer::theme::ColorfulTheme;
    use dialoguer::{Input, Password};
    use regex::Regex;
    use std::fmt;
    use std::fs::{create_dir_all, File};
    use std::io::Read;
    use std::sync::Arc;

    fn session_file() -> String {
        std::env::var("HOME").unwrap() + "/.config/rust-competitive-helper/codeforces.session"
    }

    pub struct WebClient {
        client: reqwest::blocking::Client,
        cookies: Arc<reqwest_cookie_store::CookieStoreMutex>,
        has_rcpc: bool,
        logged_in: bool,
    }

    impl Drop for WebClient {
        fn drop(&mut self) {
            create_dir_all(session_file().rsplit_once('/').unwrap().0).unwrap();
            let mut file = File::create(session_file())
                .map(std::io::BufWriter::new)
                .unwrap();
            let cookies = self.cookies.lock().unwrap();
            cookies.save_json(&mut file).unwrap();
        }
    }

    const PARMA_BFAA: &str = "f1b3f18c715565b589b7823cda7448ce";

    impl WebClient {
        pub fn new() -> WebClient {
            let cookie_store = {
                let store = File::open(session_file());
                let file = store.map(std::io::BufReader::new);
                match file {
                    Ok(file) => cookie_store::CookieStore::load_json(file).unwrap(),
                    _ => cookie_store::CookieStore::default(),
                }
            };
            let jar = Arc::from(reqwest_cookie_store::CookieStoreMutex::new(cookie_store));

            let client = reqwest::blocking::Client::builder()
                .cookie_store(true)
                .cookie_provider(Arc::clone(&jar))
                .build()
                .unwrap();

            WebClient {
                client,
                cookies: jar,
                has_rcpc: false,
                logged_in: false,
            }
        }

        fn set_rcpc(&mut self) -> Result<(), CFToolError> {
            if self.has_rcpc {
                return Ok(());
            };

            self.has_rcpc = true;

            use aes::cipher::{block_padding::ZeroPadding, BlockDecryptMut, KeyIvInit};

            let body = get_url("https://codeforces.com")?;

            // There is no rcpc.
            if !body.contains("Redirecting") {
                return Ok(());
            }

            // User Regex to get aes triple
            let number_regex = Regex::new(r#"toNumbers\("(.+?)"\)"#).unwrap();
            let caps = number_regex.captures_iter(&body);
            let caps: Vec<String> = caps.map(|cap| cap[1].to_string()).collect();

            let mut text: [u8; 16] = hex::decode(&caps[2])
                .map_err(|_| CFToolError::FailedParseRespone)?
                .try_into()
                .map_err(|_| CFToolError::FailedParseRespone)?;
            let key: [u8; 16] = hex::decode(&caps[0])
                .map_err(|_| CFToolError::FailedParseRespone)?
                .try_into()
                .map_err(|_| CFToolError::FailedParseRespone)?;
            let iv: [u8; 16] = hex::decode(&caps[1])
                .map_err(|_| CFToolError::FailedParseRespone)?
                .try_into()
                .map_err(|_| CFToolError::FailedParseRespone)?;

            // Decrypt
            type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
            let pt = Aes128CbcDec::new(&key.into(), &iv.into())
                .decrypt_padded_mut::<ZeroPadding>(&mut text)
                .map_err(|_| CFToolError::FailedParseRespone)?;

            // Set rcpc
            {
                let mut cookies = self.cookies.lock().unwrap();
                cookies
                    .parse(
                        &format!("RCPC={}", hex::encode(pt)),
                        &"https://codeforces.com".parse::<reqwest::Url>().unwrap(),
                    )
                    .map_err(|_| CFToolError::FailedRequest)?;
            }

            Ok(())
        }

        fn get_csrf(&mut self, url: &str) -> Result<String, CFToolError> {
            let body = self.get_url(url)?;
            let csrf_regex = Regex::new(r#"csrf='(.+?)'"#).unwrap();
            let caps = csrf_regex
                .captures(&body)
                .ok_or(CFToolError::FailedParseRespone)?;

            Ok(caps[1].to_string())
        }

        pub fn get_url(&mut self, url: &str) -> Result<String, CFToolError> {
            self.set_rcpc()?;

            let builder = self.client.get(url);
            let respone = builder.send().map_err(|_| CFToolError::FailedRequest)?;

            if respone.status().is_success() {
                Ok(respone.text().map_err(|_| CFToolError::FailedRequest)?)
            } else {
                Err(CFToolError::WrongRespone(respone.status().as_u16()))
            }
        }

        pub fn post_url(
            &mut self,
            url: &str,
            csrf_url: &str,
            mut params: Vec<(&str, String)>,
        ) -> Result<String, CFToolError> {
            self.set_rcpc()?;

            // Construct parmas
            let ftaa = gen_ftaa();
            params.push(("bfaa", PARMA_BFAA.into()));
            params.push(("ftaa", ftaa));
            params.push(("csrf_token", self.get_csrf(csrf_url)?));
            let url = reqwest::Url::parse_with_params(url, params)
                .map_err(|_| CFToolError::FailedRequest)?;

            let builder = self.client.post(url);
            let respone = builder.send().map_err(|_| CFToolError::FailedRequest)?;

            if respone.status().is_success() {
                Ok(respone.text().map_err(|_| CFToolError::FailedRequest)?)
            } else {
                Err(CFToolError::WrongRespone(respone.status().as_u16()))
            }
        }

        pub fn post_url_submit(
            &mut self,
            url: &str,
            csrf_url: &str,
            mut params: Vec<(&str, String)>,
        ) -> Result<String, CFToolError> {
            self.set_rcpc()?;

            // Construct parmas
            let ftaa = gen_ftaa();
            params.push(("bfaa", PARMA_BFAA.into()));
            params.push(("ftaa", ftaa));
            let token = self.get_csrf(csrf_url)?;
            params.push(("csrf_token", token.clone()));
            let response = self
                .client
                .post(format!("{}?csrf_token={}", url, token))
                .form(&params)
                .send()
                .unwrap();

            if response.status().is_success() {
                Ok(response.text().map_err(|_| CFToolError::FailedRequest)?)
            } else {
                Err(CFToolError::WrongRespone(response.status().as_u16()))
            }
        }

        fn check_login(&mut self) -> Result<Option<String>, CFToolError> {
            let body = self.get_url("https://codeforces.com/enter")?;

            let handle_regex = Regex::new(r#"handle = "(.+?)""#).unwrap();
            let caps = handle_regex.captures(&body);

            Ok(caps.map(|caps| caps[1].to_string()))
        }

        pub fn login(&mut self) -> Result<(), CFToolError> {
            if self.logged_in {
                return Ok(());
            }

            if let Some(handle) = self.check_login()? {
                self.logged_in = true;
                success(&format!("Current user: {}", handle));
                return Ok(());
            }

            failure("Not logged in");

            let handle = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Handle: ")
                .interact_on(&Term::stdout())
                .unwrap();
            let password = Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Password: ")
                .interact_on(&Term::stdout())
                .unwrap();

            self.post_url(
                "https://codeforces.com/enter",
                "https://codeforces.com/enter",
                vec![
                    ("handleOrEmail", handle),
                    ("password", password),
                    ("action", "enter".into()),
                    ("_tta", "176".into()),
                    ("remember", "on".into()),
                ],
            )?;

            if let Some(handle) = self.check_login()? {
                self.logged_in = true;
                success(&format!("Current user: {}", handle));
                Ok(())
            } else {
                failure("Login Failed. Check your handle and password.");
                Err(CFToolError::FailedRequest)
            }
        }

        pub fn submit(
            &mut self,
            contest_id: u32,
            problem_id: &str,
            contest_type: &str,
        ) -> Result<String, CFToolError> {
            println!("Submitting {} {}", contest_id, problem_id);

            let mut file = File::open("./main/src/main.rs").unwrap();
            let mut source_code = String::new();

            file.read_to_string(&mut source_code).unwrap();

            let submit_url = format!(
                "https://codeforces.com/{}/{}/submit",
                contest_type, contest_id
            );
            let body = self.post_url_submit(
                &submit_url,
                &submit_url,
                vec![
                    ("action", "submitSolutionFormSubmitted".into()),
                    ("submittedProblemIndex", problem_id.to_ascii_uppercase()),
                    ("programTypeId", "75".to_string()),
                    ("source", source_code),
                    ("tabSize", "4".into()),
                    ("sourceFile", "".into()),
                    ("_tta", "869".into()),
                ],
            )?;

            let error_regex = Regex::new(r#"error[a-zA-Z_\- ]*">(.*?)</span>"#).unwrap();
            let error_caps = error_regex.captures(&body);

            if error_caps.is_some() {
                failure(&format!("Submit Failed: {}", &error_caps.unwrap()[1]));

                return Err(CFToolError::FailedRequest);
            }

            success("Submitted");

            // let mut file = File::create("text").unwrap();
            // writeln!(file, "{}", body).unwrap();

            let submit_id_regex = Regex::new(r#"<tr\sdata-submission-id="(\d+)""#).unwrap();
            let submit_caps = submit_id_regex.captures(&body);

            if let Some(caps) = submit_caps {
                Ok(caps[1].to_string())
            } else {
                println!("Failed to get submission id");
                Err(CFToolError::NoIdReturned)
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum CFToolError {
        FailedRequest,
        FailedParseRespone,
        FailedTerminalOutput,
        WrongRespone(u16),
        NoIdReturned,
    }

    impl fmt::Display for CFToolError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "CFToolError")
        }
    }

    impl From<std::io::Error> for CFToolError {
        fn from(_: std::io::Error) -> CFToolError {
            CFToolError::FailedTerminalOutput
        }
    }

    pub fn get_url(url: &str) -> Result<String, CFToolError> {
        let client = reqwest::blocking::Client::builder().build().unwrap();
        let builder = client.get(url);
        let respone = builder.send().map_err(|_| CFToolError::FailedRequest)?;
        if respone.status().is_success() {
            Ok(respone.text().map_err(|_| CFToolError::FailedRequest)?)
        } else {
            Err(CFToolError::FailedRequest)
        }
    }

    pub fn gen_ftaa() -> String {
        use rand::{distributions::Alphanumeric, Rng};
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(18)
            .map(char::from)
            .collect()
    }
}
