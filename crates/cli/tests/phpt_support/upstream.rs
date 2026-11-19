//! Selected php-src tests that need the matching PHP 8.6 expectations.
//!
//! For example, the 8.6 stress test indexes its array by `spl_object_id()`.

use std::borrow::Cow;
use std::path::Path;

use phera_extension_api::LanguageLevel;
use phera_phpt::Phpt;

pub(crate) fn revision(path: &Path, level: LanguageLevel) -> Result<Option<Phpt>, String> {
    if level != LanguageLevel::Php86 {
        return Ok(None);
    }
    let root = phera_phpt::tests_dir();
    let Ok(relative) = path.strip_prefix(root.join("upstream")) else {
        return Ok(None);
    };
    let revision = root.join("phpt/php86-upstream").join(relative);
    match Phpt::parse_file(&revision) {
        Ok(revision) => Ok(Some(revision)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {}: {error}", revision.display())),
    }
}

pub(crate) fn source<'a>(file: &'a [u8], revision: Option<&Phpt>) -> Result<Cow<'a, [u8]>, String> {
    let Some(replacements) = revision.and_then(|revision| revision.section("FILE_REPLACE")) else {
        return Ok(Cow::Borrowed(file));
    };
    let mut file = std::str::from_utf8(file)
        .map_err(|error| format!("cannot apply FILE_REPLACE: {error}"))?
        .to_owned();
    for line in replacements.lines() {
        let (original, replacement) = line
            .split_once('\t')
            .filter(|(original, _)| !original.is_empty())
            .ok_or("FILE_REPLACE needs the old and new text separated by a tab")?;
        if !file.contains(original) {
            return Err(format!("FILE_REPLACE did not find {original:?}"));
        }
        file = file.replace(original, replacement);
    }
    Ok(Cow::Owned(file.into_bytes()))
}
