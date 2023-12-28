pub mod solution {

use crate::algo_lib::double::double;
use crate::marathon_utils::sum::sum;

            pub fn submit() {
                println!("{}", sum(5, double(2)));
            }
        
}
pub mod algo_lib {
pub mod double {

            pub fn double(x : i32) -> i32 {
                x * 2
            }
        
}
}
pub mod marathon_utils {
pub mod sum {

            pub fn sum(x : i32, y : i32) -> i32 {
                x + y
            }
        
}
}
fn main() {
    crate::solution::submit();
}