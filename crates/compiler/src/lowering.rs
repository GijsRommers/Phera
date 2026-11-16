//! Instruction translation and interning of access paths.
use phera_bytecode::{
    BinaryOp, CastKind, FetchMode, Function, FunctionId, IncDec, IncludeKind, Instruction, IterId,
    Path, PathBase, PathId, Reg, RuntimeError, StaticId, Step, Target, UnaryOp,
};

use crate::ir;

use crate::lower::Tables;
impl Tables {
    pub(super) fn instruction(
        &mut self,
        function: &mut Function,
        source: ir::Instruction,
        constants: &[ir::Constant],
        scratch: Reg,
    ) {
        use ir::Instruction as Old;
        let instruction = match source {
            Old::DynamicGlobal { name } => Instruction::BindDynamicGlobal { name },
            Old::Check { local, ty } => Instruction::CheckType {
                local,
                ty: self.ty(&ty),
            },
            Old::Clear { register } => Instruction::Clear { reg: register },
            Old::DefaultCheck { param, passed } => Instruction::ReceiveDefault {
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
            Old::StaticInit { local, name, src } => {
                let variable = static_id(self, function, &name);
                Instruction::InitStatic {
                    local,
                    variable,
                    src,
                }
            }
            Old::StartCall { name, fallback } => Instruction::InitCall {
                function: self.symbol(&name, fallback, true),
            },
            Old::StartDynamicCall { callee } => Instruction::InitDynamicCall { callee },
            Old::FinishCall { dst } => Instruction::Call { dst },
            Old::FinishCallable { dst } => Instruction::MakeCallable { dst },
            Old::Send { argument } => {
                if let Some(name) = argument.name {
                    function.instructions.push(Instruction::NameArgument {
                        name: self.name(&name),
                    });
                }
                if argument.spread {
                    Instruction::SendSpread {
                        src: argument.value,
                    }
                } else if let Some(place) = argument.place {
                    if !place.dynamic && place.path.is_empty() {
                        Instruction::SendLocal { local: place.base }
                    } else {
                        Instruction::SendPath {
                            path: path(function, place),
                        }
                    }
                } else {
                    Instruction::SendValue {
                        src: argument.value,
                    }
                }
            }
            Old::IncPlace {
                dst,
                place,
                increment,
                prefix,
            } => {
                let mode = match (increment, prefix) {
                    (true, true) => IncDec::PreIncrement,
                    (true, false) => IncDec::PostIncrement,
                    (false, true) => IncDec::PreDecrement,
                    (false, false) => IncDec::PostDecrement,
                };
                if !place.dynamic && place.path.is_empty() {
                    Instruction::IncDecLocal {
                        dst,
                        local: place.base,
                        mode,
                    }
                } else {
                    Instruction::IncDecPath {
                        dst,
                        path: path(function, place),
                        mode,
                    }
                }
            }
            Old::Compound {
                dst,
                place,
                operator,
                source,
            } => {
                let op = binary(operator);
                if !place.dynamic && place.path.is_empty() {
                    function.instructions.push(Instruction::AssignOp {
                        op,
                        local: place.base,
                        src: source,
                    });
                    Instruction::Move {
                        dst,
                        src: place.base,
                    }
                } else {
                    Instruction::AssignOpPath {
                        op,
                        dst,
                        path: path(function, place),
                        src: source,
                    }
                }
            }
            Old::Load { dst, constant } => match constants.get(constant) {
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
            Old::Move { dst, src } => Instruction::Move { dst, src },
            Old::Read { dst, place, quiet } => {
                let mode = if quiet {
                    FetchMode::Quiet
                } else {
                    FetchMode::Read
                };
                if !place.dynamic && place.path.is_empty() && mode == FetchMode::Read {
                    Instruction::Move {
                        dst,
                        src: place.base,
                    }
                } else {
                    Instruction::Fetch {
                        dst,
                        path: path(function, place),
                        mode,
                    }
                }
            }
            Old::Write { place, src } => {
                if !place.dynamic && place.path.is_empty() {
                    Instruction::Assign {
                        local: place.base,
                        src,
                    }
                } else {
                    Instruction::AssignPath {
                        path: path(function, place),
                        src,
                    }
                }
            }
            Old::Reference { dst, place } => Instruction::MakeReference {
                dst,
                path: path(function, place),
            },
            Old::Rebind { place, src } => Instruction::BindReference {
                path: path(function, place),
                src,
            },
            Old::Bind { place, source } => {
                let source = path(function, source);
                function.instructions.push(Instruction::MakeReference {
                    dst: scratch,
                    path: source,
                });
                let target = path(function, place);
                function.instructions.push(Instruction::BindReference {
                    path: target,
                    src: scratch,
                });
                Instruction::Clear { reg: scratch }
            }
            Old::Unset(place) => Instruction::UnsetPath {
                path: path(function, place),
            },
            Old::Binary {
                dst,
                operator: op,
                lhs,
                rhs,
            } => Instruction::Binary {
                dst,
                op: binary(op),
                lhs,
                rhs,
            },
            Old::Unary {
                dst,
                operator: op,
                src,
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
                dst,
                name,
                fallback,
            } => Instruction::FetchConstant {
                dst,
                constant: self.symbol(&name, fallback, false),
            },
            Old::Define { name, src } => Instruction::DeclareConstant {
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
            Old::Include {
                dst,
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
            Old::IterInit { id, src, place } => {
                function.iterators = function.iterators.max(id.0 + 1);
                if let Some(place) = place {
                    Instruction::IterStartReference {
                        iter: IterId(id.0),
                        path: path(function, place),
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
                id,
                key,
                value,
                end,
            } => Instruction::IterNextKeyed {
                iter: IterId(id.0),
                key,
                value,
                end: Target(end.0 as u32),
            },
            Old::MatchError { subject } => Instruction::Raise {
                error: RuntimeError::UnhandledMatch,
                detail: subject,
            },
            Old::Call {
                dst,
                name,
                fallback,
                arguments,
            } => {
                self.init_call(function, name, fallback, constants);
                for argument in arguments {
                    if let Some(name) = argument.name {
                        function.instructions.push(Instruction::NameArgument {
                            name: self.name(&name),
                        });
                    }
                    let send = if argument.spread {
                        Instruction::SendSpread {
                            src: argument.value,
                        }
                    } else if let Some(place) = argument.place {
                        Instruction::SendPath {
                            path: path(function, place),
                        }
                    } else {
                        Instruction::SendValue {
                            src: argument.value,
                        }
                    };
                    function.instructions.push(send);
                }
                Instruction::Call { dst }
            }
        };
        function.instructions.push(instruction);
    }

    fn init_call(
        &mut self,
        function: &mut Function,
        name: Reg,
        fallback: Option<String>,
        _constants: &[ir::Constant],
    ) {
        function
            .instructions
            .push(Instruction::InitDynamicCall { callee: name });
        let _ = fallback;
    }
}

fn path(function: &mut Function, place: ir::Place) -> PathId {
    let index = function.paths.len();
    function.paths.push(Path {
        base: if place.dynamic {
            PathBase::Named(place.base)
        } else {
            PathBase::Register(place.base)
        },
        steps: place
            .path
            .into_iter()
            .map(|step| step.map_or(Step::Append, Step::Index))
            .collect(),
    });
    PathId(index as u32)
}

fn binary(operator: ir::Binary) -> BinaryOp {
    match operator {
        ir::Binary::Add => BinaryOp::Add,
        ir::Binary::Subtract => BinaryOp::Subtract,
        ir::Binary::Multiply => BinaryOp::Multiply,
        ir::Binary::Divide => BinaryOp::Divide,
        ir::Binary::Modulo => BinaryOp::Modulo,
        ir::Binary::Power => BinaryOp::Power,
        ir::Binary::Concat => BinaryOp::Concat,
        ir::Binary::BitwiseAnd => BinaryOp::BitwiseAnd,
        ir::Binary::BitwiseOr => BinaryOp::BitwiseOr,
        ir::Binary::BitwiseXor => BinaryOp::BitwiseXor,
        ir::Binary::ShiftLeft => BinaryOp::ShiftLeft,
        ir::Binary::ShiftRight => BinaryOp::ShiftRight,
        ir::Binary::Xor => BinaryOp::LogicalXor,
        ir::Binary::Equal => BinaryOp::Equal,
        ir::Binary::NotEqual => BinaryOp::NotEqual,
        ir::Binary::Identical => BinaryOp::Identical,
        ir::Binary::NotIdentical => BinaryOp::NotIdentical,
        ir::Binary::Less => BinaryOp::Less,
        ir::Binary::LessEqual => BinaryOp::LessEqual,
        ir::Binary::Greater => BinaryOp::Greater,
        ir::Binary::GreaterEqual => BinaryOp::GreaterEqual,
        ir::Binary::Spaceship => BinaryOp::Spaceship,
    }
}

fn static_id(tables: &mut Tables, function: &mut Function, name: &str) -> StaticId {
    let name = tables.name(name);
    let index = function
        .statics
        .iter()
        .position(|candidate| candidate == &name)
        .unwrap_or_else(|| {
            function.statics.push(name);
            function.statics.len() - 1
        });
    StaticId(index as u16)
}
