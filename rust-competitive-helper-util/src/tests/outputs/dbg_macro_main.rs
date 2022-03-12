pub mod solution {

use crate::dbg;

            pub fn submit() {
                dbg!("Hello");
            }
        
}
pub mod algo_lib {
pub mod misc {
pub mod dbg_macro {

            
            #[macro_export]
            #[allow(unused_macros)]
            macro_rules! dbg {
                ($first_val:expr, $($val:expr),+ $(,)?) => {
                    eprint!("[{}:{}] {} = {:?}",
                                file!(), line!(), stringify!($first_val), &$first_val);
                    ($(eprint!(", {} = {:?}", stringify!($val), &$val)),+,);
                    eprintln!();
                };
                ($first_val:expr) => {
                    eprintln!("[{}:{}] {} = {:?}",
                                file!(), line!(), stringify!($first_val), &$first_val)
                };
            }

        
}
}
}
fn main() {
    crate::solution::submit();
}