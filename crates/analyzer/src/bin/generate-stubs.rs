//! Generate the committed PHP builtin table: `generate-stubs /path/to/php-src`.
//! Stub-only free-function semicolons become empty bodies before Phera parses them.

use phera_analyzer::{builtins, collect, index::Index};
use phera_lexer::{Lexer, TokenKind};
use std::path::{Path, PathBuf};

fn main() -> Result<(), String> {
    let root = std::env::args()
        .nth(1)
        .ok_or("Usage: generate-stubs /path/to/php-src")?;
    let root = Path::new(&root);
    let commit = std::process::Command::new("git")
        .args(["-C", &root.to_string_lossy(), "rev-parse", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    let commit = String::from_utf8_lossy(&commit.stdout).trim().to_string();
    let mut index = Index::default();
    let mut report = format!("php-src commit: {commit}\n");
    let mut count = 0;
    let mut skipped = 0;
    for file in stub_files(root)? {
        let source = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
        let source = normalize(&source)?;
        let relative = file
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy();
        match phera_parser::parse(&source) {
            Ok(program) => {
                collect::collect(&mut index, &relative, &source, &program);
                count += 1;
            }
            Err(error) => {
                let line = phera_source::LineTable::new(&source).line(error.span.start);
                report.push_str(&format!("{relative}:{line}: {}\n", error.message));
                skipped += 1;
            }
        }
    }
    for name in ["clone", "exit", "die"] {
        if let Some(signature) = index.functions.remove(&format!("phera_stub_{name}")) {
            index.functions.insert(name.into(), signature);
        }
    }
    if let Some(class) = index.classes.get_mut("stdclass") {
        class.open_properties = true;
    }
    let declarations = index.functions.len()
        + index.constants.len()
        + index.classes.len()
        + index
            .classes
            .values()
            .map(|c| c.methods.len() + c.properties.len() + c.constants.len())
            .sum::<usize>();
    report.push_str(&format!(
        "Parsed files: {count}; skipped files: {skipped}\n"
    ));
    report.push_str(&format!(
        "Declarations: {declarations}; functions: {}; classes: {}; globals: {}\n",
        index.functions.len(),
        index.classes.len(),
        index.constants.len()
    ));
    let data = format!("# php-src {commit}\n{}", builtins::encode(&index));
    let destination = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    std::fs::write(destination.join("php85.tsv"), data).map_err(|e| e.to_string())?;
    std::fs::write(destination.join("generation.txt"), &report).map_err(|e| e.to_string())?;
    print!("{report}");
    Ok(())
}

fn stub_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending: Vec<_> = ["Zend", "main", "ext"]
        .iter()
        .map(|p| root.join(p))
        .collect();
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
                pending.push(entry.map_err(|e| e.to_string())?.path());
            }
        } else if path.to_string_lossy().ends_with(".stub.php") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn normalize(source: &str) -> Result<String, String> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token().map_err(|e| format!("{e:?}"))?;
        if token.kind == TokenKind::Eof {
            break;
        }
        tokens.push(token);
    }
    let mut replacements = Vec::new();
    let mut braces = Vec::new();
    let mut awaiting_class = false;
    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Class | TokenKind::Interface | TokenKind::Trait | TokenKind::Enum => {
                awaiting_class = true;
            }
            TokenKind::LeftBrace => {
                braces.push(awaiting_class);
                awaiting_class = false;
            }
            TokenKind::RightBrace => {
                braces.pop();
            }
            _ => {}
        }
        if braces.contains(&true) {
            continue;
        }
        if token.kind != TokenKind::Function {
            continue;
        }
        let mut parens = 0;
        let mut opened = false;
        for next in &tokens[i + 1..] {
            match next.kind {
                TokenKind::LeftParen => {
                    parens += 1;
                    opened = true;
                }
                TokenKind::RightParen => parens -= 1,
                TokenKind::LeftBrace if opened && parens == 0 => break,
                TokenKind::Semicolon if opened && parens == 0 => {
                    // Methods already support semicolons; replacing them also preserves
                    // signatures, but abstractness is restored from modifiers below.
                    replacements.push(next.span);
                    break;
                }
                _ => {}
            }
        }
    }
    let mut result = source.to_string();
    for span in replacements.into_iter().rev() {
        result.replace_range(span.start..span.end, "{}");
    }
    for name in ["clone", "exit", "die"] {
        result = result.replace(
            &format!("function {name}("),
            &format!("function phera_stub_{name}("),
        );
    }
    Ok(result)
}
