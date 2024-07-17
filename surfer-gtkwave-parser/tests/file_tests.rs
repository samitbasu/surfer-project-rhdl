use std::path::PathBuf;

fn parse_file(file: &str) {
    let path = PathBuf::from("tests/files").join(file);
    let s = std::fs::read_to_string(path).unwrap();
    let (dirs, errs) = surfer_gtkwave_parser::Parser::new(&s).parse();
    let snapshot = format!(
        "## Input ##\n\n{}\n## Errors ##\n\n{}\n\n## Directives ##\n\n{}",
        s,
        errs.iter()
            .map(|e| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join("\n"),
        dirs.iter()
            .map(|d| format!("{d:?}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(file);
    settings.bind(|| {
        insta::assert_snapshot!(snapshot);
    });
}

#[test]
fn with_8_bit_everything() {
    parse_file("with_8_bit_everything.gtkw");
}

#[test]
fn with_8_bit_one() {
    parse_file("with_8_bit_one.gtkw");
}
