**2024-11-16** A lot of breaking changes. To use it, use need:

- sudo apt install libxcb-shape0-dev libxcb-xfixes0-dev
- update templates/Cargo.toml to add extra "../" for algo_lib and rust_competitive_helper_util paths
- tester/helper.rs needs to handle properly that all tasks are created under ./tasks/

**2022-09-30** Improve the support of multi-file solutions. Save all files when archiving the task.

**2022-09-30** Support minimization of the generated code. Currently it just trims all spaces in each line. It is tunred off by default. To use it, in `build.rs` file, use code like:

```
fn main() {
    rust_competitive_helper_util::build::build_several_libraries(&["algo_lib".to_owned()], true);
}
```
