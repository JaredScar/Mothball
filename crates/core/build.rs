use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let rules_dir = manifest.join("../../rules");
    println!("cargo:rerun-if-changed={}", rules_dir.display());

    let mut files: Vec<_> = fs::read_dir(&rules_dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", rules_dir.display()))
        .map(|entry| entry.expect("read rules entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .collect();
    files.sort();

    let mut out = String::from("pub static FILES: &[(&str, &str)] = &[\n");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        out.push_str(&format!("    ({name:?}, {content:?}),\n"));
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("builtin_rules.rs");
    fs::write(dest, out).unwrap();
}
