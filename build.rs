use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const VENCORD_JS_URL: &str =
    "https://github.com/Vendicated/Vencord/releases/download/devbuild/browser.js";
const VENCORD_CSS_URL: &str =
    "https://github.com/Vendicated/Vencord/releases/download/devbuild/browser.css";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VENCORD_REFRESH");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set")).join("vencord");
    fs::create_dir_all(&out_dir).expect("create vencord dir");

    let force = env::var("VENCORD_REFRESH").is_ok();
    let js_path = out_dir.join("browser.js");
    fetch(&js_path, VENCORD_JS_URL, force);
    fetch(&out_dir.join("browser.css"), VENCORD_CSS_URL, force);
    patch_menu_api_regex(&js_path);
}

// Vencord ships several webpack-patch regexes with unbounded `.+?`/`.+`
// inside variable-length lookbehinds. On JSC (WebKit) these evaluate in
// O(n²) against minified Discord module source — measured 4.25s + 2.47s
// per cold start. Bounded quantifiers preserve match semantics (Discord's
// modules are local enough that the bounds always hold) and evaluate
// roughly linearly.
fn patch_menu_api_regex(path: &Path) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    let rules: &[(&str, &str, &str)] = &[
        // observed 4.25s pre-bound
        (
            r".{0,50}?navigable:.+Menu API",
            r".{0,50}?navigable:.{1,500}?Menu API",
            "Menu API: greedy .+ → bounded .{1,500}?",
        ),
        (
            r".{0,50}?navigable:.+?Menu API",
            r".{0,50}?navigable:.{1,500}?Menu API",
            "Menu API: lazy .+? → bounded .{1,500}?",
        ),
        (
            r"Menu API).+?)}",
            r"Menu API).{1,500}?)}",
            "Menu API outer .+? → bounded .{1,500}?",
        ),
        // observed 2.47s pre-bound
        (
            r"canCopyImage\(.+?)typeof",
            r"canCopyImage\(.{1,200}?)typeof",
            "canCopyImage: .+? → bounded .{1,200}?",
        ),
    ];
    let mut patched = content;
    let mut applied: Vec<&str> = Vec::new();
    for (bad, good, label) in rules {
        if patched.contains(bad) {
            patched = patched.replacen(bad, good, 1);
            applied.push(label);
        }
    }
    if applied.is_empty() {
        return;
    }
    // Failing to write means the next compile silently embeds the
    // un-patched bundle and reintroduces the multi-second stall.
    fs::write(path, patched).expect("rewrite vencord browser.js");
    for label in applied {
        println!("cargo:warning=Patched Vencord browser.js: {label}");
    }
}

fn fetch(path: &Path, url: &str, force: bool) {
    if path.exists() && !force {
        return;
    }
    let status = Command::new("curl")
        .args(["-sSfL", "--retry", "3", "-o"])
        .arg(path)
        .arg(url)
        .status()
        .expect("curl not found in PATH (required to fetch the Vencord browser bundle at build time)");
    if !status.success() {
        panic!("curl failed downloading {url}");
    }
    let len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if len < 1024 {
        panic!(
            "{} is only {} bytes after download — refusing to embed a likely error page",
            path.display(),
            len
        );
    }
}
