mod archiver;
mod cli;
mod config;
mod listener;
mod menu;
mod submit;
mod task_creator;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        menu::run_menu();
    } else {
        cli::run(&args);
    }
}
