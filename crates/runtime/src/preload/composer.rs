//! Composer's generated maps carry the data for `require 'vendor/autoload.php'`.
//! The preload reader recognizes their path expressions without running them.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use phera_ast::{
    ArrayItem, BinaryOperator, ClassMemberKind, Expression, ExpressionKind, Literal, MagicConstant,
    StatementKind, Variable,
};

pub(super) fn files(project: &Path) -> (Vec<PathBuf>, usize) {
    let Ok(directory) = project.join("vendor/composer").canonicalize() else {
        return (Vec::new(), 0);
    };
    let Some(vendor) = directory.parent() else {
        return (Vec::new(), 0);
    };
    let Some(base) = vendor.parent() else {
        return (Vec::new(), 0);
    };
    let roots = Roots {
        directory: &directory,
        vendor,
        base,
    };
    let mut paths = HashSet::new();
    for name in [
        "autoload_classmap.php",
        "autoload_files.php",
        "autoload_static.php",
    ] {
        let Ok(source) = std::fs::read(directory.join(name)) else {
            continue;
        };
        // The discovery job runs on the same large-stack pool as compilation.
        let Ok(program) = phera_parser::parse_bytes_on_current_stack(&source) else {
            continue;
        };
        let maps = if name == "autoload_static.php" {
            static_maps(&program)
        } else {
            program
                .statements
                .iter()
                .find_map(|statement| match &statement.kind {
                    StatementKind::Return(Some(expression)) => Some(expression),
                    _ => None,
                })
                .into_iter()
                .collect()
        };
        for map in maps {
            let ExpressionKind::Array(items) = &map.kind else {
                continue;
            };
            paths.extend(items.iter().filter_map(|item| roots.path(item)));
        }
    }
    let mut unavailable = 0;
    let mut canonical = HashSet::new();
    for path in paths {
        match path.canonicalize() {
            Ok(path) => {
                canonical.insert(path);
            }
            Err(_) => unavailable += 1,
        }
    }
    let mut files: Vec<_> = canonical.into_iter().collect();
    files.sort_unstable();
    (files, unavailable)
}

fn static_maps(program: &phera_ast::Program) -> Vec<&Expression> {
    let mut pending: Vec<_> = program.statements.iter().collect();
    let mut maps = Vec::new();
    while let Some(statement) = pending.pop() {
        match &statement.kind {
            StatementKind::Namespace { statements, .. } => pending.extend(statements),
            StatementKind::Class(class) => {
                for member in &class.members {
                    if let ClassMemberKind::Property {
                        modifiers, items, ..
                    } = &member.kind
                        && modifiers.is_static
                    {
                        maps.extend(items.iter().filter_map(|item| match item.name.as_str() {
                            "classMap" | "files" => item.default.as_ref(),
                            _ => None,
                        }));
                    }
                }
            }
            _ => {}
        }
    }
    maps
}

struct Roots<'a> {
    directory: &'a Path,
    vendor: &'a Path,
    base: &'a Path,
}

impl Roots<'_> {
    /// Composer concatenates literal path parts with its generated directory variables.
    fn path(&self, item: &ArrayItem) -> Option<PathBuf> {
        if item.by_reference || item.spread {
            return None;
        }
        let mut pending = vec![&item.value];
        let mut path = Vec::new();
        while let Some(expression) = pending.pop() {
            let bytes = match &expression.kind {
                ExpressionKind::Binary {
                    operator: BinaryOperator::Concat,
                    left,
                    right,
                } => {
                    pending.push(right);
                    pending.push(left);
                    continue;
                }
                ExpressionKind::Literal(Literal::String(bytes)) => bytes,
                ExpressionKind::MagicConstant(MagicConstant::Dir) => {
                    self.directory.as_os_str().as_encoded_bytes()
                }
                ExpressionKind::Variable(Variable::Named(name)) => match name.as_str() {
                    "vendorDir" => self.vendor.as_os_str().as_encoded_bytes(),
                    "baseDir" => self.base.as_os_str().as_encoded_bytes(),
                    _ => return None,
                },
                _ => return None,
            };
            path.extend_from_slice(bytes);
        }
        if path.is_empty() {
            return None;
        }
        os_string(path).map(|path| self.base.join(path))
    }
}

#[cfg(unix)]
fn os_string(bytes: Vec<u8>) -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt;
    Some(OsString::from_vec(bytes))
}

#[cfg(not(unix))]
fn os_string(bytes: Vec<u8>) -> Option<OsString> {
    String::from_utf8(bytes).ok().map(Into::into)
}
