use std::path::PathBuf;

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let skills_dir = PathBuf::from(&manifest).join("skills");
    println!("cargo:rerun-if-changed=skills");

    let mut entries = Vec::new();
    if let Ok(files) = std::fs::read_dir(&skills_dir) {
        for entry in files.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let file_name = path.file_name().unwrap().to_string_lossy();
                let slug = path.file_stem().unwrap().to_string_lossy().into_owned();
                entries.push((slug, file_name.into_owned()));
            }
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let dest = out.join("bundled_skills_contents.rs");

    let mut contents = String::from("&[\n");
    for (slug, file_name) in entries {
        let slug = escape_str(&slug);
        let file_name = escape_str(&file_name);
        contents.push_str(&format!(
            "    (\"{slug}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/skills/{file_name}\"))),\n"
        ));
    }
    contents.push(']');

    std::fs::write(dest, contents).expect("write bundled_skills_contents.rs");
}
