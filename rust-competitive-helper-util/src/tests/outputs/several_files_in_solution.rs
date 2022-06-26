pub mod solution {


use crate::helper::help;

            pub fn submit() {
                help();
            }
        
}
pub mod helper {

            pub fn help() {
                println!("Hello world!");
            }
        
}
fn main() {
    crate::solution::submit();
}