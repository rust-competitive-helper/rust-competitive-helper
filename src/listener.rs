use crate::task_creator;
use rust_competitive_helper_util::Task;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};

pub static REDRAW_NEEDED: AtomicBool = AtomicBool::new(false);

fn handle(mut stream: TcpStream) {
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    let request = String::from_utf8_lossy(&buf[..]).to_string();
    if request.is_empty() {
        return;
    }
    let pos = request.find('{');
    match pos {
        None => {
            println!("Bad request: {}", request);
        }
        Some(pos) => {
            process(&request[pos..]);
        }
    }
    REDRAW_NEEDED.store(true, Ordering::Relaxed);
}

fn process(request: &str) {
    let task: Task = serde_json::from_slice(request.as_bytes()).unwrap();
    task_creator::create(task);
}

pub fn start_listener() {
    std::thread::spawn(|| {
        let listener = TcpListener::bind("127.0.0.1:4244").unwrap();
        println!("Listening for connections on port 4244");
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    handle(stream);
                }
                Err(e) => {
                    println!("Unable to connect: {}", e);
                }
            }
        }
    });
}
