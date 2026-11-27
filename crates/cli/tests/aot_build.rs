//! End-to-end tests for `phera build`, `phera run --image`, and `phera serve --image`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn phera_bin() -> &'static str {
    env!("CARGO_BIN_EXE_phera")
}

fn free_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    listener.local_addr().expect("local addr")
}

struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_http(addr: SocketAddr, path: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(mut stream) = TcpStream::connect(addr) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let req =
                format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
            if stream.write_all(req.as_bytes()).is_ok() {
                let mut buf = String::new();
                if stream.read_to_string(&mut buf).is_ok() && !buf.is_empty() {
                    return buf;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "server on {addr} did not answer {path} in time"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, content).expect("write fixture");
}

#[test]
fn build_and_run_strict_image_with_deleted_sources_and_eval_seal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_file(
        &root.join("src/Greeter.php"),
        r#"<?php
namespace App;

final class Greeter {
    public function __construct(private readonly string $prefix) {}
    public function greet(string $name): string {
        return $this->prefix . ', ' . $name . '!';
    }
}
"#,
    );
    write_file(
        &root.join("src/helper.php"),
        r#"<?php
require_once __DIR__ . '/Greeter.php';
function make_greeting(string $who): string {
    $g = new App\Greeter('Hello');
    return $g->greet($who);
}
"#,
    );
    write_file(
        &root.join("index.php"),
        r#"<?php
require_once __DIR__ . '/src/helper.php';
echo make_greeting($argv[1] ?? 'World'), "\n";
"#,
    );

    let image_path = root.join("dist/app.phb");
    let build = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "both",
            "--entry",
            "index.php",
            "--verify",
            "--audit=json",
            "-o",
        ])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("run phera build");
    assert!(
        build.status.success(),
        "build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(image_path.is_file(), "image file must exist");

    // Audit JSON parses cleanly.
    let audit_json: serde_json::Value =
        serde_json::from_slice(&build.stdout).expect("valid audit json");
    assert_eq!(audit_json["mode"], "strict");
    assert_eq!(audit_json["findings"].as_array().map(Vec::len), Some(0));

    // Delete ALL PHP source files to prove strict image runs with zero source on disk!
    std::fs::remove_file(root.join("index.php")).unwrap();
    std::fs::remove_dir_all(root.join("src")).unwrap();

    // Run default entry from image.
    let run = Command::new(phera_bin())
        .arg("run")
        .arg("--image")
        .arg(&image_path)
        .arg("index.php")
        .arg("Phera")
        .output()
        .expect("run from image");
    assert!(
        run.status.success(),
        "run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "Hello, Phera!\n");
}

#[test]
fn strict_mode_rejects_unallowed_eval_and_allows_when_permitted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_file(
        &root.join("index.php"),
        r#"<?php
try {
    $x = eval('return 21 * 2;');
    echo "eval=$x\n";
} catch (Phera\SealedImageException $e) {
    echo "sealed:" . $e->getMessage() . "\n";
}
"#,
    );

    let image_path = root.join("app.phb");
    // Without --allow eval, strict build fails at build time.
    let rejected = Command::new(phera_bin())
        .args(["build", "--mode", "strict", "--entry", "index.php", "-o"])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("strict build without allow");
    assert!(
        !rejected.status.success(),
        "strict build should reject unallowed eval"
    );
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("eval"),
        "stderr should mention eval: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );

    // With --allow eval, strict build succeeds and runtime permits eval.
    let allowed = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "cli",
            "--entry",
            "index.php",
            "--allow",
            "eval",
            "-o",
        ])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("strict build with allow eval");
    assert!(
        allowed.status.success(),
        "build with --allow eval failed: {}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let run = Command::new(phera_bin())
        .arg("run")
        .arg("--image")
        .arg(&image_path)
        .arg("index.php")
        .output()
        .expect("run allowed eval");
    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "eval=42\n");
}

