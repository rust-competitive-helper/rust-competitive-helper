use crate::config::Config;
use dialoguer::console::Term;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Select};
use rand::{random, Rng};
use rust_competitive_helper_util::{
    read_from_file, read_lines, write_lines, write_to_file, IOEnum, IOType, Task, Test, TestType,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn task_name(task: &Task) -> String {
    let mut res = String::new();
    for c in task.name.chars() {
        if c.is_whitespace() {
            if !res.is_empty() && !res.ends_with('_') {
                res.push('_');
            }
        } else if c.is_ascii_alphabetic() {
            res.push(c.to_ascii_lowercase());
        } else if c.is_ascii_digit() {
            if res.is_empty() {
                res.push_str("task_");
            }
            res.push(c);
        }
    }
    if res.is_empty() {
        res.push_str(&format!("task_{}", random.gen_range(0..10000)));
    }
    res
}

fn select_test_type() -> TestType {
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select test type:")
        .default(0)
        .items(&TestType::INPUT_TYPES[..])
        .interact_on(&Term::stdout())
        .unwrap();
    match selection {
        0 => TestType::Single,
        1 => TestType::MultiNumber,
        2 => TestType::MultiEof,
        _ => unreachable!(),
    }
}

pub fn get_solve(task: &Task) -> String {
    let mut spl = task.group.split(' ');
    let site = match spl.next() {
        None => "default".to_string(),
        Some(site) => site.to_lowercase(),
    };
    let solve = std::fs::read_to_string(format!("templates/sites/{}.rs", site));
    match solve {
        Err(_) => read_from_file("templates/sites/default.rs").unwrap(),
        Ok(solve) => solve,
    }
}

pub fn get_invoke(task: &Task) -> Option<String> {
    read_from_file(match task.test_type {
        TestType::Single => "templates/single.rs",
        TestType::MultiNumber => "templates/multi_number.rs",
        TestType::MultiEof => "templates/multi_eof.rs",
    })
}

pub fn get_interactive(task: &Task) -> Option<String> {
    read_from_file(if task.interactive {
        "templates/interactive.rs"
    } else {
        "templates/classic.rs"
    })
}

pub fn get_io_settings(task: &Task) -> String {
    let input = match task.input.io_type {
        IOEnum::StdIn | IOEnum::Regex => "TaskIoType::Std".to_string(),
        IOEnum::StdOut => panic!("input should not have type StdOut"),
        IOEnum::File => format!(
            "TaskIoType::File(\"{}\".to_string())",
            task.input.file_name.clone().unwrap()
        ),
    };
    let output = match task.output.io_type {
        IOEnum::StdOut | IOEnum::Regex => "TaskIoType::Std".to_string(),
        IOEnum::StdIn => panic!("output should not have type StdIn"),
        IOEnum::File => format!(
            "TaskIoType::File(\"{}\".to_string())",
            task.output.file_name.clone().unwrap()
        ),
    };
    format!(
        "TaskIoSettings {{
        is_interactive: {},
        input: {},
        output: {},
    }}",
        task.interactive, input, output
    )
}

fn generate_new_cargo_toml_content(task_name: &str) -> Option<Vec<String>> {
    let mut lines = Vec::new();
    for l in read_lines("Cargo.toml").unwrap() {
        if l.contains(format!("\"tasks/{}\"", task_name).as_str()) {
            eprintln!("Task {} exists", task_name);
            open_task(task_name.to_string(), None);
            return None;
        }
        lines.push(l.clone());
        if l.as_str() == "members = [" {
            lines.push(format!("    \"tasks/{}\",", task_name));
        }
    }
    Some(lines)
}

