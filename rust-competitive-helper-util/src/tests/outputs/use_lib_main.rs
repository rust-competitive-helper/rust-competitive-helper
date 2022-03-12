pub mod solution {

use crate::algo_lib::double::double;

            pub fn submit() {
                println!("{}", double(2));
            }
        
}
pub mod algo_lib {
pub mod double {

            pub fn double(x : i32) -> i32 {
                x * 2
            }
        
}
}
fn main() {
    crate::solution::submit();
}