use crate::{archiver, listener, submit, task_creator};
use dialoguer::console::Term;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;

const OPTIONS: [&str; 5] = ["Run listener", "Submit", "Create new task", "Archive tasks", "Exit"];

pub fn run_menu() {
    loop {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select option:")
            .default(0)
            .items(&OPTIONS)
            .interact_on(&Term::stdout())
            .unwrap();
        match selection {
            0 => {
                listener::listen();
            }
            1 => {
                submit::submit();
            }
            2 => {
                task_creator::create_task_wizard();
            }
            3 => {
                archiver::archive();
            }
            4 => {
                return;
            }
            _ => unreachable!(),
        }
    }
}