pub fn create(task: Task) {
    let name = task_name(&task);

    let new_cargo_toml_content = match generate_new_cargo_toml_content(&name) {
        Some(content) => content,
        None => return,
    };

    fs::create_dir_all(format!("tasks/{}/src", name)).unwrap();
    fs::create_dir_all(format!("tasks/{}/tests", name)).unwrap();
    for (i, test) in task.tests.iter().enumerate() {
        write_to_file(format!("tasks/{}/tests/{}.in", name, i + 1), &test.input);
        write_to_file(format!("tasks/{}/tests/{}.out", name, i + 1), &test.output);
    }
    write_to_file(
        format!("tasks/{}/build.rs", name),
        read_from_file("templates/build.rs").expect("templates/build.rs not found"),
    );
    let mut solve = get_solve(&task);
    if let Some(invoke) = get_invoke(&task) {
        solve = solve.replace("$INVOKE", invoke.as_str());
    }
    if let Some(interactive) = get_interactive(&task) {
        solve = solve.replace("$INTERACTIVE", interactive.as_str());
    }
    let mut main = read_from_file("templates/main.rs").expect("templates/main.rs not found");
    main = main.replace("$SOLVE", solve.as_str());
    main = main.replace("$JSON", serde_json::to_string(&task).unwrap().as_str());
    main = main.replace("$IO_SETTINGS", get_io_settings(&task).as_str());
    let (row, col): (i32, i32) = match main.find("$CARET") {
        None => (1, 1),
        Some(pos) => {
            let chars = main[..pos].chars();
            let mut row = 1;
            let mut col = 1;
            for c in chars {
                if c == '\n' {
                    row += 1;
                    col = 0;
                }
                col += 1;
            }
            (row, col)
        }
    };
    main = main.replace("$CARET", "");
    main = main.replace("$TASK", name.as_str());
    match task.input.io_type {
        IOEnum::StdIn => {
            main = main.replace(
                "$INPUT",
                &read_from_file("templates/main/stdin.rs").unwrap(),
            );
        }
        IOEnum::Regex => {
            main = main.replace(
                "$INPUT",
                &read_from_file("templates/main/regex.rs").unwrap(),
            );
        }
        IOEnum::File => {
            main = main.replace(
                "$INPUT",
                &read_from_file("templates/main/file_in.rs").unwrap(),
            );
        }
        IOEnum::StdOut => {
            unreachable!()
        }
    }
    match task.output.io_type {
        IOEnum::StdOut => {
            main = main.replace(
                "$OUTPUT",
                &read_from_file("templates/main/stdout.rs").unwrap(),
            );
        }
        IOEnum::File => {
            main = main.replace(
                "$OUTPUT",
                &read_from_file("templates/main/file_out.rs").unwrap(),
            );
        }
        IOEnum::Regex | IOEnum::StdIn => {
            unreachable!()
        }
    }
    write_to_file(format!("tasks/{}/src/main.rs", name), main);
    if Path::new("templates/tester.rs").exists() {
        let mut tester =
            read_from_file("templates/tester.rs").expect("templates/tester.rs not found");
        tester = tester.replace("$TIME_LIMIT", task.time_limit.to_string().as_str());
        tester = tester.replace("$TASK", name.as_str());
        if let Some(interactive) = get_interactive(&task) {
            tester = tester.replace("$INTERACTIVE", interactive.as_str());
        }
        write_to_file(format!("tasks/{}/src/tester.rs", name), tester);
    }
    let mut toml = read_from_file("templates/Cargo.toml").expect("templates/Cargo.toml not found");
    toml = toml.replace("$TASK", name.as_str());
    write_to_file(format!("tasks/{}/Cargo.toml", name).as_str(), toml);

    write_lines("Cargo.toml", new_cargo_toml_content);
    println!("Task {} parsed!", name);
    open_task(name, Some((row, col)));
}

fn open_task(name: String, coords: Option<(i32, i32)>) {
    let config = Config::load();
    let open_task_result = {
        let mut templates_args: HashMap<String, String> = HashMap::new();
        templates_args.insert("$NAME".to_owned(), name.clone());
        if let Some((row, col)) = coords {
            templates_args.insert("$LINE".to_owned(), row.to_string());
            templates_args.insert("$COLUMN".to_owned(), col.to_string());
        } else {
            templates_args.insert("$LINE".to_owned(), "0".to_owned());
            templates_args.insert("$COLUMN".to_owned(), "0".to_owned());
        }
        templates_args.insert("$FILE".to_owned(), format!("tasks/{}/src/main.rs", name));
        config.run_open_task_command(&templates_args)
    };
    match open_task_result {
        Ok(_) => {}
        Err(err) => eprintln!("{}", err),
    }
}

fn select_name() -> String {
    Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Task name:")
        .interact_on(&Term::stdout())
        .unwrap()
}

fn select_num_tests() -> usize {
    Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Num sample tests:")
        .validate_with(|input: &String| match input.parse::<usize>() {
            Ok(num) => {
                if num > 10 {
                    Err("Too many")
                } else {
                    Ok(())
                }
            }
            Err(_) => Err("Please enter number"),
        })
        .interact_on(&Term::stdout())
        .unwrap()
        .parse()
        .unwrap()
}

const INPUT_OPTIONS: [&str; 2] = ["Stdin", "File"];

fn select_input_type() -> IOType {
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select option:")
        .default(0)
        .items(&INPUT_OPTIONS)
        .interact_on(&Term::stdout())
        .unwrap();
    let (io_type, file_name) = match selection {
        0 => (IOEnum::StdIn, None),
        1 => (
            IOEnum::File,
            Some(
                Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Input file name:")
                    .interact_on(&Term::stdout())
                    .unwrap(),
            ),
        ),
        _ => unreachable!(),
    };
    IOType {
        io_type,
        file_name,
        pattern: None,
    }
}

const OUTPUT_OPTIONS: [&str; 2] = ["Stdout", "File"];

fn select_output_type() -> IOType {
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select option:")
        .default(0)
        .items(&OUTPUT_OPTIONS)
        .interact_on(&Term::stdout())
        .unwrap();
    let (io_type, file_name) = match selection {
        0 => (IOEnum::StdOut, None),
        1 => (
            IOEnum::File,
            Some(
                Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Output file name:")
                    .interact_on(&Term::stdout())
                    .unwrap(),
            ),
        ),
        _ => unreachable!(),
    };
    IOType {
        io_type,
        file_name,
        pattern: None,
    }
}

pub fn create_task_wizard() {
    let name = select_name();
    let task = Task {
        name: name.clone(),
        group: "Manual".to_string(),
        url: "".to_string(),
        interactive: false,
        time_limit: 2000,
        tests: vec![
            Test {
                input: "".to_string(),
                output: "".to_string()
            };
            select_num_tests()
        ],
        test_type: select_test_type(),
        input: select_input_type(),
        output: select_output_type(),
    };
    create(task);
}
