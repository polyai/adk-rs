// Build scripts run before crate dependencies are available, so this cannot use
// `adk-io` while generating Cargo OUT_DIR include metadata.
#![allow(clippy::disallowed_methods)]

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let template_dir = manifest_dir.join("python-gen-template");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));

    println!("cargo:rerun-if-changed={}", template_dir.display());
    let mut files = Vec::new();
    collect_python_files(&template_dir, &template_dir, &mut files)?;
    files.sort();

    let mut generated = String::new();
    generated.push_str("pub(crate) const PYTHON_GEN_TEMPLATE_FILES: &[(&str, &str)] = &[\n");
    for rel in files {
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        generated.push_str("    (\n");
        generated.push_str(&format!("        {rel_str:?},\n"));
        generated.push_str(&format!(
            "        include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/python-gen-template/{rel_str}\")),\n"
        ));
        generated.push_str("    ),\n");
    }
    generated.push_str("];\n");

    fs::write(out_dir.join("python_gen_template_files.rs"), generated)
}

fn collect_python_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            collect_python_files(root, &path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "py") {
            files.push(
                path.strip_prefix(root)
                    .expect("relative template")
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}