#[test]
fn serve_from_strict_image_matches_source_and_survives_deleted_sources() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let public = root.join("public");

    write_file(
        &root.join("lib/math.php"),
        "<?php function fib(int $n): int { return $n <= 1 ? $n : fib($n - 1) + fib($n - 2); }\n",
    );
    write_file(
        &public.join("index.php"),
        r#"<?php
require_once dirname(__DIR__) . '/lib/math.php';
header('Content-Type: text/plain');
echo 'fib(10)=' . fib(10) . ' path=' . ($_SERVER['REQUEST_URI'] ?? '');
"#,
    );
    write_file(
        &root.join("phera.toml"),
        r#"
[build]
output = "build/site.phb"
mode = "strict"
entries = ["public/index.php"]
"#,
    );

    // Build using `[build]` from phera.toml.
    let build = Command::new(phera_bin())
        .args(["build", "--verify"])
        .arg(root)
        .output()
        .expect("build from phera.toml");
    assert!(
        build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let image_path = root.join("build/site.phb");
    assert!(image_path.is_file());

    // Delete PHP source files on disk!
    std::fs::remove_file(public.join("index.php")).unwrap();
    std::fs::remove_dir_all(root.join("lib")).unwrap();

    let addr = free_addr();
    let child = Command::new(phera_bin())
        .args(["serve", "--workers", "1", "--listen"])
        .arg(addr.to_string())
        .arg("--image")
        .arg(&image_path)
        .arg(&public)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn phera serve --image");
    let _guard = ServerGuard(child);

    let response = wait_for_http(addr, "/hello");
    assert!(
        response.contains("200 OK") && response.contains("fib(10)=55 path=/hello"),
        "unexpected HTTP response: {response}"
    );
}

#[test]
fn dev_mode_ignores_image_and_hybrid_recompiles_edited_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let public = root.join("public");

    write_file(
        &public.join("index.php"),
        "<?php header('Content-Type: text/plain'); echo 'v1';",
    );

    let image_path = root.join("app.phb");
    let build = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "hybrid",
            "--entry",
            "public/index.php",
            "-o",
        ])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("build hybrid");
    assert!(
        build.status.success(),
        "hybrid build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // Edit source on disk after build.
    write_file(
        &public.join("index.php"),
        "<?php header('Content-Type: text/plain'); echo 'v2-edited';",
    );

    // 1. Hybrid mode detects the edit and serves v2-edited.
    let addr1 = free_addr();
    let child1 = Command::new(phera_bin())
        .args(["serve", "--workers", "1", "--listen"])
        .arg(addr1.to_string())
        .arg("--image")
        .arg(&image_path)
        .arg(&public)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hybrid serve");
    let _guard1 = ServerGuard(child1);
    let resp1 = wait_for_http(addr1, "/");
    assert!(resp1.contains("v2-edited"), "hybrid response: {resp1}");
    drop(_guard1);

    // 2. --dev ignores --image and serves live disk source.
    let addr2 = free_addr();
    let child2 = Command::new(phera_bin())
        .args(["serve", "--workers", "1", "--dev", "--listen"])
        .arg(addr2.to_string())
        .arg("--image")
        .arg(&image_path)
        .arg(&public)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn dev serve");
    let _guard2 = ServerGuard(child2);
    let resp2 = wait_for_http(addr2, "/");
    assert!(resp2.contains("v2-edited"), "dev response: {resp2}");
}

#[test]
fn composer_psr4_and_autoload_files_discovery_and_audit_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_file(&root.join("composer.json"), r#"{"name":"demo/app"}"#);
    write_file(
        &root.join("vendor/composer/autoload_classmap.php"),
        "<?php\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\nreturn array();\n",
    );
    write_file(
        &root.join("vendor/composer/autoload_psr4.php"),
        "<?php\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\nreturn array('Demo\\\\' => array($baseDir . '/src'));\n",
    );
    write_file(
        &root.join("vendor/composer/autoload_files.php"),
        "<?php\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\nreturn array('h1' => $baseDir . '/src/helpers.php');\n",
    );
    write_file(
        &root.join("src/helpers.php"),
        "<?php function demo_helper(int $x): int { return $x + 1; }\n",
    );
    write_file(
        &root.join("src/App.php"),
        "<?php namespace Demo; final class App { public function run(): int { return demo_helper(41); } }\n",
    );
    write_file(
        &root.join("public/index.php"),
        "<?php echo (new Demo\\App())->run();\n",
    );

    let audit = Command::new(phera_bin())
        .args([
            "build",
            "--audit-only",
            "--audit=text",
            "--entry",
            "public/index.php",
        ])
        .arg(root)
        .output()
        .expect("audit-only");
    assert!(
        audit.status.success(),
        "audit-only failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&audit.stdout),
        String::from_utf8_lossy(&audit.stderr)
    );
    assert!(
        !root.join("var/phera/app.phb").exists(),
        "--audit-only must not write an image"
    );
    let report = String::from_utf8_lossy(&audit.stdout);
    assert!(
        report.contains("files") || report.contains("Files"),
        "unexpected audit text output: {report}"
    );
}

