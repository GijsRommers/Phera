//! Phase isolation probe: parse and compile a file step by step, printing
//! which phase dies. usage: probe <file.php>
fn main() {
    let path = std::env::args().nth(1).expect("usage: probe <file>");
    let source = std::fs::read_to_string(&path).expect("read");
    eprintln!("phase: lex ({} bytes)", source.len());
    let mut lex = phera_lexer::Lexer::new(&source);
    let mut n = 0usize;
    loop {
        match lex.next_token() {
            Ok(t) => {
                n += 1;
                if t.kind == phera_lexer::TokenKind::Eof { break; }
            }
            Err(e) => { eprintln!("lex error: {:?}", e); break; }
        }
    }
    eprintln!("phase: lex ok ({n} tokens)");
    eprintln!("phase: parse ({} bytes)", source.len());
    let program = match phera_parser::parse(&source) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("parse error: {e}");
            return;
        }
    };
    eprintln!("phase: parse ok; compile");
    match phera_compiler::compile(&source, &path) {
        Ok(_) => eprintln!("phase: compile ok"),
        Err(e) => eprintln!("compile error: {e}"),
    }
    drop(program);
    eprintln!("phase: drop ast");
}
