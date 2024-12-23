#[cfg(test)]
mod test {
    use expect_test::expect_file;

    use crate::file_explorer::{FakeFileExplorer, FileExplorer};
    use crate::old_build::build_several_libraries_impl;

    fn gen_code<F: FileExplorer>(file_explorer: &mut F) -> String {
        build_several_libraries_impl(
            &["algo_lib".to_owned(), "marathon_utils".to_owned()],
            file_explorer,
            false,
        )
        .join("\n")
    }

    #[test]
    fn simple() {
        let mut file_explorer = FakeFileExplorer::new();
        file_explorer.add_file(
            "src/main.rs",
            r#"
            pub fn submit() {
                println!("Hello world!");
            }
        "#,
        );
        let expected = expect_file!["outputs/simple_main.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }

    #[test]
    fn use_lib() {
        let mut file_explorer = FakeFileExplorer::new();
        file_explorer.add_file(
            "src/main.rs",
            r#"
            use algo_lib::double::double;

            pub fn submit() {
                println!("{}", double(2));
            }
        "#,
        );
        file_explorer.add_file(
            "../../algo_lib/src/double.rs",
            r#"
            pub fn double(x : i32) -> i32 {
                x * 2
            }
        "#,
        );
        let expected = expect_file!["outputs/use_lib_main.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }

    #[test]
    fn dbg_macro() {
        let mut file_explorer = FakeFileExplorer::new();
        file_explorer.add_file(
            "src/main.rs",
            r#"
            use algo_lib::dbg;

            pub fn submit() {
                dbg!("Hello");
            }
        "#,
        );
        file_explorer.add_file(
            "../../algo_lib/src/misc/dbg_macro.rs",
            r#"
            
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

        "#,
        );
        let expected = expect_file!["outputs/dbg_macro_main.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }

    #[test]
    fn several_libs() {
        let mut file_explorer = FakeFileExplorer::new();
        file_explorer.add_file(
            "src/main.rs",
            r#"
            use algo_lib::double::double;
            use marathon_utils::sum::sum;

            pub fn submit() {
                println!("{}", sum(5, double(2)));
            }
        "#,
        );
        file_explorer.add_file(
            "../../algo_lib/src/double.rs",
            r#"
            pub fn double(x : i32) -> i32 {
                x * 2
            }
        "#,
        );

        file_explorer.add_file(
            "../../marathon_utils/src/sum.rs",
            r#"
            pub fn sum(x : i32, y : i32) -> i32 {
                x + y
            }
        "#,
        );
        let expected = expect_file!["outputs/several_libs_main.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }

    #[test]
    fn use_super() {
        let mut file_explorer = FakeFileExplorer::new();
        file_explorer.add_file(
            "src/main.rs",
            r#"
            use algo_lib::geometry::convex_polygon_intersection::convex_polygon_intersection;

            pub fn submit() {
                convex_polygon_intersection();
            }
        "#,
        );
        file_explorer.add_file(
            "../../algo_lib/src/geometry/convex_polygon_intersection.rs",
            r#"

            use super::half_plane_intersection::half_plane_intersection;

            pub fn convex_polygon_intersection() {
                half_plane_intersection();
            }
        "#,
        );
        // TODO: WARNING!
        // Current generated output is NOT correct!
        // this file should be recursively included in the generated [main.rs]
        file_explorer.add_file(
            "../../algo_lib/src/geometry/half_plane_intersection.rs",
            r#"

            pub fn half_plane_intersection() {
                // ...
            }
        "#,
        );
        let expected = expect_file!["outputs/use_super_main.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }

    #[test]
    fn several_files_in_solution() {
        let mut file_explorer = FakeFileExplorer::new();
        file_explorer.add_file(
            "src/main.rs",
            r#"

            mod helper;
            use crate::helper::help;

            pub fn submit() {
                help();
            }
        "#,
        );
        file_explorer.add_file(
            "src/helper.rs",
            r#"
            pub fn help() {
                println!("Hello world!");
            }
        "#,
        );
        let expected = expect_file!["outputs/several_files_in_solution.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }

    #[test]
    fn mod_inside_file() {
        let mut file_explorer = FakeFileExplorer::new();
        file_explorer.add_file(
            "src/main.rs",
            r#"

            pub fn submit() {
                // ...
            }

            mod tests {
                fn some_test() {

                }
            }
        "#,
        );
        let expected = expect_file!["outputs/mod_inside_file.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }

    fn add_new_io_templates(file_explorer: &mut FakeFileExplorer) {
        file_explorer.add_file(
            "../../templates/main/main.rs",
            r#"
        fn main() {
            $INPUT
            $OUTPUT
                crate::solution::run(input, output);
        }
        "#,
        );

        file_explorer.add_file(
            "../../templates/main/stdin.rs",
            r#"
            let mut sin = std::io::stdin();
            let input = if $INTERACTIVE {
                crate::io::input::Input::new_with_size(&mut sin, 1)
            } else {
                crate::io::input::Input::new(&mut sin)
            };
            "#,
        );

        file_explorer.add_file(
            "../../templates/main/stdout.rs",
            r#"
            let mut stdout = std::io::stdout();
            let output = if $INTERACTIVE {
                crate::io::output::Output::new_with_auto_flush(&mut stdout)
            } else {
                crate::io::output::Output::new(&mut stdout)
            };
            "#,
        );

        file_explorer.add_file(
            "../../templates/classic.rs",
            r#"
            "#,
        );
    }

    #[test]
    fn new_io_templates() {
        let mut file_explorer = FakeFileExplorer::new();
        add_new_io_templates(&mut file_explorer);
        file_explorer.add_file(
            "src/main.rs",
            r#"
            //{"name":"d","group":"Manual","url":"","interactive":false,"timeLimit":2000,"tests":[{"input":"","output":""},{"input":"","output":""}],"testType":"single","input":{"type":"stdin","fileName":null,"pattern":null},"output":{"type":"stdout","fileName":null,"pattern":null},"languages":{"java":{"taskClass":"d"}}}

            pub fn run(mut input: Input, mut output: Output) {
                println!("Hello world!");
            }
        "#,
        );
        let expected = expect_file!["outputs/new_io_templates.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }
}