#[test]
fn strict_mode_seals_dynamic_class_alias_and_allows_when_permitted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_file(
        &root.join("direct.php"),
        "<?php class Orig {} class_alias('Orig', 'Alias');\n",
    );
    write_file(
        &root.join("dynamic.php"),
        r#"<?php
class Orig {}
try {
    call_user_func('class_alias', 'Orig', 'Alias');
    echo class_exists('Alias') ? "aliased\n" : "missing\n";
} catch (Phera\SealedImageException $e) {
    echo "sealed:" . $e->getMessage() . "\n";
}
"#,
    );

    let image_path = root.join("app.phb");

    // 1. Direct class_alias is rejected at build time in strict mode without --allow class_alias.
    let direct_rejected = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "cli",
            "--entry",
            "direct.php",
            "-o",
        ])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("strict build direct class_alias");
    assert!(
        !direct_rejected.status.success(),
        "direct class_alias should be rejected in strict mode"
    );
    assert!(
        String::from_utf8_lossy(&direct_rejected.stderr).contains("class_alias"),
        "stderr: {}",
        String::from_utf8_lossy(&direct_rejected.stderr)
    );

    // 2. Dynamic call_user_func('class_alias', ...) passes static audit, but is sealed at runtime!
    let sealed_build = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "cli",
            "--entry",
            "dynamic.php",
            "-o",
        ])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("strict build dynamic class_alias");
    assert!(
        sealed_build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&sealed_build.stderr)
    );
    let sealed_run = Command::new(phera_bin())
        .arg("run")
        .arg("--image")
        .arg(&image_path)
        .arg("dynamic.php")
        .output()
        .expect("run sealed dynamic class_alias");
    assert!(
        sealed_run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&sealed_run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&sealed_run.stdout),
        "sealed:class_alias() is disabled in strict AOT image mode\n"
    );

    // 3. With --allow class_alias, runtime permits class_alias (and drops SEALED_NAMES only).
    let allowed_build = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "cli",
            "--entry",
            "dynamic.php",
            "--allow",
            "class_alias",
            "--audit=json",
            "-o",
        ])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("strict build allow class_alias");
    assert!(
        allowed_build.status.success(),
        "allowed build failed: {}",
        String::from_utf8_lossy(&allowed_build.stderr)
    );
    let audit_json: serde_json::Value =
        serde_json::from_slice(&allowed_build.stdout).expect("audit json");
    assert_eq!(audit_json["holds"]["sealed_names"], false);
    assert_eq!(audit_json["holds"]["sealed_hierarchy"], true);

    let allowed_run = Command::new(phera_bin())
        .arg("run")
        .arg("--image")
        .arg(&image_path)
        .arg("dynamic.php")
        .output()
        .expect("run allowed dynamic class_alias");
    assert!(allowed_run.status.success());
    assert_eq!(String::from_utf8_lossy(&allowed_run.stdout), "aliased\n");
}

