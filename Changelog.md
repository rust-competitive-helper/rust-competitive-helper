**2022-09-30** Improve the support of multi-file solutions. Save all files when archiving the task.

**2022-09-30** Support minimization of the generated code. Currently it just trims all spaces in each line. It is tunred off by default. To use it, in `build.rs` file, use code like:

```
fn main() {
    rust_competitive_helper_util::build::build_several_libraries(&["algo_lib".to_owned()], true);
}
```
