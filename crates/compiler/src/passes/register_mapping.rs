//! Reorder IR register operands so named locals precede temporary slots.
//!
//! Input registers reflect allocation order. The caller supplies a complete
//! mapping; output rewrites every operand without changing instructions.
//! For example, a local allocated as r3 can become r0, including in paths.

use phera_bytecode::Reg;

use crate::ir::{Instruction, Place};

impl Instruction {
    /// Visits all register operands, including writable argument paths.
    pub fn map_registers(&mut self, mut map: impl FnMut(Reg) -> Reg) {
        /// Walk indexed targets iteratively, then evaluate index expressions from root to leaf.
        fn place(target: &mut Place, mapping: &mut impl FnMut(Reg) -> Reg) {
            target.base = mapping(target.base);
            if let crate::ir::PlaceScope::Global(scratch) = &mut target.scope {
                *scratch = mapping(*scratch);
            }
            for register in target.path.iter_mut().flatten() {
                *register = mapping(*register)
            }
        }
        match self {
            Instruction::DefaultCheck { param, .. } => *param = map(*param),
            Instruction::StaticCheck { local, .. } => *local = map(*local),
            Instruction::StaticInit { local, src, .. } => {
                *local = map(*local);
                *src = map(*src);
            }
            Instruction::Check { local, .. } => *local = map(*local),
            Instruction::DynamicGlobal { name } => *name = map(*name),
            Instruction::Clear { register } => *register = map(*register),
            Instruction::StartCall { .. } => {}
            Instruction::StartDynamicCall { callee } => *callee = map(*callee),
            Instruction::FinishCall { dst } | Instruction::FinishCallable { dst } => {
                *dst = map(*dst)
            }
            Instruction::Send { argument } => match &mut argument.source {
                crate::ir::ArgumentSource::Value(register) => *register = map(*register),
                crate::ir::ArgumentSource::Place(target) => place(target, &mut map),
            },
            Instruction::IncPlace {
                dst, place: target, ..
            } => {
                *dst = map(*dst);
                place(target, &mut map);
            }
            Instruction::Compound {
                dst,
                place: target,
                source,
                ..
            } => {
                *dst = map(*dst);
                *source = map(*source);
                place(target, &mut map);
            }

            Instruction::Load { dst, .. }
            | Instruction::Constant { dst, .. }
            | Instruction::Array(dst)
            | Instruction::Echo(dst)
            | Instruction::ReturnReference(dst)
            | Instruction::Return(dst)
            | Instruction::Exit(dst) => *dst = map(*dst),
            Instruction::Include { dst, path: src, .. }
            | Instruction::Move { dst, src }
            | Instruction::Unary { dst, src, .. } => {
                *dst = map(*dst);
                *src = map(*src)
            }
            Instruction::Reference { dst, place: target }
            | Instruction::Read {
                dst, place: target, ..
            } => {
                *dst = map(*dst);
                place(target, &mut map)
            }
            Instruction::Rebind { place: target, src }
            | Instruction::Write { place: target, src } => {
                place(target, &mut map);
                *src = map(*src)
            }
            Instruction::Bind {
                place: target,
                source,
            } => {
                place(target, &mut map);
                place(source, &mut map)
            }
            Instruction::Unset(target) => place(target, &mut map),
            Instruction::Binary { dst, lhs, rhs, .. } => {
                *dst = map(*dst);
                *lhs = map(*lhs);
                *rhs = map(*rhs)
            }
            Instruction::Branch { value, .. }
            | Instruction::BranchNull { value, .. }
            | Instruction::Define { src: value, .. }
            | Instruction::Global { local: value, .. } => *value = map(*value),
            Instruction::Call {
                dst,
                name,
                arguments,
                ..
            } => {
                *dst = map(*dst);
                *name = map(*name);
                for argument in arguments {
                    match &mut argument.source {
                        crate::ir::ArgumentSource::Value(register) => *register = map(*register),
                        crate::ir::ArgumentSource::Place(target) => place(target, &mut map),
                    }
                }
            }
            Instruction::ArrayPush {
                array, key, value, ..
            } => {
                *array = map(*array);
                *value = map(*value);
                if let Some(key_register) = key {
                    *key_register = map(*key_register)
                }
            }
            Instruction::IterInit {
                id,
                src,
                place: target,
            } => {
                *id = map(*id);
                *src = map(*src);
                if let Some(target) = target {
                    place(target, &mut map)
                }
            }
            Instruction::IterNext { id, key, value, .. } => {
                *id = map(*id);
                *key = map(*key);
                *value = map(*value)
            }
            Instruction::MatchError { subject } => *subject = map(*subject),
            Instruction::Jump(_) | Instruction::Suppress(_) | Instruction::Declare(_) => {}
        }
    }
}