#[test]
fn strict_mode_allows_dynamic_include_within_image_and_blocks_external_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_file(
        &root.join("src/in_image.php"),
        "<?php return 'loaded-from-image';\n",
    );
    write_file(&root.join("outside.php"), "<?php return 'from-outside';\n");
    write_file(
        &root.join("index.php"),
        r#"<?php
class VirtStream {
    public $context;
    private int $pos = 0;
    private string $data = "<?php return 'injected-stream';";
    public function stream_open(string $path, string $mode, int $options, ?string &$opened_path): bool {
        $this->pos = 0;
        return true;
    }
    public function stream_read(int $count): string {
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos += strlen($chunk);
        return $chunk;
    }
    public function stream_eof(): bool {
        return $this->pos >= strlen($this->data);
    }
    public function stream_stat(): array {
        return ['size' => strlen($this->data)];
    }
}
stream_wrapper_register('virt', VirtStream::class);

function load_file(string $path) {
    return @include $path;
}
$a = load_file(__DIR__ . '/src/in_image.php');
$b = load_file(__DIR__ . '/outside.php');
$stream = 'none';
try {
    include 'virt://injected.php';
    $stream = 'ok';
} catch (Phera\SealedImageException $e) {
    $stream = 'sealed-stream';
}
echo 'a=' . ($a === false ? 'false' : $a)
    . ' b=' . ($b === false ? 'false' : $b)
    . ' stream=' . $stream . "\n";
"#,
    );

    let image_path = root.join("app.phb");
    let build = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "cli",
            "--entry",
            "index.php",
            "--include",
            "src/in_image.php",
            "--exclude",
            "outside.php",
            "--audit=json",
            "-o",
        ])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("strict build with dynamic include");
    assert!(
        build.status.success(),
        "strict build should accept dynamic_include (sealed by loader):\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let audit_json: serde_json::Value =
        serde_json::from_slice(&build.stdout).expect("valid audit json");
    assert_eq!(audit_json["holds"]["sealed_names"], true);
    assert_eq!(audit_json["holds"]["sealed_hierarchy"], true);
    assert!(
        audit_json["summary"]["dynamic_include"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "dynamic_include must still be reported in audit: {audit_json}"
    );

    // Delete built PHP files while leaving outside.php on disk.
    std::fs::remove_file(root.join("index.php")).unwrap();
    std::fs::remove_dir_all(root.join("src")).unwrap();

    let run = Command::new(phera_bin())
        .arg("run")
        .arg("--image")
        .arg(&image_path)
        .arg("index.php")
        .output()
        .expect("run strict dynamic include");
    assert!(
        run.status.success(),
        "run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "a=loaded-from-image b=false stream=sealed-stream\n"
    );
}

#[test]
fn serve_image_blocks_backslash_docroot_escape_and_hidden_scripts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let public = root.join("public");

    write_file(
        &public.join("index.php"),
        "<?php header('Content-Type: text/plain'); echo 'front-controller';",
    );
    write_file(&root.join("secret/admin.php"), "<?php echo 'SECRET-LEAK';");
    write_file(&public.join(".hidden/x.php"), "<?php echo 'HIDDEN-LEAK';");

    let image_path = root.join("app.phb");
    let build = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "web",
            "--entry",
            "public/index.php",
            "--include",
            "secret/admin.php",
            "--include",
            "public/.hidden/x.php",
            "-o",
        ])
        .arg(&image_path)
        .arg(root)
        .output()
        .expect("build strict web image");
    assert!(
        build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let addr = free_addr();
    let child = Command::new(phera_bin())
        .args(["serve", "--workers", "1", "--listen"])
        .arg(addr.to_string())
        .arg("--image")
        .arg(&image_path)
        .arg(&public)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn serve --image");
    let _guard = ServerGuard(child);

    let ok_resp = wait_for_http(addr, "/");
    assert!(ok_resp.contains("front-controller"), "root resp: {ok_resp}");

    // Backslash traversal outside document root must NEVER execute secret/admin.php.
    let escape_resp = wait_for_http(addr, "/x%5C..%5C..%5Csecret%5Cadmin.php");
    assert!(
        !escape_resp.contains("SECRET-LEAK"),
        "docroot escape executed secret script: {escape_resp}"
    );
    assert!(
        escape_resp.contains("404 Not Found"),
        "expected 404 for backslash path: {escape_resp}"
    );

    // Backslash traversal into hidden directory must NEVER execute .hidden/x.php.
    let hidden_resp = wait_for_http(addr, "/x%5C..%5C.hidden%5Cx.php");
    assert!(
        !hidden_resp.contains("HIDDEN-LEAK"),
        "hidden script executed: {hidden_resp}"
    );
    assert!(
        hidden_resp.contains("404 Not Found"),
        "expected 404 for hidden backslash path: {hidden_resp}"
    );
}

