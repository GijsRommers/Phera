//! Generate analyzer-only signatures for extensions such as APCu and phpredis.
//! PHPStan's bundled PhpStorm stubs describe these APIs even when PHP lacks them.

use crate::{builtins, collect, index::Index};
use std::path::Path;

const EXTENSIONS: &[&str] = &[
    "amqp",
    "apcu",
    "frankenphp",
    "igbinary",
    "imagick",
    "memcached",
    "mongodb",
    "redis",
    "relay",
    "swoole",
    "xdebug",
];

pub fn generate(root: &Path, reference: &str) -> Result<String, String> {
    let mut index = Index::default();
    let mut files = Vec::new();
    let mut pending: Vec<_> = EXTENSIONS
        .iter()
        .map(|extension| root.join(extension))
        .collect();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in
                std::fs::read_dir(&path).map_err(|error| format!("{}: {error}", path.display()))?
            {
                pending.push(entry.map_err(|error| error.to_string())?.path());
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension == "stub")
        {
            files.push(path);
        }
    }
    files.sort();
    for path in &files {
        let file = path.strip_prefix(root).map_err(|error| error.to_string())?;
        let file = format!("phpstorm-stubs/{}", file.display());
        let source = std::fs::read_to_string(path).map_err(|error| format!("{file}: {error}"))?;
        let program = phera_parser::parse(&source).map_err(|error| {
            let line = phera_source::LineTable::new(&source).line(error.span.start);
            format!("{file}:{line}: {}", error.message)
        })?;
        collect::collect(&mut index, &file, &source, &program);
    }
    if files.is_empty() {
        return Err(format!("No extension stubs in {}", root.display()));
    }
    let data = format!(
        "# JetBrains/phpstorm-stubs {reference} (Apache-2.0), bundled in PHPStan\n{}",
        builtins::encode(&index)
    );
    let destination = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/optional-extensions.tsv");
    std::fs::write(destination, data).map_err(|error| error.to_string())?;
    Ok(format!(
        "Read {} optional extension stub files.\n",
        files.len()
    ))
}
