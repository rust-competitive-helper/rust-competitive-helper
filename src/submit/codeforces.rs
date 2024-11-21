// use crate::submit;
// use regex::Regex;
// use reqwest::blocking::Client;
// use serde::Serialize;
// use std::io::{Read, Write};
// use std::net::{Shutdown, TcpListener};
// use std::thread;
// use std::time::{Duration, Instant};
//
// #[derive(Serialize)]
// struct CodeforcesSubmit {
//     empty: bool,
//     #[serde(rename = "problemName")]
//     problem_name: String,
//     url: String,
//     #[serde(rename = "sourceCode")]
//     source_code: String,
//     #[serde(rename = "languageId")]
//     language_id: i32,
// }
//
// // Url should be https://codeforces.com/$contest_type/$contest_id/problem/$problem_id
// pub(crate) fn submit(url: &str) -> bool {
//     let client = Client::builder().build().unwrap();
//     let url_regex = Regex::new(r"https://codeforces.com/.*/problem/(.+)").unwrap();
//     let problem_id = {
//         match url_regex.captures(url) {
//             None => {
//                 submit::failure("Unexpected URL for codeforces problem");
//                 return false;
//             }
//             Some(caps) =>
//                 caps[1].to_string().replace("/", ""),
//         }
//     };
//     let listener = TcpListener::bind("127.0.0.1:27121").unwrap();
//     listener.set_nonblocking(true).unwrap();
//     let started = Instant::now();
//     loop {
//         let mut stream = match listener.accept() {
//             Ok((stream, _)) => stream,
//             Err(_) => {
//                 if started.elapsed() > Duration::from_secs(5) {
//                     submit::failure("You probably had not installed cph-submit extension from https://github.com/agrawal-d/cph-submit");
//                     return false;
//                 }
//                 thread::sleep(Duration::from_secs(1));
//                 continue;
//             }
//         };
//         stream.write_all(b"HTTP/1.1 200 OK\n").unwrap();
//         stream.write_all(b"Content-Type: application/json; charset=UTF-8\n").unwrap();
//         let body = serde_json::to_vec(&CodeforcesSubmit {
//             empty: false,
//             problem_name: problem_id,
//             url: url.to_string(),
//             source_code: std::fs::read_to_string("./main/src/main.rs").unwrap(),
//             language_id: 75,
//         }).unwrap();
//         stream.write_all(format!("Content-Length: {}\n\n", body.as_slice().len()).as_bytes()).unwrap();
//         stream.write_all(body.as_slice()).unwrap();
//         stream.flush().unwrap();
//         thread::sleep(Duration::from_secs(3));
//         stream.shutdown(Shutdown::Both).unwrap();
//         return true;
//     }
//     /*    Server::bind("localhost:27121").serve(|req, info| {
//             Response::new(Body::new(serde_json::to_vec(&CodeforcesSubmit {
//                 empty: false,
//                 problem_name: problem_id,
//                 url: url.to_string(),
//                 source_code: std::fs::read_to_string("./main/src/main.rs").unwrap(),
//                 language_id: 75,
//             }).unwrap()))
//         });
//         let get = client.get("http://localhost:27121/getSubmit").body(serde_json::to_vec(&CodeforcesSubmit {
//             empty: false,
//             problem_name: problem_id,
//             url: url.to_string(),
//             source_code: std::fs::read_to_string("./main/src/main.rs").unwrap(),
//             language_id: 75,
//         }).unwrap()).header("cph-submit", "true").send();
//         match get {
//             Ok(_) => true,
//             Err(err) => {
//                 submit::failure(format!("Failed to send request: {:?}", err).as_str());
//                 submit::failure("Make sure you have installed cph-submit from https://github.com/agrawal-d/cph-submit");
//                 false
//             }
//         }*/
// }