#[test]
fn strict_image_refuses_debug_and_rejects_wrong_target_and_unknown_toml_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_file(
        &root.join("index.php"),
        "#!/usr/bin/env php\n<?php echo PHP_SAPI, \"\\n\";\n",
    );
    let web_img = root.join("web.phb");
    let build_web = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "web",
            "--entry",
            "index.php",
            "-o",
        ])
        .arg(&web_img)
        .arg(root)
        .output()
        .expect("build strict web");
    assert!(build_web.status.success());

    // 1. Running a web-only strict image under CLI fails clearly (no cross-target fallback).
    let wrong_target = Command::new(phera_bin())
        .args(["run", "--image"])
        .arg(&web_img)
        .arg("index.php")
        .output()
        .expect("run web-only image");
    assert!(!wrong_target.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_target.stderr).contains("no CLI units"),
        "stderr: {}",
        String::from_utf8_lossy(&wrong_target.stderr)
    );

    // 2. Strict CLI image refuses XDEBUG_MODE=debug and --debug.
    let cli_img = root.join("cli.phb");
    let build_cli = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "strict",
            "--target",
            "cli",
            "--entry",
            "index.php",
            "-o",
        ])
        .arg(&cli_img)
        .arg(root)
        .output()
        .expect("build strict cli");
    assert!(build_cli.status.success());

    let debug_env = Command::new(phera_bin())
        .env("XDEBUG_MODE", "debug")
        .args(["run", "--image"])
        .arg(&cli_img)
        .arg("index.php")
        .output()
        .expect("run strict with XDEBUG_MODE");
    assert!(!debug_env.status.success());
    assert!(
        String::from_utf8_lossy(&debug_env.stderr).contains("debugging"),
        "stderr: {}",
        String::from_utf8_lossy(&debug_env.stderr)
    );

    // 3. `phera --image cli.phb` (without `run`) runs default CLI entry cleanly.
    let direct_img = Command::new(phera_bin())
        .arg("--image")
        .arg(&cli_img)
        .output()
        .expect("phera --image without run");
    assert!(
        direct_img.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&direct_img.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&direct_img.stdout), "cli\n");

    // 4. Unknown [build] key in phera.toml fails loudly.
    write_file(&root.join("phera.toml"), "[build]\nmod = \"strict\"\n");
    let bad_toml = Command::new(phera_bin())
        .args(["build", "--audit-only"])
        .arg(root)
        .output()
        .expect("build with typo in phera.toml");
    assert!(!bad_toml.status.success());
    assert!(
        String::from_utf8_lossy(&bad_toml.stderr).contains("unknown [build] key `mod`"),
        "stderr: {}",
        String::from_utf8_lossy(&bad_toml.stderr)
    );
}

#[test]
fn hybrid_cli_recompiles_edited_files_and_relative_includes_match_php() {
    let proj = tempfile::tempdir().expect("proj");
    let other = tempfile::tempdir().expect("other");
    let root = proj.path();

    write_file(&root.join("helpers.php"), "<?php return 'ROOT-HELPER';\n");
    write_file(
        &root.join("src/helpers.php"),
        "<?php return 'SRC-HELPER';\n",
    );
    write_file(
        &root.join("src/boot.php"),
        "<?php echo 'v1:', include 'helpers.php', \"\\n\";\n",
    );
    write_file(&root.join("lib/a.php"), "<?php echo 'IMAGE-LIB';\n");
    write_file(&other.path().join("lib/a.php"), "<?php echo 'CWD-LIB';\n");

    let img = root.join("hybrid.phb");
    let build = Command::new(phera_bin())
        .args([
            "build",
            "--mode",
            "hybrid",
            "--target",
            "cli",
            "--entry",
            "src/boot.php",
            "--include",
            "helpers.php",
            "--include",
            "src/helpers.php",
            "--include",
            "lib/a.php",
            "-o",
        ])
        .arg(&img)
        .arg(root)
        .output()
        .expect("build hybrid cli");
    assert!(
        build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // 1. From another CWD, `include 'helpers.php'` in `src/boot.php` must pick `src/helpers.php`
    //    (NOT the project-root `helpers.php`).
    let run1 = Command::new(phera_bin())
        .current_dir(other.path())
        .args(["run", "--image"])
        .arg(&img)
        .arg(root.join("src/boot.php"))
        .output()
        .expect("run boot from other cwd");
    assert!(
        run1.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run1.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run1.stdout), "v1:SRC-HELPER\n");

    // 2. From `other`, `phera run --image hybrid.phb lib/a.php` must run `other/lib/a.php`,
    //    NOT the image's `root/lib/a.php`.
    let run_cwd = Command::new(phera_bin())
        .current_dir(other.path())
        .args(["run", "--image"])
        .arg(&img)
        .arg("lib/a.php")
        .output()
        .expect("run local lib/a.php from other cwd");
    assert!(run_cwd.status.success());
    assert_eq!(String::from_utf8_lossy(&run_cwd.stdout), "CWD-LIB");

    // 3. Editing both the entry and the included file is detected in hybrid CLI mode!
    write_file(
        &root.join("src/helpers.php"),
        "<?php return 'SRC-HELPER-EDITED';\n",
    );
    write_file(
        &root.join("src/boot.php"),
        "<?php echo 'v2:', include 'helpers.php', \"\\n\";\n",
    );
    let run2 = Command::new(phera_bin())
        .current_dir(other.path())
        .args(["run", "--image"])
        .arg(&img)
        .arg(root.join("src/boot.php"))
        .output()
        .expect("run edited hybrid cli");
    assert!(run2.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run2.stdout),
        "v2:SRC-HELPER-EDITED\n"
    );
}

