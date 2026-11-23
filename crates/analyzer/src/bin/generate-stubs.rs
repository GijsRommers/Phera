//! `generate-stubs /path/to/php-src [git-ref]` reads PHP 8.5 without changing its checkout.
//! Use `generate-stubs --extensions /path/to/phpstorm-stubs REF` for optional extensions.
//! PHP signatures such as `strlen(string $string): int` become committed records.

fn main() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let root = arguments
        .next()
        .ok_or("Usage: generate-stubs /path/to/php-src [git-ref]")?;
    if root == "--extensions" {
        let root = arguments
            .next()
            .ok_or("--extensions needs a PhpStorm stubs directory")?;
        let reference = arguments
            .next()
            .ok_or("--extensions needs a stub version or commit")?;
        let report =
            phera_analyzer::stubs::generate_extensions(std::path::Path::new(&root), &reference)?;
        print!("{report}");
        return Ok(());
    }
    let reference = arguments.next().unwrap_or("php-8.5.12".into());
    let report = phera_analyzer::stubs::generate(std::path::Path::new(&root), &reference)?;
    print!("{report}");
    Ok(())
}
