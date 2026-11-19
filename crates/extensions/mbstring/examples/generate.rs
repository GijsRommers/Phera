//! Regenerate pinned tables: `cargo run -p phera-ext-mbstring --example generate`.
use phera_extension_api::{
    LanguageLevel,
    testing::oracle::{find_php_for, test_level},
};
use std::process::Command;
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let cjk = std::env::args().any(|argument| argument == "cjk");
    let level = if cjk {
        LanguageLevel::Php85
    } else {
        test_level()
    };
    let output = match level {
        LanguageLevel::Php85 => root.join("data"),
        LanguageLevel::Php86 => root.join("data/php86"),
    };
    std::fs::create_dir_all(&output).expect("create the table directory");
    let status = Command::new(find_php_for(level).expect("The selected PHP oracle is required"))
        .arg("-n")
        .arg(root.join(if cjk {
            "examples/generate-cjk.php"
        } else {
            "examples/generate.php"
        }))
        .arg(output)
        .status()
        .expect("run generator");
    assert!(status.success());
}