#[test]
fn serve_never_exposes_phb_image_as_static_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_file(
        &root.join("config.php"),
        "<?php define('DB_PASSWORD', 'probe-secret-7731');\n",
    );
    write_file(
        &root.join("index.php"),
        "<?php require_once __DIR__ . '/config.php'; header('Content-Type: text/plain'); echo 'ok';\n",
    );

    // Default output path: var/phera/app.phb (inside docroot when serving `.`).
    let build_default = Command::new(phera_bin())
        .args(["build", "--mode", "strict"])
        .arg(root)
        .output()
        .expect("build default phb");
    assert!(
        build_default.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build_default.stderr)
    );
    let default_img = root.join("var/phera/app.phb");
    assert!(default_img.is_file());

    // Also build a custom-extension image inside docroot to test explicit exclusion.
    let custom_img = root.join("var/custom.img");
    let build_custom = Command::new(phera_bin())
        .args(["build", "--mode", "strict", "-o"])
        .arg(&custom_img)
        .arg(root)
        .output()
        .expect("build custom img");
    assert!(build_custom.status.success());

    let addr = free_addr();
    let child = Command::new(phera_bin())
        .args(["serve", "--workers", "1", "--listen"])
        .arg(addr.to_string())
        .arg("--image")
        .arg(&custom_img)
        .arg(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn serve root");
    let _guard = ServerGuard(child);

    let ok_resp = wait_for_http(addr, "/");
    assert!(
        ok_resp.contains("200 OK") && ok_resp.contains("ok"),
        "{ok_resp}"
    );

    // Any .phb file in docroot must return 404 (never leak string literals!).
    let phb_resp = wait_for_http(addr, "/var/phera/app.phb");
    assert!(
        phb_resp.contains("404 Not Found") && !phb_resp.contains("probe-secret-7731"),
        "leaked .phb image: {phb_resp}"
    );

    // Custom-extension opened image is also excluded and must return 404.
    let custom_resp = wait_for_http(addr, "/var/custom.img");
    assert!(
        custom_resp.contains("404 Not Found") && !custom_resp.contains("probe-secret-7731"),
        "leaked custom image: {custom_resp}"
    );
}

#[test]
fn soundness_oracle_verifies_deep_facts_layouts_flow_calls_and_props_end_to_end() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    write_file(
        &root.join("src/types.php"),
        r#"<?php
namespace App;

trait MixA {
    public int $traitCount = 5;
    private string $traitSecret = 'ts';
    public function collide(): string { return 'A'; }
    public function readTraitSecret(): string { return $this->traitSecret; }
}

trait MixB {
    public function collide(): string { return 'B'; }
}

enum Color: string {
    case Red = 'red';
    case Blue = 'blue';
}

class Base {
    private int $secret = 10;
    protected int $shared = 20;
    public int $uninit;

    public function __construct(int $s = 10) {
        $this->secret = $s;
    }

    private function priv(): string {
        return 'base-priv:' . $this->secret;
    }

    public function callPriv(): string {
        return $this->priv();
    }

    public function mono(): int {
        return $this->shared;
    }

    public function poly(): string {
        return 'base';
    }

    public static function who(): string {
        return 'base-static';
    }
}

final class Child extends Base {
    use MixA, MixB {
        MixA::collide insteadof MixB;
        MixB::collide as collideB;
    }

    private string $secret = 'child-secret';
    public int $shared = 30;
    public string $hooked {
        get => 'hook:' . $this->shared;
    }

    public function __construct(int $baseSecret, int $shared) {
        parent::__construct($baseSecret);
        $this->shared = $shared;
    }

    public function priv(): string {
        return 'child-pub:' . $this->secret;
    }

    public function poly(): string {
        return 'child';
    }

    public function sumWith(int $delta = 1): int {
        $acc = $this->shared;
        for ($i = 0; $i < 3; $i++) {
            $acc = (int)($acc + $delta);
        }
        return $acc;
    }
}

class MagicBox {
    public int $x = 1;
    public function __get(string $name): mixed { return 99; }
    public function __set(string $name, mixed $val): void {}
}

class RebindTarget {
    public string $tag = 'rebound';
}

function cross_double(int $n): int {
    return (int)($n * 2);
}

function coerce_float(int $n): float {
    return $n;
}

function exact_identity(int $n): int {
    return $n;
}

function try_finally_ret(int $n): float {
    try {
        if ($n < 0) {
            throw new \RuntimeException('neg');
        }
        return $n;
    } catch (\RuntimeException $e) {
        return 0.5;
    } finally {
        $cleanup = $n + 1;
    }
}
"#,
    );

    write_file(
        &root.join("index.php"),
        r#"<?php
require __DIR__ . '/src/types.php';

use App\Base;
use App\Child;
use App\Color;
use App\MagicBox;
use App\RebindTarget;

function local_loop(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum = (int)($sum + $i);
    }
    return $sum;
}

