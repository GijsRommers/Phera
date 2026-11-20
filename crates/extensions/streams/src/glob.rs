//! `glob('src/{test,main}/*.php', GLOB_BRACE)` expands braces and wildcards.

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::errors::{check_path, warn_false};
use crate::io_policy::IoPolicy;
use crate::pattern::{InvalidClass, Options};
use crate::request::{directory, path};

// PHP uses glibc's glob flags on Linux and the bundled BSD glob elsewhere.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const FLAGS: [i64; 7] = [1024, 1, 2, 16, 64, 4, 8192];
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
const FLAGS: [i64; 7] = [128, 4, 8, 16, 4096, 32, 1 << 30];
const AVAILABLE: i64 = FLAGS[0] | FLAGS[1] | FLAGS[2] | FLAGS[3] | FLAGS[4] | FLAGS[5] | FLAGS[6];

pub fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("pattern", ParamType::String),
        Param::new("flags", ParamType::Int).optional(),
    ];
    registry.function(FunctionEntry::asynchronous("glob", PARAMS, glob));
    for (name, flag) in [
        "GLOB_BRACE",
        "GLOB_ERR",
        "GLOB_MARK",
        "GLOB_NOCHECK",
        "GLOB_NOESCAPE",
        "GLOB_NOSORT",
        "GLOB_ONLYDIR",
    ]
    .into_iter()
    .zip(FLAGS)
    {
        registry.constant(name, flag);
    }
    registry.constant("GLOB_AVAILABLE_FLAGS", AVAILABLE);
}

struct Expansion {
    pattern: Options,
    braces: bool,
    errors: bool,
    mark: bool,
    no_check: bool,
    sort: bool,
    directories: bool,
}

fn glob(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let pattern = args::bytes(&args, 0);
        check_path("glob", "#1 ($pattern)", pattern)?;
        let flags = args::int(&args, 1).unwrap_or_default();
        if flags & !AVAILABLE != 0 {
            return Err(NativeError::value_error(
                "glob(): Argument #2 ($flags) must be a valid flag value",
            ));
        }
        let limit = if cfg!(target_os = "linux") {
            4096
        } else {
            1024
        };
        if pattern.len() >= limit {
            return warn_false(
                context,
                "glob(): Pattern exceeds the maximum allowed length".into(),
            );
        }
        let Some(cwd) = directory(context).path().map(Path::to_owned) else {
            return Ok(Value::Bool(false));
        };
        let [
            braces,
            errors,
            mark,
            no_check,
            no_escape,
            no_sort,
            directories,
        ] = FLAGS.map(|flag| flags & flag != 0);
        let expansion = Expansion {
            pattern: Options {
                escape: !no_escape,
                path: false,
                period: true,
                fold: false,
                invalid_class: InvalidClass::Literal,
            },
            braces,
            errors,
            mark,
            no_check,
            sort: !no_sort,
            directories,
        };
        let pattern = pattern.to_vec();
        let entries = IoPolicy::Blocking
            .run(move || expansion.expand(&cwd, pattern))
            .await?;
        let Ok(entries) = entries else {
            return Ok(Value::Bool(false));
        };
        let mut result = PhpArray::new();
        for entry in entries {
            result.push(Value::from(entry))?;
        }
        Ok(Value::from(result))
    })
}

impl Expansion {
    fn expand(&self, cwd: &Path, pattern: Vec<u8>) -> io::Result<Vec<Vec<u8>>> {
        let mut pending = vec![pattern];
        let mut entries = Vec::new();
        while let Some(pattern) = pending.pop() {
            if self.braces
                && let Some(alternatives) = split_brace(&pattern, self.pattern.escape)
            {
                pending.extend(alternatives.into_iter().rev());
                continue;
            }
            let mut result = self.find(cwd, &pattern)?;
            if result.is_empty() && self.no_check && !self.directories {
                result.push(pattern);
            }
            if self.sort {
                result.sort_unstable();
            }
            entries.extend(result);
        }
        Ok(entries)
    }

    fn find(&self, cwd: &Path, pattern: &[u8]) -> io::Result<Vec<Vec<u8>>> {
        if pattern.is_empty() {
            return Ok(Vec::new());
        }
        let mut prefixes = vec![Vec::new()];
        let mut components = pattern.split(|byte| *byte == b'/').peekable();
        while let Some(component) = components.next() {
            let mut next = Vec::new();
            for mut prefix in prefixes {
                if component.is_empty() {
                    next.push(prefix);
                    continue;
                }
                if let Some(literal) = self.pattern.literal(component) {
                    prefix.extend(literal);
                    next.push(prefix);
                    continue;
                }
                self.collect(cwd, &prefix, component, &mut next)?;
            }
            if components.peek().is_some() {
                for prefix in &mut next {
                    prefix.push(b'/');
                }
            }
            prefixes = next;
        }
        Ok(prefixes
            .into_iter()
            .filter_map(|mut name| {
                let file = cwd.join(path(&name));
                let is_directory = (self.directories || self.mark) && file.is_dir();
                if (self.directories && !is_directory) || file.symlink_metadata().is_err() {
                    return None;
                }
                if self.mark && is_directory && !name.ends_with(b"/") {
                    name.push(b'/');
                }
                Some(name)
            })
            .collect())
    }

    fn collect(
        &self,
        cwd: &Path,
        prefix: &[u8],
        component: &[u8],
        entries: &mut Vec<Vec<u8>>,
    ) -> io::Result<()> {
        let directory = cwd.join(path(prefix));
        let children = match directory.read_dir() {
            Ok(entries) => entries,
            Err(error)
                if self.errors
                    && !matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
            {
                return Err(error);
            }
            Err(_) => return Ok(()),
        };
        let mut push = |name: &[u8]| {
            if self.pattern.matches(component, name) {
                let mut result = prefix.to_vec();
                result.extend_from_slice(name);
                entries.push(result);
            }
        };
        if !cfg!(all(target_os = "linux", target_env = "gnu")) || component != b".*" {
            push(b".");
            push(b"..");
        }
        for child in children {
            match child {
                Ok(child) => push(child.file_name().as_bytes()),
                Err(error) if self.errors => return Err(error),
                Err(_) => (),
            }
        }
        Ok(())
    }
}

/// The patterns a pattern's first brace group stands for:
/// `src/{a,b}/x` gives `src/a/x` and `src/b/x`, and `{config}` gives
/// `config`. As in BSD glob, a pattern that is exactly `{}` stays literal.
fn split_brace(pattern: &[u8], escape: bool) -> Option<Vec<Vec<u8>>> {
    if pattern == b"{}" {
        return None;
    }
    let mut open = None;
    let mut depth = 0;
    let mut parts = Vec::new();
    let mut skipped = false;
    for (index, byte) in pattern.iter().copied().enumerate() {
        if skipped {
            skipped = false;
            continue;
        }
        match byte {
            b'\\' if escape => skipped = true,
            b'{' => {
                if depth == 0 {
                    open = Some(index);
                    parts.push(index);
                }
                depth += 1;
            }
            b',' if depth == 1 => parts.push(index),
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    parts.push(index);
                    let start = open?;
                    return Some(
                        parts
                            .windows(2)
                            .map(|bounds| {
                                [
                                    &pattern[..start],
                                    &pattern[bounds[0] + 1..bounds[1]],
                                    &pattern[index + 1..],
                                ]
                                .concat()
                            })
                            .collect(),
                    );
                }
            }
            _ => (),
        }
    }
    None
}
