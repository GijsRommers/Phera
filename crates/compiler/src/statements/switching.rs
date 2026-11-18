//! Lower PHP switch cases into ordered comparisons and fallthrough targets.

use phera_ast as ast;

use crate::policies::DeclarationTiming;
use crate::{
    CompileError, Compiler, Loop,
    ir::{Binary, Instruction, Target},
};

/// A comparison branch waiting for its case body's instruction position.
struct CaseJump {
    case_index: usize,
    instruction: usize,
}

impl Compiler<'_> {
    /// Comparisons run in source order, then bodies retain PHP fallthrough.
    pub(crate) fn compile_switch(
        &mut self,
        subject: &ast::Expression,
        cases: &[ast::SwitchCase],
        line: usize,
    ) -> Result<(), CompileError> {
        let (first, start) = (self.next, self.position());
        let value = self.compile_expression(subject)?;
        let mut jumps = vec![];
        let mut default = None;
        for (i, case) in cases.iter().enumerate() {
            if let Some(expression) = &case.condition {
                let rhs = self.compile_expression(expression)?;
                let dst = self.allocate_register()?;
                self.emit(
                    Instruction::Binary {
                        destination: dst,
                        operator: Binary::Equal,
                        left: value,
                        right: rhs,
                    },
                    line,
                );
                jumps.push(CaseJump {
                    case_index: i,
                    instruction: self.emit(
                        Instruction::Branch {
                            value: dst,
                            target: Target(0),
                            when: true,
                        },
                        line,
                    ),
                });
            } else {
                default = Some(i)
            }
        }
        let miss = self.emit(Instruction::Jump(Target(0)), line);
        self.loops.push(Loop {
            breaks: vec![],
            continues: vec![],
        });
        for (i, case) in cases.iter().enumerate() {
            for jump in jumps.iter().filter(|jump| jump.case_index == i) {
                self.patch(jump.instruction, self.position())
            }
            if default == Some(i) {
                self.patch(miss, self.position())
            }
            // The subject and the case values are dead in every body.
            self.release_temporaries(first, start, line);
            self.block(&case.body, DeclarationTiming::AtExecution)?
        }
        if default.is_none() {
            self.patch(miss, self.position());
            self.release_temporaries(first, start, line);
        }
        self.loop_end(self.position());
        Ok(())
    }
}
