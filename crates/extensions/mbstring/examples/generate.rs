//! Regenerate pinned tables: `cargo run -p phera-ext-mbstring --example generate`.
use phera_extension_api::testing::oracle::find_php;
use std::process::Command;
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let status = Command::new(find_php().expect("PHP 8.5 required"))
        .arg("-n")
        .arg(root.join(if std::env::args().any(|a| a == "cjk") {
            "examples/generate-cjk.php"
        } else {
            "examples/generate.php"
        }))
        .arg(root.join("data"))
        .status()
        .expect("run generator");
    assert!(status.success());
}
