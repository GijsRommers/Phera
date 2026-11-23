//! Extracts PHP seed files from `.phpt` tests (the `--FILE--` section) and
//! from plain `.php` / `.inc` files, into a corpus directory.

use std::fs;
use std::path::{Path, PathBuf};

/// Sections of a .phpt are introduced by a line `--NAME--`.
fn file_section(text: &str) -> Option<String> {
    let mut in_file = false;
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim_end_matches(['\r']);
        if let Some(name) = trimmed.strip_prefix("--").and_then(|s| s.strip_suffix("--")) {
            if in_file {
                return Some(out);
            }
            in_file = name.eq_ignore_ascii_case("FILE");
            continue;
        }
        if in_file {
            out.push_str(line);
            out.push('\n');
        }
    }
    if in_file {
        Some(out)
    } else {
        None
    }
}

/// Collects seeds from `roots` into `corpus_dir`, one file per seed.
/// Returns the number of seeds written.
pub fn collect(roots: &[PathBuf], corpus_dir: &Path) -> std::io::Result<usize> {
    fs::create_dir_all(corpus_dir)?;
    let mut count = 0;
    for root in roots {
        collect_dir(root, corpus_dir, &mut count)?;
    }
    Ok(count)
}

fn collect_dir(dir: &Path, corpus_dir: &Path, count: &mut usize) -> std::io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dir(&path, corpus_dir, count)?;
        } else {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let text = match fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let seed = if ext == "phpt" {
                file_section(&text)
            } else if ext == "php" || ext == "inc" {
                Some(text)
            } else {
                None
            };
            if let Some(seed) = seed {
                if seed.len() > 512 * 1024 {
                    continue;
                }
                let name = format!("seed-{:06}.php", *count);
                fs::write(corpus_dir.join(name), seed)?;
                *count += 1;
            }
        }
    }
    Ok(())
}

/// Loads all seeds from a corpus directory.
pub fn load(corpus_dir: &Path) -> Vec<Vec<u8>> {
    let mut seeds = Vec::new();
    if let Ok(entries) = fs::read_dir(corpus_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("php") {
                if let Ok(bytes) = fs::read(&path) {
                    if bytes.len() <= 256 * 1024 {
                        seeds.push(bytes);
                    }
                }
            }
        }
    }
    seeds.sort();
    seeds
}
