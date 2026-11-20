//! Startup compiles Composer paths without executing either the maps or the
//! files they name, so `require __DIR__ . '/../src/Service.php';` still runs PHP.

use super::super::Served;

#[test]
fn composer_maps_are_preloaded_without_running_php() {
    let served = Served::start_in(
        "composer-preload",
        1,
        &[
            (
                "phera.toml",
                "[check]\npaths = [\"public\"]\n[server]\npreload = true\n",
            ),
            (
                "public/index.php",
                r#"<?php
echo require __DIR__ . '/../vendor/pkg/from-classmap.php';
echo require __DIR__ . '/../vendor/pkg/from-files.php';
echo require __DIR__ . '/../src/from-static.php';
"#,
            ),
            (
                "vendor/composer/autoload_classmap.php",
                r#"<?php
$vendorDir = dirname(__DIR__);
file_put_contents(__DIR__ . '/map-ran', 'bad');
return ['Demo' => $vendorDir . '/pkg/from-classmap.php'];
"#,
            ),
            (
                "vendor/composer/autoload_files.php",
                r#"<?php
$vendorDir = dirname(__DIR__);
return ['files' => $vendorDir . '/pkg/from-files.php'];
"#,
            ),
            (
                "vendor/composer/autoload_static.php",
                r#"<?php
namespace Composer\Autoload;
class ComposerStaticInitExample {
    public static $files = ['files' => __DIR__ . '/../pkg/from-files.php'];
    public static $classMap = ['DemoStatic' => __DIR__ . '/../..' . '/src/from-static.php'];
    public static $prefixDirsPsr4 = ['Demo\\' => __DIR__ . '/../../src'];
}
"#,
            ),
            ("vendor/pkg/from-classmap.php", "<?php return 'classmap ';"),
            ("vendor/pkg/from-files.php", "<?php return 'files ';"),
            (
                "src/from-static.php",
                "<?php file_put_contents(__DIR__ . '/file-ran', 'yes'); return 'static';",
            ),
        ],
        "public",
        &["-d", "opcache.validate_timestamps=0"],
    );
    assert!(
        served.banner.contains("preloaded 3 Composer files in"),
        "{}",
        served.banner
    );
    assert!(!served.root.join("vendor/composer/map-ran").exists());
    assert!(!served.root.join("src/file-ran").exists());
    for path in [
        "vendor/pkg/from-classmap.php",
        "vendor/pkg/from-files.php",
        "src/from-static.php",
    ] {
        std::fs::write(served.root.join(path), "<?php return 'edited';").unwrap();
    }
    assert_eq!(served.get("/"), (200, "classmap files static".into()));
    assert!(served.root.join("src/file-ran").exists());
}

#[test]
fn preload_flag_accepts_a_site_without_composer() {
    let served = Served::start_in(
        "no-composer-preload",
        1,
        &[("index.php", "<?php echo 'ready';")],
        ".",
        &["--preload"],
    );
    assert!(
        served.banner.contains("preloaded 0 Composer files in"),
        "{}",
        served.banner
    );
    assert_eq!(served.get("/"), (200, "ready".into()));
}
