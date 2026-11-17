//! Literal static calls carry resolved class and normalized method symbols.

use phera_bytecode::Instruction;

/// Static syntax must preserve diagnostic spelling while resolving aliases once.
#[test]
fn literal_static_operands_keep_lookup_and_written_names() {
    let unit = phera_compiler::compile(
        "<?php namespace App; use Vendor\\Example as Alias; Alias::RuN();",
        "known-static.php",
    )
    .unwrap();
    let (class, method) = unit
        .main
        .instructions
        .iter()
        .find_map(|instruction| {
            if let Instruction::InitStaticCall { class, method, .. } = instruction {
                Some((*class, *method))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(unit.symbols[class.index()].written, "Vendor\\Example");
    assert_eq!(unit.symbols[method.index()].name, "run");
    assert_eq!(unit.symbols[method.index()].written, "RuN");
}