function exercise_nullable(?Child $maybe): int {
    if ($maybe === null) {
        return -1;
    }
    $fallback = $maybe->uninit ?? 7;
    return (int)($maybe->shared + $fallback);
}

function exercise_instanceof(Base $b): string {
    if ($b instanceof Child) {
        return $b->priv() . '|' . $b->collide() . '|' . $b->collideB() . '|' . $b->readTraitSecret();
    }
    return $b->poly();
}

function exercise_overflow(): string {
    $max = PHP_INT_MAX;
    $over = $max + 1;
    $min = PHP_INT_MIN;
    $neg = -$min;
    $inc = PHP_INT_MAX;
    $inc++;
    return gettype($over) . ',' . gettype($neg) . ',' . gettype($inc);
}

function exercise_refs(): int {
    $a = 1;
    $b = &$a;
    $b = 'mutated';
    $arr = [1, 2, 3];
    foreach ($arr as &$v) {
        $v = $v * 10;
    }
    unset($v);
    $c = 0;
    $inc = function () use (&$c): void { $c++; };
    $inc();
    return strlen($a) + $arr[2] + $c;
}

function exercise_closure_rebind(): string {
    $child = new Child(11, 22);
    $fn = (function (): string {
        return $this->tag ?? 'no-tag';
    })->bindTo(new RebindTarget(), RebindTarget::class);
    return $fn() . ':' . $child->hooked;
}

function exercise_gmp(): string {
    $g = gmp_init(5);
    $r = $g + 1;
    return get_class($r) . ':' . gmp_strval($r);
}

function exercise_reflection_dynamic_extract(): string {
    try {
        (new ReflectionFunction('extract'))->invoke(['k' => 1]);
        return 'unexpected';
    } catch (Error $e) {
        return $e->getMessage();
    }
}

$child = new Child(11, 30);
$base = new Base(5);
$magic = new MagicBox();
$color = Color::Red;

