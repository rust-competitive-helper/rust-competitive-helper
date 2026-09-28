use super::*;
use crate::file_explorer::FakeFileExplorer;

#[test]
fn deleting_macro_import_preserves_following_import() {
    let source = "#[allow(unused)]\nuse solution::dbg;\nuse std::fmt::Debug;\n";
    let mut visitor = Visitor::new(false, FakeFileExplorer::new());
    visitor.cur_line_index = Some(LineIndex::new(source));
    // The built-in solution namespace keeps this test independent of the filesystem.
    visitor.content.insert(
        "solution".into(),
        Library {
            macros: HashMap::from([(
                "dbg".into(),
                File {
                    path: "src/dbg.rs".into(),
                    fqn: vec!["solution".into(), "dbg".into()],
                },
            )]),
            root: Module {
                name: "solution".into(),
                children: BTreeMap::new(),
                file: None,
                source: None,
                edits: Vec::new(),
            },
        },
    );

    visitor.visit_file_mut(&mut syn::parse_file(source).unwrap());

    assert_eq!(
        apply_edits(source, &visitor.cur_edits),
        "\nuse std::fmt::Debug;\n"
    );
    assert_eq!(visitor.queue.len(), 2); // The macro definition is still collected.
}
