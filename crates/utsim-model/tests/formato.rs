//! Every shipped example declares or implies format version 1.

use utsim_model::{FORMAT_VERSION, Model};

#[test]
fn examples_are_format_1() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "ron") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        // Experiments and optimisation problems are not models.
        let first = text.lines().find(|l| !l.trim_start().starts_with("//"));
        if !first.is_some_and(|l| l.starts_with("Model")) {
            continue;
        }
        let model = Model::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(model.format.unwrap_or(1), 1, "{}", path.display());
        checked += 1;
    }
    assert_eq!(FORMAT_VERSION, 1);
    assert!(checked >= 10, "só {checked} exemplos verificados");
}
