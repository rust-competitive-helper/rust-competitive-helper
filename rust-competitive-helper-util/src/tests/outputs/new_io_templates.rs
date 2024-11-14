// 
pub mod solution {

            //{"name":"d","group":"Manual","url":"","interactive":false,"timeLimit":2000,"tests":[{"input":"","output":""},{"input":"","output":""}],"testType":"single","input":{"type":"stdin","fileName":null,"pattern":null},"output":{"type":"stdout","fileName":null,"pattern":null},"languages":{"java":{"taskClass":"d"}}}

            pub fn run(mut input: Input, mut output: Output) {
                println!("Hello world!");
            }
        
}

        fn main() {
            
            let mut sin = std::io::stdin();
            let input = if 
             {
                crate::io::input::Input::new_with_size(&mut sin, 1)
            } else {
                crate::io::input::Input::new(&mut sin)
            };
            
            
            let mut stdout = std::io::stdout();
            let output = if 
             {
                crate::io::output::Output::new_with_auto_flush(&mut stdout)
            } else {
                crate::io::output::Output::new(&mut stdout)
            };
            
                crate::solution::run(input, output);
        }
        