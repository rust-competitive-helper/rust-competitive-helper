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
            "../algo_lib/src/geometry/convex_polygon_intersection.rs",
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
            "../algo_lib/src/geometry/half_plane_intersection.rs",
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
}
