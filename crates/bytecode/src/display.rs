//! A readable listing of a compiled unit, for `phera --dump` and for tests.

use std::fmt;

use crate::{CompiledUnit, Function};

impl fmt::Display for CompiledUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "file {}", self.file)?;
        for (index, constant) in self.constants.iter().enumerate() {
            writeln!(f, "  const#{index} = {constant:?}")?;
        }
        for (index, symbol) in self.symbols.iter().enumerate() {
            match &symbol.fallback {
                Some(fallback) => {
                    writeln!(f, "  symbol#{index} = {} or {fallback}", symbol.name)?;
                }
                None => writeln!(f, "  symbol#{index} = {}", symbol.name)?,
            }
        }
        for (index, ty) in self.types.iter().enumerate() {
            writeln!(f, "  type#{index} = {}", ty.display)?;
        }
        self.function(f, &self.main)?;
        for function in &self.functions {
            self.function(f, function)?;
        }
        Ok(())
    }
}

impl CompiledUnit {
    fn name(&self, id: crate::NameId) -> &str {
        self.names.get(id.index()).map_or("?", String::as_str)
    }

    fn function(&self, f: &mut fmt::Formatter<'_>, function: &Function) -> fmt::Result {
        writeln!(f)?;
        writeln!(
            f,
            "function {} ({} registers{})",
            function.name,
            function.registers,
            if function.dynamic_scope {
                ", dynamic scope"
            } else {
                ""
            }
        )?;
        for (register, name) in function.locals.iter().enumerate() {
            writeln!(f, "  r{register} = ${}", self.name(*name))?;
        }
        for (index, path) in function.paths.iter().enumerate() {
            writeln!(f, "  path#{index} = {:?} {:?}", path.base, path.steps)?;
        }
        for (index, site) in function.method_sites.iter().enumerate() {
            let arguments = function
                .site_arguments
                .get(site.argument_range())
                .unwrap_or_default();
            writeln!(
                f,
                "  site#{index} = {:?}->{:?}({arguments:?}), after {:?}{}",
                site.receiver,
                site.method,
                site.after,
                if site.receiver_taken {
                    ", takes the receiver"
                } else {
                    ""
                }
            )?;
        }
        for (index, handler) in function.handlers.iter().enumerate() {
            writeln!(f, "  handler#{index} = {handler:?}")?;
        }
        for (index, instruction) in function.instructions.iter().enumerate() {
            let line = function.lines.get(index).copied().unwrap_or(0);
            writeln!(f, "  {index:04}  line {line:<4} {instruction:?}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{BinaryOp, CompiledUnit, Function, Instruction, NameId, Reg, Target};

    #[test]
    fn a_listing_names_locals_and_shows_operands_compactly() {
        let unit = CompiledUnit {
            file: "loop.php".to_string(),
            names: vec!["i".to_string()],
            main: Function {
                name: "{main}".to_string(),
                registers: 2,
                locals: vec![NameId(0)],
                instructions: vec![
                    Instruction::LoadInt {
                        dst: Reg(1),
                        value: 1,
                    },
                    Instruction::AssignOp {
                        op: BinaryOp::Add,
                        local: Reg(0),
                        src: Reg(1),
                    },
                    Instruction::Jump { target: Target(0) },
                ]
                .into(),
                lines: vec![3, 3, 4],
                ..Function::default()
            },
            ..CompiledUnit::default()
        };
        let listing = unit.to_string();
        assert!(
            listing.contains("function {main} (2 registers)"),
            "{listing}"
        );
        assert!(listing.contains("r0 = $i"), "{listing}");
        assert!(
            listing.contains("0001  line 3    AssignOp { op: Add, local: r0, src: r1 }"),
            "{listing}"
        );
        assert!(listing.contains("Jump { target: @0 }"), "{listing}");
    }
}
