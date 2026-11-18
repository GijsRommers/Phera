//! Constants the engine itself defines.

use phera_extension_api::Registry;

const OS: &str = if cfg!(target_os = "macos") {
    "Darwin"
} else if cfg!(target_os = "windows") {
    "WINNT"
} else if cfg!(target_os = "freebsd") {
    "FreeBSD"
} else {
    "Linux"
};

const OS_FAMILY: &str = if cfg!(target_os = "macos") {
    "Darwin"
} else if cfg!(target_os = "windows") {
    "Windows"
} else if cfg!(target_os = "freebsd") {
    "BSD"
} else {
    "Linux"
};

const ERROR_LEVELS: &[(&str, i64)] = &[
    ("E_ERROR", 1),
    ("E_WARNING", 2),
    ("E_PARSE", 4),
    ("E_NOTICE", 8),
    ("E_CORE_ERROR", 16),
    ("E_CORE_WARNING", 32),
    ("E_COMPILE_ERROR", 64),
    ("E_COMPILE_WARNING", 128),
    ("E_USER_ERROR", 256),
    ("E_USER_WARNING", 512),
    ("E_USER_NOTICE", 1024),
    // No longer raised and no longer part of E_ALL, but old code names it.
    ("E_STRICT", 2048),
    ("E_RECOVERABLE_ERROR", 4096),
    ("E_DEPRECATED", 8192),
    ("E_USER_DEPRECATED", 16384),
    ("E_ALL", 30719),
];

pub(crate) fn register(registry: &mut Registry) {
    let mut version = phera_extension_api::PHP_VERSION
        .split('.')
        .map(|part| part.parse::<i64>().unwrap_or_default());
    let major = version.next().unwrap_or_default();
    let minor = version.next().unwrap_or_default();
    let release = version.next().unwrap_or_default();
    registry.constant("PHP_VERSION", phera_extension_api::PHP_VERSION);
    registry.constant("PHP_MAJOR_VERSION", major);
    registry.constant("PHP_MINOR_VERSION", minor);
    registry.constant("PHP_RELEASE_VERSION", release);
    registry.constant("PHP_VERSION_ID", major * 10_000 + minor * 100 + release);
    registry.constant("PHP_EXTRA_VERSION", "");
    registry.constant("PHP_DEBUG", false);
    registry.constant("PHP_ZTS", false);

    registry.constant("PHP_OS", OS);
    registry.constant("PHP_OS_FAMILY", OS_FAMILY);
    registry.constant("PHP_EOL", if cfg!(windows) { "\r\n" } else { "\n" });
    registry.constant(
        "DIRECTORY_SEPARATOR",
        if cfg!(windows) { "\\" } else { "/" },
    );
    registry.constant("PATH_SEPARATOR", if cfg!(windows) { ";" } else { ":" });
    registry.constant(
        "PHP_MAXPATHLEN",
        if cfg!(target_os = "linux") {
            4096
        } else {
            1024
        },
    );

    registry.constant("PHP_INT_MAX", i64::MAX);
    registry.constant("PHP_INT_MIN", i64::MIN);
    registry.constant("PHP_INT_SIZE", 8);
    registry.constant("PHP_FLOAT_EPSILON", f64::EPSILON);
    registry.constant("PHP_FLOAT_MAX", f64::MAX);
    registry.constant("PHP_FLOAT_MIN", f64::MIN_POSITIVE);
    registry.constant("PHP_FLOAT_DIG", 15);
    registry.constant("NAN", f64::NAN);
    registry.constant("INF", f64::INFINITY);

    for &(name, level) in ERROR_LEVELS {
        registry.constant(name, level);
    }
}
