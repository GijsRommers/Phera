//! Corpus-wide round-trip & golden-bytes verification for `.phb` images.
//!
//! Compiles PHP scripts from `tests/fixtures/`, `tests/phpt/`, and the Laravel
//! & Symfony warm-up fixtures, encodes them into a single `.phb` image, reopens
//! it, decodes & validates every unit, and asserts structural equality plus
//! byte-for-byte determinism.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use phera_aot::{
    AotImage, FileFlags, ImageFileEntry, ImageInput, ImageManifest, ImageTarget, blake2b_128,
    encode_image, runtime_abi_hash,
};
use phera_bytecode::CompiledUnit;
use phera_bytecode::facts::{BuildMode, DynamicAudit};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn collect_php_files(dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_php_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "php")
            && let Ok(bytes) = std::fs::read(&path)
        {
            out.push((path.to_string_lossy().into_owned(), bytes));
        }
    }
}

fn collect_phpt_files(dir: &Path, out: &mut Vec<(String, Vec<u8>)>, limit: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if out.len() >= limit {
            return;
        }
        if path.is_dir() {
            collect_phpt_files(&path, out, limit);
        } else if path.extension().is_some_and(|ext| ext == "phpt")
            && let Ok(raw) = std::fs::read(&path)
            && let Some(php) = extract_phpt_file_section(&raw)
        {
            out.push((path.to_string_lossy().into_owned(), php));
        }
    }
}

fn extract_phpt_file_section(raw: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut in_file = false;
    let mut body = String::new();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "--FILE--" {
            in_file = true;
            continue;
        }
        if in_file
            && trimmed.starts_with("--")
            && trimmed.ends_with("--")
            && trimmed.len() > 4
            && trimmed[2..trimmed.len() - 2]
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b == b'_')
        {
            break;
        }
        if in_file {
            body.push_str(line);
        }
    }
    (!body.is_empty()).then_some(body.into_bytes())
}

fn strip_metadata(unit: &CompiledUnit) -> CompiledUnit {
    let mut copy = unit.clone();
    copy.metadata = None;
    copy
}

#[test]
fn fixtures_and_phpt_corpus_round_trip_through_phb_image() {
    let root = workspace_root();
    let mut sources = Vec::new();
    collect_php_files(&root.join("tests"), &mut sources);
    collect_php_files(&root.join("crates/cli/tests/warmup_laravel"), &mut sources);
    collect_php_files(&root.join("crates/cli/tests/warmup_symfony"), &mut sources);
    collect_phpt_files(&root.join("tests"), &mut sources, usize::MAX);

    assert!(
        sources.len() >= 1000,
        "expected at least 1000 corpus files, found {}",
        sources.len()
    );

    let mut units = Vec::new();
    let mut files = Vec::new();
    for (idx, (path, bytes)) in sources.iter().enumerate() {
        // Intentional syntax-error tests are skipped cleanly.
        let Ok(unit) = phera_compiler::compile_bytes(bytes, path) else {
            continue;
        };
        let unit_id = units.len() as u32;
        units.push(Arc::new(unit));
        files.push(ImageFileEntry {
            relative_path: format!("corpus/{idx}.php"),
            canonical_path: path.clone(),
            unit_web: Some(unit_id),
            unit_cli: Some(unit_id),
            source_len: bytes.len() as u64,
            source_hash: blake2b_128(bytes),
            flags: FileFlags::NONE,
        });
    }

    assert!(
        units.len() >= 80,
        "expected at least 80 compilable corpus units, got {}",
        units.len()
    );

    let abi = runtime_abi_hash([0x42; 16]);
    let input = ImageInput {
        runtime_abi_hash: abi,
        manifest: ImageManifest {
            project_root: root.to_string_lossy().into_owned(),
            mode: BuildMode::Strict,
            has_web: true,
            has_cli: true,
            allow: Vec::new(),
            framework: None,
            entries: vec!["corpus/0.php".into()],
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            include_dev: false,
        },
        files,
        classmap: Vec::new(),
        units: units.clone(),
        program_facts: None,
        audit: DynamicAudit::default(),
    };

    let bytes_first = encode_image(&input).expect("encode first");
    let bytes_second = encode_image(&input).expect("encode second");
    assert_eq!(bytes_first, bytes_second, "encoding must be deterministic");

    let image = AotImage::from_bytes(bytes_first.into(), Some(abi)).expect("open image");
    assert_eq!(image.verify_all_units().expect("verify all"), units.len());

    for (idx, original) in units.iter().enumerate() {
        let decoded = image
            .load_unit_for_path(Path::new(&original.file), ImageTarget::Web)
            .expect("decode")
            .expect("present");
        assert_eq!(
            strip_metadata(&decoded),
            strip_metadata(original),
            "round-trip mismatch for corpus unit #{idx} ({})",
            original.file
        );
    }
}
