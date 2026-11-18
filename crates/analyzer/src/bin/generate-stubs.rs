//! `generate-stubs /path/to/php-src [git-ref]` reads PHP 8.5 without changing its checkout.
//! By default the data matches the PHP 8.5.12 release used by the oracle.
//! PHP signatures such as `strlen(string $string): int` become committed records.

fn main() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let root = arguments
        .next()
        .ok_or("Usage: generate-stubs /path/to/php-src [git-ref]")?;
    let reference = arguments.next().unwrap_or("php-8.5.12".into());
    let report = phera_analyzer::stubs::generate(std::path::Path::new(&root), &reference)?;
    print!("{report}");
    Ok(())
}
