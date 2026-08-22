use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let dist = manifest.join("../../web/dist");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("out dir"));
    let web_out = out.join("web");
    fs::create_dir_all(&web_out).expect("create web output");
    let mut files = Vec::new();
    if dist.is_dir() {
        copy_tree(&dist, &dist, &web_out, &mut files);
    } else {
        let index = "<!doctype html><html><body><main><h1>Graft Hub</h1><p>Build web assets with pnpm build.</p></main></body></html>";
        fs::write(web_out.join("index.html"), index).expect("write fallback index");
        files.push((
            "index.html".to_owned(),
            "text/html; charset=utf-8".to_owned(),
        ));
    }
    if files.is_empty() {
        let index = "<!doctype html><html><body><main><h1>Graft Hub</h1></main></body></html>";
        fs::write(web_out.join("index.html"), index).expect("write fallback index");
        files.push((
            "index.html".to_owned(),
            "text/html; charset=utf-8".to_owned(),
        ));
    }
    files.sort();
    let mut generated = String::from(
        "pub fn asset(path: &str) -> Option<(&'static [u8], &'static str)> { match path {",
    );
    for (path, mime) in files {
        let include_path = path.replace('\\', "/");
        generated.push_str(&format!(
            "\"{path}\" => Some((include_bytes!(concat!(env!(\"OUT_DIR\"), \"/web/{include_path}\")), \"{mime}\")),"
        ));
    }
    generated.push_str("_ => None, } }\n");
    fs::write(out.join("web_assets.rs"), generated).expect("write asset module");
    println!("cargo:rerun-if-changed=../../web/src");
    println!("cargo:rerun-if-changed=../../web/package.json");
    println!("cargo:rerun-if-changed=../../web/dist");
}

fn copy_tree(root: &Path, current: &Path, output: &Path, files: &mut Vec<(String, String)>) {
    let entries = fs::read_dir(current).expect("read web dist");
    for entry in entries {
        let entry = entry.expect("web dist entry");
        let path = entry.path();
        if path.is_dir() {
            copy_tree(root, &path, output, files);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("web relative path")
            .to_string_lossy()
            .replace('\\', "/");
        let destination = output.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).expect("create web asset parent");
        }
        fs::copy(&path, &destination).expect("copy web asset");
        files.push((relative.clone(), mime_for(&relative).to_owned()));
    }
}

fn mime_for(path: &str) -> &str {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("woff") | Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
