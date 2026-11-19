//! Read Composer's `return ['Name' => $vendorDir . '/pkg/Name.php'];`.
//! The two directory assignments are recognized as syntax and never executed.

use super::Autoload;
use phera_ast::{
    Arguments, ArrayItem, BinaryOperator, Expression, ExpressionKind, Literal, MagicConstant,
    NameKind, StatementKind, Variable,
};
use std::path::{Path, PathBuf};

pub fn load(root: &Path) -> Result<Autoload, String> {
    let vendor = root.join("vendor");
    let composer = vendor.join("composer");
    let mut autoload = Autoload {
        vendor: vendor.clone(),
        files: Vec::new(),
        classmap: Default::default(),
        prefixes: Default::default(),
    };
    read_map(&composer.join("autoload_namespaces.php"), true, |items| {
        if items.is_empty() {
            Ok(())
        } else {
            Err("PSR-0 autoloading is not supported")
        }
    })?;
    read_map(&composer.join("autoload_classmap.php"), false, |items| {
        for item in items {
            let (name, value) = entry(item)?;
            autoload
                .classmap
                .insert(name.to_ascii_lowercase(), path(value, root, &vendor)?);
        }
        Ok(())
    })?;
    read_map(&composer.join("autoload_psr4.php"), false, |items| {
        for item in items {
            let (prefix, value) = entry(item)?;
            if !prefix.is_empty() && !prefix.ends_with('\\') {
                return Err("expected a PSR-4 namespace prefix ending in a backslash");
            }
            let ExpressionKind::Array(directories) = &value.kind else {
                return Err("expected an array of PSR-4 directories");
            };
            let mut paths = Vec::new();
            for directory in directories {
                if directory.key.is_some() || directory.spread || directory.by_reference {
                    return Err("expected a list of PSR-4 directories");
                }
                paths.push(path(&directory.value, root, &vendor)?);
            }
            autoload.prefixes.insert(prefix.into(), paths);
        }
        Ok(())
    })?;
    read_map(&composer.join("autoload_files.php"), true, |items| {
        for item in items {
            let (_, value) = entry(item)?;
            autoload.files.push(path(value, root, &vendor)?);
        }
        Ok(())
    })?;
    Ok(autoload)
}

fn read_map(
    file: &Path,
    optional: bool,
    read_entries: impl FnOnce(&[ArrayItem]) -> Result<(), &'static str>,
) -> Result<(), String> {
    let label = file.display();
    let source = match std::fs::read(file) {
        Ok(source) => source,
        Err(error) if optional && error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{label}: {error}")),
    };
    let program = phera_parser::parse_on_current_stack(&String::from_utf8_lossy(&source))
        .map_err(|error| format!("{label}: {}", error.message))?;
    let [vendor, base, returned] = program.statements.as_slice() else {
        return Err(format!(
            "{label}: expected Composer's directory assignments and return array"
        ));
    };
    let valid_vendor = assignment(&vendor.kind, "vendorDir")
        .and_then(dirname_argument)
        .and_then(dirname_argument)
        .is_some_and(|file| {
            matches!(
                file.kind,
                ExpressionKind::MagicConstant(MagicConstant::File)
            )
        });
    let valid_base = assignment(&base.kind, "baseDir")
        .and_then(dirname_argument)
        .is_some_and(|value| named_variable(value) == Some("vendorDir"));
    if !valid_vendor || !valid_base {
        return Err(format!(
            "{label}: expected Composer's directory assignments"
        ));
    }
    if let StatementKind::Return(Some(value)) = &returned.kind
        && let ExpressionKind::Array(items) = &value.kind
    {
        return read_entries(items).map_err(|reason| format!("{label}: {reason}"));
    }
    Err(format!("{label}: expected a return array"))
}

fn assignment<'a>(statement: &'a StatementKind, variable: &str) -> Option<&'a Expression> {
    let StatementKind::Expression(expression) = statement else {
        return None;
    };
    if let ExpressionKind::Assign { target, value } = &expression.kind
        && named_variable(target) == Some(variable)
    {
        return Some(value);
    }
    None
}

fn dirname_argument(expression: &Expression) -> Option<&Expression> {
    let ExpressionKind::Call {
        callee,
        arguments: Arguments::List(arguments),
    } = &expression.kind
    else {
        return None;
    };
    if let ExpressionKind::Name(name) = &callee.kind
        && name.kind == NameKind::Unqualified
        && name.text == "dirname"
        && let [argument] = arguments.as_slice()
        && argument.name.is_none()
        && !argument.spread
    {
        return Some(&argument.value);
    }
    None
}

fn named_variable(expression: &Expression) -> Option<&str> {
    match &expression.kind {
        ExpressionKind::Variable(Variable::Named(name)) => Some(name),
        _ => None,
    }
}

fn entry(item: &ArrayItem) -> Result<(&str, &Expression), &'static str> {
    if item.spread || item.by_reference {
        return Err("expected a literal-keyed entry");
    }
    let key = item.key.as_ref().and_then(literal_string);
    key.map(|key| (key, &item.value))
        .ok_or("expected a literal string key")
}

fn literal_string(expression: &Expression) -> Option<&str> {
    if let ExpressionKind::Literal(Literal::String(bytes)) = &expression.kind {
        return std::str::from_utf8(bytes).ok();
    }
    None
}

fn path(value: &Expression, root: &Path, vendor: &Path) -> Result<PathBuf, &'static str> {
    let (directory, suffix) = match &value.kind {
        ExpressionKind::Binary {
            operator: BinaryOperator::Concat,
            left,
            right,
        } => {
            let suffix = literal_string(right)
                .and_then(|s| s.strip_prefix('/'))
                .ok_or("expected a literal path suffix starting with /")?;
            (left.as_ref(), suffix)
        }
        _ => (value, ""),
    };
    let base = match named_variable(directory) {
        Some("baseDir") => root,
        Some("vendorDir") => vendor,
        _ => return Err("expected a path based on $vendorDir or $baseDir"),
    };
    // Composer concatenates the slash text even when the suffix starts with
    // another slash. Path::join would interpret that as an absolute path.
    Ok(base.join(suffix.trim_start_matches('/')))
}
