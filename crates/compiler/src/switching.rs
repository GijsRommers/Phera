//! Lower PHP switch cases into ordered comparisons and fallthrough targets.

use phera_ast as ast;

use crate::{
    CompileError, Compiler, Loop,
    ir::{Binary, Instruction, Target},
};

impl Compiler<'_> {
    /// Comparisons run in source order, then bodies retain PHP fallthrough.
    pub(super) fn compile_switch(
        &mut self,
        subject: &ast::Expression,
        cases: &[ast::SwitchCase],
        line: usize,
    ) -> Result<(), CompileError> {
        let value = self.expr(subject)?;
        let mut jumps = vec![];
        let mut default = None;
        for (i, c) in cases.iter().enumerate() {
            if let Some(expression) = &c.condition {
                let rhs = self.expr(expression)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Binary {
                        dst,
                        operator: Binary::Equal,
                        lhs: value,
                        rhs,
                    },
                    line,
                );
                jumps.push((
                    i,
                    self.emit(
                        Instruction::Branch {
                            value: dst,
                            target: Target(0),
                            when: true,
                        },
                        line,
                    ),
                ));
            } else {
                default = Some(i)
            }
        }
        let miss = self.emit(Instruction::Jump(Target(0)), line);
        self.loops.push(Loop {
            breaks: vec![],
            continues: vec![],
        });
        for (i, c) in cases.iter().enumerate() {
            for (_, jump) in jumps.iter().filter(|(name, _)| *name == i) {
                self.patch(*jump, self.pos())
            }
            if default == Some(i) {
                self.patch(miss, self.pos())
            }
            self.block(&c.body, false)?
        }
        if default.is_none() {
            self.patch(miss, self.pos())
        }
        self.loop_end(self.pos());
        Ok(())
    }
}
