//! Instruction translation and interning of access paths.

use phera_bytecode::{
    CastKind, Function, FunctionId, IncludeKind, Instruction, IterId, Reg, RuntimeError, Target,
    UnaryOp,
};

use crate::ir;
use crate::{
    passes::lower::Tables,
    passes::lowering_tables::{binary, path, static_id},
};

impl Tables {
    /// Translate one IR instruction, appending any required preparation instructions first.
    pub(crate) fn instruction(
        &mut self,
        function: &mut Function,
        source: ir::Instruction,
        constants: &[ir::Constant],
        scratch: Reg,
    ) {
        use ir::Instruction as Old;
        let instruction = match source {
            Old::GeneratorStart => Instruction::GeneratorStart,
            Old::Yield {
                destination: dst,
                key,
                value,
            } => Instruction::Yield { dst, key, value },
            Old::YieldFrom {
                destination: dst,
                source: src,
            } => Instruction::YieldFrom { dst, src },
            Old::CheckReturn { .. } if function.is_generator => Instruction::Nop,
            Old::Return(src) | Old::ReturnChecked { source: src } if function.is_generator => {
                Instruction::GeneratorReturn { src }
            }
            Old::ThisOrNull { destination: dst } => Instruction::ThisOrNull { dst },
            Old::Capture { source: src, kind } => Instruction::Capture { src, kind },
            Old::Closure {
                destination: dst,
                function,
                binding,
            } => Instruction::MakeClosure {
                dst,
                function: FunctionId(function as u32),
                binding,
            },
            Old::CheckReturn { source } => Instruction::CheckReturn { source },
            Old::ReturnChecked { source } => Instruction::ReturnChecked { source },
            Old::FinallyCall {
                state,
                target,
                reason,
            } => Instruction::FinallyCall {
                state,
                target: Target(target.0 as u32),
                reason,
            },
            Old::FinallyReturn { state } => Instruction::FinallyReturn { state },
            Old::FinallyDiscard { state } => Instruction::FinallyDiscard { state },
            Old::Throw { source } => Instruction::Throw { source },
            Old::KnownStaticCall {
                class,
                method,
                binding,
            } => Instruction::InitStaticCall {
                class: self.symbol(&class, None, crate::policies::NameKind::Class),
                method: self.symbol(&method, None, crate::policies::NameKind::Function),
                binding,
            },
            Old::DynamicClassConstant {
                destination: dst,
                class,
                name,
            } => Instruction::FetchDynamicClassConstant { dst, class, name },
            Old::ClassConstant {
                destination,
                class,
                name,
            } => Instruction::FetchClassConstant {
                dst: destination,
                class,
                name: self.name(&name),
            },
            Old::NewDynamic { class } => Instruction::NewDynamic { class },
            Old::DynamicMethodCall { receiver, method } => {
                Instruction::DynamicMethodCall { receiver, method }
            }
            Old::StaticCall {
                class,
                method,
                binding,
            } => Instruction::StaticCall {
                class,
                method,
                binding,
            },
            Old::DynamicInstanceOf {
                destination,
                source,
                class,
            } => Instruction::DynamicInstanceOf {
                dst: destination,
                source,
                class,
            },
            Old::DeclareClass(index) => Instruction::DeclareClass {
                class: phera_bytecode::ClassId(index as u32),
            },
            Old::New { class } => Instruction::New {
                class: self.symbol(&class, None, crate::policies::NameKind::Class),
            },
            Old::MethodCall { receiver, method } => Instruction::InitMethodCall {
                receiver,
                method: self.name(&method),
            },
            Old::ClassName { destination, class } => Instruction::FetchClassName {
                dst: destination,
                class: self.symbol(&class, None, crate::policies::NameKind::Class),
            },
            Old::This { destination } => Instruction::FetchThis { dst: destination },
            Old::Clone {
                destination,
                source,
            } => Instruction::Clone {
                dst: destination,
                src: source,
            },
            Old::InstanceOf {
                destination,
                source,
                class,
            } => Instruction::InstanceOf {
                dst: destination,
                src: source,
                class: self.symbol(&class, None, crate::policies::NameKind::Class),
            },
            Old::DynamicGlobal { name } => Instruction::BindDynamicGlobal { name },
            Old::Check {
                local,
                declaration_type: ty,
            } => Instruction::CheckType {
                local,
                ty: self.ty(&ty),
            },
            Old::Clear { register } => Instruction::Clear { reg: register },
            Old::DefaultCheck {
                parameter: param,
                passed,
            } => Instruction::ReceiveDefault {
                param,
                passed: Target(passed.0 as u32),
            },
            Old::StaticCheck {
                local,
                name,
                initialized,
            } => {
                let variable = static_id(self, function, &name);
                Instruction::BindStatic {
                    local,
                    variable,
                    initialized: Target(initialized.0 as u32),
                }
            }
            Old::StaticInit {
                local,
                name,
                source: src,
            } => {
                let variable = static_id(self, function, &name);
                Instruction::InitStatic {
                    local,
                    variable,
                    src,
                }
            }
            Old::StartCall { name, fallback } => Instruction::InitCall {
                function: self.symbol(&name, fallback, crate::policies::NameKind::Function),
            },
            Old::StartDynamicCall { callee } => Instruction::InitDynamicCall { callee },
            Old::FinishCall { destination: dst } => Instruction::Call { dst },
            Old::FinishCallable { destination: dst } => Instruction::MakeCallable { dst },
            Old::Send { argument } => self.send_argument(function, argument),
            Old::IncPlace {
                destination: dst,
                place,
                mode,
            } => self.increment_place(function, dst, place, mode),
            Old::Compound {
                destination: dst,
                place,
                operator,
                source,
            } => self.compound_assignment(function, dst, place, operator, source),
            Old::Load {
                destination: dst,
                constant,
            } => match constants.get(constant) {
                Some(ir::Constant::Int(value)) if i32::try_from(*value).is_ok() => {
                    Instruction::LoadInt {
                        dst,
                        value: *value as i32,
                    }
                }
                Some(value) => Instruction::LoadConst {
                    dst,
                    constant: self.constant(value),
                },
                None => Instruction::LoadNull { dst },
            },
            Old::Move {
                destination: dst,
                source: src,
            } => Instruction::Move { dst, src },
            Old::Read {
                destination: dst,
                place,
                mode,
            } => self.read_place(function, dst, place, mode),
            Old::Write { place, source: src } => self.write_place(function, place, src),
            Old::Reference {
                destination: dst,
                place,
            } => Instruction::MakeReference {
                dst,
                path: path(self, function, place),
            },
            Old::Rebind { place, source: src } => Instruction::BindReference {
                path: path(self, function, place),
                src,
            },
            Old::Bind { place, source } => self.bind_reference(function, place, source, scratch),
            Old::Unset(place) => Instruction::UnsetPath {
                path: path(self, function, place),
            },
            Old::Binary {
                destination: dst,
                operator: op,
                left: lhs,
                right: rhs,
            } => Instruction::Binary {
                dst,
                op: binary(op),
                lhs,
                rhs,
            },
            Old::Unary {
                destination: dst,
                operator: op,
                source: src,
            } => match op {
                ir::Unary::Plus => Instruction::Unary {
                    dst,
                    src,
                    op: UnaryOp::Plus,
                },
                ir::Unary::Minus => Instruction::Unary {
                    dst,
                    src,
                    op: UnaryOp::Negate,
                },
                ir::Unary::Not => Instruction::Unary {
                    dst,
                    src,
                    op: UnaryOp::Not,
                },
                ir::Unary::BitwiseNot => Instruction::Unary {
                    dst,
                    src,
                    op: UnaryOp::BitwiseNot,
                },
                other => Instruction::Cast {
                    dst,
                    src,
                    to: match other {
                        ir::Unary::Bool => CastKind::Bool,
                        ir::Unary::Int => CastKind::Int,
                        ir::Unary::Float => CastKind::Float,
                        ir::Unary::String => CastKind::String,
                        ir::Unary::Object => CastKind::Object,
                        _ => CastKind::Array,
                    },
                },
            },
            Old::Echo(src) => Instruction::Echo { src },
            Old::Jump(target) => Instruction::Jump {
                target: Target(target.0 as u32),
            },
            Old::Branch {
                value,
                target,
                when,
            } => {
                if when {
                    Instruction::JumpIf {
                        condition: value,
                        target: Target(target.0 as u32),
                    }
                } else {
                    Instruction::JumpIfNot {
                        condition: value,
                        target: Target(target.0 as u32),
                    }
                }
            }
            Old::BranchNull { value, target } => Instruction::JumpIfNull {
                value,
                target: Target(target.0 as u32),
            },
            Old::Suppress(true) => Instruction::BeginSilence,
            Old::Suppress(false) => Instruction::EndSilence,
            Old::ReturnReference(src) => Instruction::ReturnReference { src },
            Old::Return(src) => Instruction::Return { src },
            Old::Exit(src) => Instruction::Exit { src },
            Old::Array(dst) => Instruction::NewArray { dst, capacity: 0 },
            Old::ArrayPush {
                array,
                key,
                value: src,
                spread,
            } => {
                if spread {
                    Instruction::ArraySpread { array, src }
                } else if let Some(key) = key {
                    Instruction::ArraySetReference { array, key, src }
                } else {
                    Instruction::ArrayPushReference { array, src }
                }
            }
            Old::Constant {
                destination: dst,
                name,
                fallback,
            } => Instruction::FetchConstant {
                dst,
                constant: self.symbol(&name, fallback, crate::policies::NameKind::Constant),
            },
            Old::Define { name, source: src } => Instruction::DeclareConstant {
                name: self.name(&name),
                src,
            },
            Old::Declare(index) => Instruction::DeclareFunction {
                function: FunctionId(index as u32),
            },
            Old::Global { local, name } => Instruction::BindGlobal {
                local,
                name: self.name(&name),
            },
            Old::Eval {
                destination: dst,
                source,
            } => Instruction::Eval { dst, source },
            Old::Include {
                destination: dst,
                path,
                once,
                required,
            } => Instruction::Include {
                dst,
                path,
                kind: match (once, required) {
                    (false, false) => IncludeKind::Include,
                    (true, false) => IncludeKind::IncludeOnce,
                    (false, true) => IncludeKind::Require,
                    (true, true) => IncludeKind::RequireOnce,
                },
            },
            Old::IterInit {
                iterator: id,
                source: src,
                place,
            } => {
                function.iterators = function.iterators.max(id.0 + 1);
                if let Some(place) = place {
                    Instruction::IterStartReference {
                        subject: src,
                        iter: IterId(id.0),
                        path: path(self, function, place),
                        end: Target(0),
                    }
                } else {
                    Instruction::IterStart {
                        iter: IterId(id.0),
                        src,
                        end: Target(0),
                    }
                }
            }
            Old::IterNext {
                iterator: id,
                key,
                value,
                end,
            } => match key {
                Some(key) => Instruction::IterNextKeyed {
                    iter: IterId(id.0),
                    key,
                    value,
                    end: Target(end.0 as u32),
                },
                None => Instruction::IterNext {
                    iter: IterId(id.0),
                    value,
                    end: Target(end.0 as u32),
                },
            },
            Old::MatchError { subject } => Instruction::Raise {
                error: RuntimeError::UnhandledMatch,
                detail: subject,
            },
            Old::Call {
                destination: dst,
                name,
                fallback,
                arguments,
            } => self.dynamic_call(function, dst, name, fallback, arguments),
        };
        function.instructions.push(instruction);
    }
}
