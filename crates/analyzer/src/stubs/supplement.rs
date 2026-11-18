//! Some native registrations have no stub: `STDERR` and dynamically named MHASH constants.
//! Read their PHP sources where possible; keep the small remaining language facts explicit.

use crate::{
    index::{ConstantInfo, Index, Location, Signature},
    stubs::source::StubSource,
};

pub fn add(index: &mut Index, source: &StubSource<'_>) -> Result<usize, String> {
    let before = index.classes.len() + index.constants.len();
    for (name, file) in [
        ("STDIN", "sapi/cli/php_cli.c"),
        ("STDOUT", "sapi/cli/php_cli.c"),
        ("STDERR", "sapi/cli/php_cli.c"),
        ("PHP_CLI_PROCESS_TITLE", "sapi/cli/php_cli.c"),
        ("IMAGETYPE_SVG", "ext/libxml/image_svg.c"),
    ] {
        index.constants.insert(
            name.into(),
            ConstantInfo::untyped(native_location(
                source,
                file,
                if name == "IMAGETYPE_SVG" {
                    "PHP_IMAGE_CONST_NAME(\"SVG\")"
                } else {
                    name
                },
            )?),
        );
    }
    add_mhash(index, source)?;
    if let Some(class) = index.classes.get_mut("closure") {
        let mut signature = Signature::new(
            &[],
            None,
            &crate::names::Names::default(),
            native_location(
                source,
                "Zend/zend_closures.c",
                "ZEND_METHOD(Closure, __invoke)",
            )?,
        );
        signature.accepts_extra = true;
        class.methods.insert("__invoke".into(), signature);
    }
    if let Some(class) = index.classes.get_mut("stdclass") {
        class.open_properties = true;
    }
    for name in ["clone", "exit", "die"] {
        if let Some(signature) = index.functions.remove(&format!("phera_stub_{name}")) {
            index.functions.insert(name.into(), signature);
        }
    }
    Ok(index.classes.len() + index.constants.len() - before)
}

pub fn aliases(index: &mut Index, file: &str, source: &str) -> usize {
    let classes: Vec<_> = index
        .classes
        .values()
        .filter(|c| c.location.file == file)
        .cloned()
        .collect();
    let mut added = 0;
    let lines: Vec<_> = source.lines().collect();
    for class in classes {
        let start = class.location.line.saturating_sub(1);
        let before = lines[..start.min(lines.len())].join("\n");
        let Some(comment) = before.rsplit("/**").next() else {
            continue;
        };
        let Some((_, alias)) = comment.split_once("@alias ") else {
            continue;
        };
        let Some((name, rest)) = alias.split_once(char::is_whitespace) else {
            continue;
        };
        let Some((_, after)) = rest.split_once("*/") else {
            continue;
        };
        if after.contains([';', '{', '}']) {
            continue;
        }
        let mut alias = class.clone();
        alias.name = name.into();
        if index
            .classes
            .insert(name.to_ascii_lowercase(), alias)
            .is_none()
        {
            added += 1;
        }
    }
    added
}

fn native_location(source: &StubSource<'_>, file: &str, needle: &str) -> Result<Location, String> {
    let text = source.read(file)?;
    let line = text
        .lines()
        .position(|line| line.contains(needle))
        .ok_or_else(|| format!("Missing native registration {needle} in {file}"))?
        + 1;
    Ok(Location {
        file: file.into(),
        line,
    })
}

fn add_mhash(index: &mut Index, source: &StubSource<'_>) -> Result<(), String> {
    let hash = source.read("ext/hash/hash.c")?;
    let mut table = false;
    for (line, text) in hash.lines().enumerate() {
        if text.contains("mhash_to_hash[MHASH_NUM_ALGOS] = {") {
            table = true;
            continue;
        }
        if table && text.starts_with("};") {
            break;
        }
        if table
            && let Some(text) = text.trim().strip_prefix("{\"")
            && let Some((name, _)) = text.split_once('"')
        {
            index.constants.insert(
                format!("MHASH_{name}"),
                ConstantInfo::untyped(Location {
                    file: "ext/hash/hash.c".into(),
                    line: line + 1,
                }),
            );
        }
    }
    Ok(())
}
