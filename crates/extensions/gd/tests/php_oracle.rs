//! Compare GD operation checksums, decoder channels and encoder interoperability.

use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::Path;

use phera_extension_api::testing::scripts::{Comparison, Runtime};

const LOSSLESS: &[&str] = &[
    "rgba.png",
    "indexed.png",
    "indexed.gif",
    "rgb.bmp",
    "indexed.bmp",
    "rgba-lossless.webp",
];
const LOSSY: &[&str] = &["gradient.jpeg", "rgba-lossy.webp"];

#[test]
fn gd_operations_match_php() {
    let Some(comparison) = Comparison::from_environment("gd-operations") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let operations = comparison
        .compare(&root.join("operations.php"), |_, _| {})
        .expect("compare GD drawing, geometry, filters, and built-in font pixels");
    println!(
        "{} operation checksums matched",
        operations.stdout.lines().count()
    );
    let inputs = root.join("corpus");
    let metadata = comparison
        .compare(&root.join("metadata.php"), |_, command| {
            command.arg(&inputs).args(LOSSLESS).args(LOSSY);
        })
        .expect("compare getimagesize and exif_imagetype with real PHP");
    println!("{} metadata cases matched", metadata.stdout.lines().count());
    let errors = comparison
        .compare(&root.join("errors.php"), |_, command| {
            command.arg(&inputs);
        })
        .expect("compare GD argument validation and warnings");
    println!(
        "{} GD diagnostic cases matched",
        errors.stdout.lines().count()
    );
}

#[test]
fn gd_codecs_match_php_pixels() {
    let Some(comparison) = Comparison::from_environment("gd-codecs") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let codec = root.join("codec.php");
    let wire = root.join("wire.php");
    let source = root.join("corpus");
    let lossless = comparison
        .compare(&codec, |_, command| {
            command.arg(&source).arg("--packed").args(LOSSLESS);
        })
        .expect("compare packed lossless pixels and palette indices");
    println!(
        "{} lossless packed rasters matched",
        lossless.stdout.lines().count()
    );
    let mut mismatches = Vec::new();
    let reference = decode(&comparison, Runtime::Php, &codec, &source, LOSSLESS, LOSSY);
    let decoded = decode(
        &comparison,
        Runtime::Phera,
        &codec,
        &source,
        LOSSLESS,
        LOSSY,
    );
    compare_pixels(&reference, &decoded, 2, &mut mismatches);
    let outputs = [
        "rgba.png",
        "interlaced.png",
        "interlaced-indexed.png",
        "indexed.gif",
        "indexed.bmp",
        "rgb.bmp",
        "rgba-lossless.webp",
        "stdout.png",
        "stream.png",
    ];
    let mut decoded = Vec::new();
    for runtime in [Runtime::Php, Runtime::Phera] {
        let destination = comparison.scratch_directory(&wire, runtime);
        let result = comparison
            .command(runtime, &wire)
            .arg(&source)
            .arg(&destination)
            .output()
            .expect("write cross-decoder samples");
        assert!(
            result.status.success() && result.stdout.is_empty() && result.stderr.is_empty(),
            "{} encoder script failed: {} {}",
            runtime.label(),
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        for name in outputs.iter().copied().chain(["gradient.jpeg"]) {
            let size = std::fs::metadata(destination.join(name))
                .expect("encoded file exists")
                .len();
            println!("{} encoded {name}: {size} bytes", runtime.label());
        }
        decoded.push(decode(
            &comparison,
            Runtime::Php,
            &codec,
            &destination,
            &outputs,
            &["gradient.jpeg"],
        ));
    }
    compare_pixels(&decoded[0], &decoded[1], 2, &mut mismatches);
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

struct Raster {
    dimensions: String,
    rgba: Vec<u8>,
    lossy: bool,
}

fn decode(
    comparison: &Comparison,
    runtime: Runtime,
    script: &Path,
    directory: &Path,
    lossless: &[&str],
    lossy: &[&str],
) -> BTreeMap<String, Raster> {
    let result = comparison
        .command(runtime, script)
        .arg(directory)
        .args(lossless)
        .args(lossy)
        .output()
        .expect("decode pixels");
    assert!(
        result.status.success() && result.stderr.is_empty(),
        "{} decode failed: {} {}",
        runtime.label(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let source = String::from_utf8(result.stdout).expect("ASCII pixel matrix");
    let lines = source.lines();
    assert_eq!(lines.clone().count(), lossless.len() + lossy.len());
    lines
        .map(|row| {
            let mut columns = row.split('\t');
            let name = columns.next().expect("filename");
            let dimensions = columns.next().expect("image dimensions").to_owned();
            let rgba = columns
                .next()
                .expect("pixels")
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|hex| u8::from_str_radix(std::str::from_utf8(hex).unwrap(), 16).unwrap())
                .collect();
            (
                name.into(),
                Raster {
                    dimensions,
                    rgba,
                    lossy: lossy.contains(&name),
                },
            )
        })
        .collect()
}

fn compare_pixels(
    reference: &BTreeMap<String, Raster>,
    result: &BTreeMap<String, Raster>,
    tolerance: u8,
    failures: &mut Vec<String>,
) {
    for (name, expected) in reference {
        let actual = &result[name];
        if actual.dimensions != expected.dimensions || actual.rgba.len() != expected.rgba.len() {
            failures.push(format!(
                "{name}: metadata {} versus {}",
                expected.dimensions, actual.dimensions
            ));
        }
        let mut channels = [0; 4];
        for (index, (&left, &right)) in expected.rgba.iter().zip(&actual.rgba).enumerate() {
            channels[index % 4] = channels[index % 4].max(left.abs_diff(right));
        }
        println!("{name}: largest RGBA differences {channels:?}");
        let limit = if expected.lossy { tolerance } else { 0 };
        if channels.iter().any(|&delta| delta > limit) {
            failures.push(format!(
                "{name}: RGBA differences {channels:?} exceed {limit}"
            ));
        }
    }
}