echo implode("\n", [
    'priv=' . $child->callPriv(),
    'mono=' . $child->mono() . ',' . $base->mono(),
    'poly=' . $child->poly() . ',' . $base->poly(),
    'static=' . Child::who(),
    'sum=' . $child->sumWith() . ',' . $child->sumWith(2),
    'cross=' . App\cross_double(21) . ',' . App\exact_identity(9) . ',' . App\coerce_float(3),
    'tf=' . App\try_finally_ret(4) . ',' . App\try_finally_ret(-1),
    'loop=' . local_loop(10),
    'null=' . exercise_nullable(null) . ',' . exercise_nullable($child),
    'inst=' . exercise_instanceof($child) . ';' . exercise_instanceof($base),
    'over=' . exercise_overflow(),
    'refs=' . exercise_refs(),
    'rebind=' . exercise_closure_rebind(),
    'magic=' . $magic->x . ',' . $magic->missing,
    'enum=' . $color->name . ':' . $color->value,
    'gmp=' . exercise_gmp(),
    'refl=' . exercise_reflection_dynamic_extract(),
]), "\n";
"#,
    );

    let expected_stdout = "\
priv=base-priv:11
mono=30,20
poly=child,base
static=base-static
sum=33,36
cross=42,9,3
tf=4,0.5
loop=45
null=-1,37
inst=child-pub:child-secret|A|B|ts;base
over=double,double,double
refs=38
rebind=rebound:hook:22
magic=1,99
enum=Red:red
gmp=GMP:6
refl=Cannot call extract() dynamically
";

    for mode in ["strict", "hybrid"] {
        let image_path = root.join(format!("oracle-{mode}.phb"));
        let build = Command::new(phera_bin())
            .args([
                "build",
                "--mode",
                mode,
                "--target",
                "cli",
                "--entry",
                "index.php",
                "-o",
            ])
            .arg(&image_path)
            .arg(root)
            .output()
            .expect("build oracle image");
        assert!(
            build.status.success(),
            "build ({mode}) failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );

        let image = phera_runtime::image::open_image(&image_path).expect("open .phb image");
        let facts = image.program_facts().expect("program_facts present");

        // Verify exact class layouts and method tables were populated.
        let child_layout = facts
            .classes
            .iter()
            .find(|c| &*c.name == "app\\child")
            .expect("app\\child class layout");
        assert!(
            child_layout
                .flags
                .contains(phera_bytecode::facts::ClassFlags::LAYOUT_EXACT),
            "Child should have LAYOUT_EXACT"
        );
        let slot_keys: Vec<&[u8]> = child_layout.slots.iter().map(|s| &*s.key).collect();
        assert_eq!(
            slot_keys,
            vec![
                &b"\0App\\Base\0secret"[..],
                &b"shared"[..],
                &b"uninit"[..],
                &b"\0App\\Child\0secret"[..],
                &b"hooked"[..],
                &b"traitCount"[..],
                &b"\0App\\Child\0traitSecret"[..],
            ],
            "Child root-first mangled slot keys must match PHP runtime layout"
        );
        let method_names: Vec<&str> = child_layout.methods.iter().map(|m| &*m.name).collect();
        assert!(method_names.contains(&"callpriv"), "{method_names:?}");
        assert!(method_names.contains(&"collide"), "{method_names:?}");
        assert!(method_names.contains(&"collideb"), "{method_names:?}");
        assert!(
            method_names.contains(&"readtraitsecret"),
            "{method_names:?}"
        );

        // Verify `RETURN_PROVEN` is true for `exact_identity` and false for `coerce_float`.
        let types_entry = image
            .find_relative("src/types.php")
            .expect("src/types.php in image");
        let types_unit = image
            .load_unit(types_entry.unit_cli.expect("cli unit"))
            .expect("decode src/types.php");
        let types_facts = types_unit.facts.as_deref().expect("unit facts");
        let find_fn_facts = |name: &str| {
            let pos = types_unit
                .functions
                .iter()
                .position(|f| f.name.eq_ignore_ascii_case(name))
                .unwrap_or_else(|| panic!("missing function {name}"));
            &types_facts.functions[pos + 1]
        };
        assert!(
            find_fn_facts("App\\exact_identity")
                .flags
                .contains(phera_bytecode::facts::FunctionFlags::RETURN_PROVEN),
            "exact_identity should have RETURN_PROVEN"
        );
        assert!(
            !find_fn_facts("App\\coerce_float")
                .flags
                .contains(phera_bytecode::facts::FunctionFlags::RETURN_PROVEN),
            "coerce_float coerces int -> float and must NOT have RETURN_PROVEN"
        );

        // Execute the `.phb` image through the cold-path VM Soundness Oracle!
        let rt = phera_runtime::Runtime::new()
            .with_image(image)
            .with_soundness_oracle(true);
        let tokio_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let entry_path = root.join("index.php");
        let run = tokio_rt
            .block_on(rt.run_file(&entry_path, &[], None))
            .expect("run under soundness oracle");
        assert_eq!(
            run.status,
            0,
            "mode={mode} output:\n{}",
            String::from_utf8_lossy(&run.output)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.output),
            expected_stdout,
            "mode={mode} stdout mismatch"
        );

        let report = run.oracle_report.expect("soundness oracle report present");
        assert!(
            report.is_clean(),
            "mode={mode} SOUNDNESS VIOLATIONS:\n{}",
            report.violations.join("\n")
        );
        assert!(
            report.stats.steps > 100,
            "mode={mode} stats: {:?}",
            report.stats
        );
        assert!(
            report.stats.non_ref_local_checks > 0,
            "mode={mode} stats: {:?}",
            report.stats
        );
        assert!(
            report.stats.block_type_checks > 0,
            "mode={mode} stats: {:?}",
            report.stats
        );
        assert!(
            report.stats.return_checks > 0,
            "mode={mode} stats: {:?}",
            report.stats
        );
        assert!(
            report.stats.call_checks > 0,
            "mode={mode} stats: {:?}",
            report.stats
        );
        if mode == "strict" {
            assert!(
                report.stats.args_exact_checks > 0,
                "strict stats: {:?}",
                report.stats
            );
            assert!(
                report.stats.prop_checks > 0,
                "strict stats: {:?}",
                report.stats
            );
            assert!(
                report.stats.class_layout_checks >= 2,
                "strict stats: {:?}",
                report.stats
            );
        }
    }
}
