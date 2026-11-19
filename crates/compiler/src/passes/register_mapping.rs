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
            if let crate::ir::PlaceScope::StaticProperty { name, scratch } = &mut target.scope {
                *name = mapping(*name);
                *scratch = mapping(*scratch);
            }
            for step in &mut target.path {
                if let crate::ir::PathStep::Index(register)
                | crate::ir::PathStep::DynamicProperty(register) = step
                {
                    *register = mapping(*register);
                }
            }
        }
        match self {
            Instruction::GeneratorStart => {}
            Instruction::Yield {
                destination,
                key,
                value,
            } => {
                *destination = map(*destination);
                *key = key.map(&mut map);
                *value = value.map(&mut map);
            }
            Instruction::YieldFrom {
                destination,
                source,
            } => {
                *destination = map(*destination);
                *source = map(*source);
            }
            Instruction::AttributeDiagnostics => {}
            Instruction::ThisOrNull { destination } => *destination = map(*destination),
            Instruction::Capture { source, .. } => *source = map(*source),
            Instruction::Closure { destination, .. } => *destination = map(*destination),
            Instruction::CheckReturn { source } | Instruction::ReturnChecked { source } => {
                *source = map(*source)
            }
            Instruction::FinallyCall { state, .. }
            | Instruction::FinallyReturn { state }
            | Instruction::FinallyDiscard { state } => *state = map(*state),
            Instruction::Throw { source } => *source = map(*source),
            Instruction::DynamicClassConstant {
                destination,
                class,
                name,
            } => {
                *destination = map(*destination);
                *class = map(*class);
                *name = map(*name);
            }
            Instruction::ClassConstant {
                destination, class, ..
            } => {
                *destination = map(*destination);
                *class = map(*class);
            }

            Instruction::NewDynamic { class } => *class = map(*class),
            Instruction::DynamicMethodCall { receiver, method } => {
                *receiver = map(*receiver);
                *method = map(*method);
            }
            Instruction::StaticCall { class, method, .. } => {
                *class = map(*class);
                *method = map(*method);
            }
            Instruction::DynamicInstanceOf {
                destination,
                source,
                class,
            } => {
                *destination = map(*destination);
                *source = map(*source);
                *class = map(*class);
            }
            Instruction::DeclareClass(_)
            | Instruction::New { .. }
            | Instruction::KnownStaticCall { .. } => {}
            Instruction::ClassName { destination, .. } | Instruction::This { destination } => {
                *destination = map(*destination)
            }
            Instruction::MethodCall { receiver, .. } => *receiver = map(*receiver),
            Instruction::Clone {
                destination,
                source,
                with,
            } => {
                *destination = map(*destination);
                *source = map(*source);
                *with = with.map(map);
            }
            Instruction::InstanceOf {
                destination,
                source,
                ..
            } => {
                *destination = map(*destination);
                *source = map(*source);
            }
            Instruction::DefaultCheck {
                parameter: param, ..
            } => *param = map(*param),
            Instruction::StaticCheck { local, .. } => *local = map(*local),
            Instruction::StaticInit {
                local, source: src, ..
            } => {
                *local = map(*local);
                *src = map(*src);
            }
            Instruction::Check { local, .. } => *local = map(*local),
            Instruction::DynamicGlobal { name } => *name = map(*name),
            Instruction::Clear { register } => *register = map(*register),
            Instruction::StartCall { .. } => {}
            Instruction::StartDynamicCall { callee } => *callee = map(*callee),
            Instruction::FinishCall {
                destination: dst, ..
            }
            | Instruction::FinishCallable { destination: dst } => *dst = map(*dst),
            Instruction::Send { argument } => match &mut argument.source {
                crate::ir::ArgumentSource::Value(register) => *register = map(*register),
                crate::ir::ArgumentSource::Place(target) => place(target, &mut map),
            },
            Instruction::IncPlace {
                destination: dst,
                place: target,
                ..
            } => {
                *dst = map(*dst);
                place(target, &mut map);
            }
            Instruction::Compound {
                destination: dst,
                place: target,
                source,
                ..
            } => {
                *dst = map(*dst);
                *source = map(*source);
                place(target, &mut map);
            }

            Instruction::Load {
                destination: dst, ..
            }
            | Instruction::Constant {
                destination: dst, ..
            }
            | Instruction::Array(dst)
            | Instruction::Echo(dst)
            | Instruction::ReturnReference(dst)
            | Instruction::Return(dst)
            | Instruction::Exit(dst) => *dst = map(*dst),
            Instruction::Eval {
                destination: dst,
                source: src,
            }
            | Instruction::Include {
                destination: dst,
                path: src,
                ..
            }
            | Instruction::Move {
                destination: dst,
                source: src,
            }
            | Instruction::Unary {
                destination: dst,
                source: src,
                ..
            } => {
                *dst = map(*dst);
                *src = map(*src)
            }
            Instruction::Reference {
                destination: dst,
                place: target,
            }
            | Instruction::Read {
                destination: dst,
                place: target,
                ..
            } => {
                *dst = map(*dst);
                place(target, &mut map)
            }
            Instruction::Rebind {
                place: target,
                source: src,
            }
            | Instruction::Write {
                place: target,
                source: src,
            } => {
                place(target, &mut map);
                *src = map(*src)
            }
            Instruction::WriteFromLocal {
                place: target,
                source: src,
                destination: dst,
            } => {
                place(target, &mut map);
                *src = map(*src);
                *dst = map(*dst);
            }
            Instruction::Bind {
                place: target,
                source,
            } => {
                place(target, &mut map);
                place(source, &mut map)
            }
            Instruction::Unset(target) => place(target, &mut map),
            Instruction::Binary {
                destination: dst,
                left: lhs,
                right: rhs,
                ..
            } => {
                *dst = map(*dst);
                *lhs = map(*lhs);
                *rhs = map(*rhs)
            }
            Instruction::Branch { value, .. }
            | Instruction::BranchNull { value, .. }
            | Instruction::Define { source: value, .. }
            | Instruction::Global { local: value, .. } => *value = map(*value),
            Instruction::Call {
                destination: dst,
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
                iterator: id,
                source: src,
                place: target,
            } => {
                *id = map(*id);
                *src = map(*src);
                if let Some(target) = target {
                    place(target, &mut map)
                }
            }
            Instruction::IterEnd(iterator) => *iterator = map(*iterator),
            Instruction::IterNext {
                iterator: id,
                key,
                value,
                ..
            } => {
                *id = map(*id);
                if let Some(key) = key {
                    *key = map(*key);
                }
                *value = map(*value)
            }
            Instruction::MatchError { subject } => *subject = map(*subject),
            Instruction::Nop
            | Instruction::Jump(_)
            | Instruction::Suppress(_)
            | Instruction::Declare(_)
            | Instruction::ReturnMissing => {}
        }
    }
}
