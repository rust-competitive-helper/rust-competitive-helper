use crate::{archiver, listener, submit, task_creator};
use dialoguer::console::Term;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;

const OPTIONS: [&str; 4] = ["Submit", "Create new task", "Archive tasks", "Exit"];

pub fn run_menu() {
    listener::start_listener();

    loop {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select option:")
            .default(0)
            .items(&OPTIONS)
            .interact_on_opt(&Term::stdout())
            .unwrap();
        match selection {
            Some(0) => submit::submit(),
            Some(1) => task_creator::create_task_wizard(),
            Some(2) => archiver::archive(),
            Some(3) => return,
            None => continue,
            _ => unreachable!(),
        }
    }
}
