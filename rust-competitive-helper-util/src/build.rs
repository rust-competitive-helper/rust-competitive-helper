use crate::new_build::Visitor;
use crate::{file_explorer::RealFileExplorer, old_build};
pub fn build_several_libraries(libraries: &[String], minimize: bool) {
    let file_explorer = RealFileExplorer::new();
    let code = old_build::build_several_libraries_impl(libraries, &file_explorer, minimize);

    crate::write_lines("../../main/src/main.rs", code);
    old_build::add_rerun_if_changed_instructions(libraries);
}

pub fn build() {
    build_several_libraries(&["algo_lib".to_owned()], false);
}

pub fn build_new(minimize: bool) {
    let mut visitor = Visitor::new(minimize, RealFileExplorer::new());
    visitor.build();
}
