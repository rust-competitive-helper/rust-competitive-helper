#[cfg(test)]
mod test {
    use expect_test::expect_file;

    use crate::{
        build::build_several_libraries_impl,
        file_explorer::{FakeFileExplorer, FileExplorer},
    };

    fn gen_code<F: FileExplorer>(file_explorer: &mut F) -> String {
        build_several_libraries_impl(
            &["algo_lib".to_owned(), "marathon_utils".to_owned()],
            file_explorer,
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
            "../algo_lib/src/double.rs",
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
            "../algo_lib/src/misc/dbg_macro.rs",
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
            "../algo_lib/src/double.rs",
            r#"
            pub fn double(x : i32) -> i32 {
                x * 2
            }
        "#,
        );

        file_explorer.add_file(
            "../marathon_utils/src/sum.rs",
            r#"
            pub fn sum(x : i32, y : i32) -> i32 {
                x + y
            }
        "#,
        );
        let expected = expect_file!["outputs/several_libs_main.rs"];
        expected.assert_eq(&gen_code(&mut file_explorer));
    }
}
