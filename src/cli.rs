use crate::{archiver, submit, task_creator};
use rust_competitive_helper_util::{IOEnum, IOType, Task, Test, TestType};
use std::path::Path;

pub fn run(args: &[String]) {
    let (cmd, rest) = (args[0].as_str(), &args[1..]);
    match cmd {
        "submit" => {
            if let Err(e) = expect_no_args(rest, "submit") {
                fail(&e);
            }
            submit::submit();
        }
        "new" => match parse_new(rest) {
            Ok(args) => task_creator::create(build_task(args)),
            Err(e) => fail(&format!("new: {}\n\n{}", e, NEW_USAGE)),
        },
        "archive" => match parse_archive(rest) {
            Ok(args) => run_archive(args),
            Err(e) => fail(&format!("archive: {}\n\n{}", e, ARCHIVE_USAGE)),
        },
        "help" | "-h" | "--help" => println!("{}", HELP),
        other => fail(&format!("Unknown command: {}\n\n{}", other, HELP)),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("{}", msg);
    std::process::exit(2);
}

fn expect_no_args(args: &[String], cmd: &str) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!("{} takes no arguments (got: {:?})", cmd, args))
    }
}

struct NewArgs {
    name: String,
    tests: usize,
    test_type: TestType,
    input: IOType,
    output: IOType,
    interactive: bool,
    time_limit: u64,
    group: String,
}

fn parse_new(args: &[String]) -> Result<NewArgs, String> {
    let mut name: Option<String> = None;
    let mut tests = 0usize;
    let mut test_type = TestType::Single;
    let mut input_file: Option<String> = None;
    let mut output_file: Option<String> = None;
    let mut interactive = false;
    let mut time_limit = 2000u64;
    let mut group = "Manual".to_string();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--tests" => tests = take_value(args, &mut i, "--tests")?.parse().map_err(|_| "invalid --tests value".to_string())?,
            "--test-type" => {
                let v = take_value(args, &mut i, "--test-type")?;
                test_type = match v.as_str() {
                    "single" => TestType::Single,
                    "multi-number" => TestType::MultiNumber,
                    "multi-eof" => TestType::MultiEof,
                    _ => return Err(format!("--test-type expects single|multi-number|multi-eof (got {})", v)),
                };
            }
            "--input-file" => input_file = Some(take_value(args, &mut i, "--input-file")?),
            "--output-file" => output_file = Some(take_value(args, &mut i, "--output-file")?),
            "--interactive" => interactive = true,
            "--time-limit" => time_limit = take_value(args, &mut i, "--time-limit")?.parse().map_err(|_| "invalid --time-limit value".to_string())?,
            "--group" => group = take_value(args, &mut i, "--group")?,
            other if other.starts_with("--") => return Err(format!("unknown flag: {}", other)),
            _ => {
                if name.is_some() {
                    return Err(format!("unexpected positional: {}", a));
                }
                name = Some(a.clone());
            }
        }
        i += 1;
    }
    let name = name.ok_or_else(|| "missing task <name>".to_string())?;
    let input = match input_file {
        Some(p) => IOType { io_type: IOEnum::File, file_name: Some(p), pattern: None },
        None => IOType { io_type: IOEnum::StdIn, file_name: None, pattern: None },
    };
    let output = match output_file {
        Some(p) => IOType { io_type: IOEnum::File, file_name: Some(p), pattern: None },
        None => IOType { io_type: IOEnum::StdOut, file_name: None, pattern: None },
    };
    Ok(NewArgs { name, tests, test_type, input, output, interactive, time_limit, group })
}

fn build_task(a: NewArgs) -> Task {
    Task {
        name: a.name,
        group: a.group,
        url: String::new(),
        interactive: a.interactive,
        time_limit: a.time_limit,
        tests: vec![Test { input: String::new(), output: String::new() }; a.tests],
        test_type: a.test_type,
        input: a.input,
        output: a.output,
    }
}

enum ArchiveTarget {
    Contest(String),
    Task(String),
}

struct ArchiveArgs {
    target: ArchiveTarget,
    action: usize, // 0=skip, 1=delete, 2=archive only, 3=archive+tests
}

fn parse_archive(args: &[String]) -> Result<ArchiveArgs, String> {
    let mut contest: Option<String> = None;
    let mut task: Option<String> = None;
    let mut action: usize = 2;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--task" => task = Some(take_value(args, &mut i, "--task")?),
            "--action" => {
                let v = take_value(args, &mut i, "--action")?;
                action = match v.as_str() {
                    "skip" => 0,
                    "delete" => 1,
                    "archive" => 2,
                    "archive-tests" => 3,
                    _ => return Err(format!("--action expects skip|delete|archive|archive-tests (got {})", v)),
                };
            }
            other if other.starts_with("--") => return Err(format!("unknown flag: {}", other)),
            _ => {
                if contest.is_some() {
                    return Err(format!("unexpected positional: {}", a));
                }
                contest = Some(a.clone());
            }
        }
        i += 1;
    }
    let target = match (contest, task) {
        (Some(_), Some(_)) => return Err("provide either <contest> or --task NAME, not both".into()),
        (Some(c), None) => ArchiveTarget::Contest(c),
        (None, Some(t)) => ArchiveTarget::Task(t),
        (None, None) => return Err("missing <contest> or --task NAME".into()),
    };
    Ok(ArchiveArgs { target, action })
}

fn run_archive(args: ArchiveArgs) {
    match args.target {
        ArchiveTarget::Task(name) => {
            if !Path::new(&format!("tasks/{}/src/main.rs", name)).is_file() {
                fail(&format!("Task not found: tasks/{}/src/main.rs", name));
            }
            archiver::ask_archive(name, args.action);
        }
        ArchiveTarget::Contest(name) => {
            let contests = archiver::contest_list();
            let entry = contests.iter().find(|(c, _)| c == &name);
            let Some((_, tasks)) = entry else {
                fail(&format!("Contest not found: {}", name));
            };
            for task in tasks.iter().cloned() {
                archiver::ask_archive(task, args.action);
            }
        }
    }
}

fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i).cloned().ok_or_else(|| format!("{} expects a value", flag))
}

const NEW_USAGE: &str = "Usage: rust-competitive-helper new <name>
    [--tests N]                                       (default 0)
    [--test-type single|multi-number|multi-eof]       (default single)
    [--input-file PATH]                               (default stdin)
    [--output-file PATH]                              (default stdout)
    [--interactive]                                   (default false)
    [--time-limit MS]                                 (default 2000)
    [--group NAME]                                    (default \"Manual\")";

const ARCHIVE_USAGE: &str = "Usage: rust-competitive-helper archive (<contest> | --task <name>)
    [--action skip|delete|archive|archive-tests]      (default archive)";

const HELP: &str = "rust-competitive-helper — competitive programming task helper

Usage:
    rust-competitive-helper                 launch the interactive menu
    rust-competitive-helper submit          submit main/src/main.rs
    rust-competitive-helper new <name> ...  create a task non-interactively
    rust-competitive-helper archive ...     archive a contest or single task
    rust-competitive-helper help            show this help

new flags:
    --tests N                                       (default 0)
    --test-type single|multi-number|multi-eof       (default single)
    --input-file PATH                               (default stdin)
    --output-file PATH                              (default stdout)
    --interactive                                   (default false)
    --time-limit MS                                 (default 2000)
    --group NAME                                    (default \"Manual\")

archive flags:
    --task NAME                                     archive a single task
    --action skip|delete|archive|archive-tests      (default archive)";
