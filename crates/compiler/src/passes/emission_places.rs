//! Translate IR access and call operations into bytecode, preserving instruction order.

use phera_bytecode::{FetchMode, Function, Instruction, Reg};

use crate::{
    ir,
    passes::lower::Tables,
    passes::lowering_tables::{binary, path},
};

impl Tables {
    /// Send a place when the callee may require a reference.
    pub(crate) fn send_argument(
        &mut self,
        function: &mut Function,
        argument: ir::Argument,
    ) -> Instruction {
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

    /// Choose the local fast path only for a direct register place.
    pub(crate) fn increment_place(
        &mut self,
        function: &mut Function,
        dst: Reg,
        place: ir::Place,
        mode: phera_bytecode::IncDec,
    ) -> Instruction {
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

    /// Keep the extra result copy after local compound assignment.
    pub(crate) fn compound_assignment(
        &mut self,
        function: &mut Function,
        dst: Reg,
        place: ir::Place,
        operator: ir::Binary,
        source: Reg,
    ) -> Instruction {
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

    /// Quiet reads use a path even for a local to suppress undefined warnings.
    pub(crate) fn read_place(
        &mut self,
        function: &mut Function,
        dst: Reg,
        place: ir::Place,
        mode: phera_bytecode::FetchMode,
    ) -> Instruction {
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

    /// Write through a path unless the place is a direct local.
    pub(crate) fn write_place(
        &mut self,
        function: &mut Function,
        place: ir::Place,
        src: Reg,
    ) -> Instruction {
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

    /// Clear the reference scratch register after binding the destination.
    pub(crate) fn bind_reference(
        &mut self,
        function: &mut Function,
        place: ir::Place,
        source: ir::Place,
        scratch: Reg,
    ) -> Instruction {
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

    /// Prepare the callee, send every argument, then invoke the call.
    pub(crate) fn dynamic_call(
        &mut self,
        function: &mut Function,
        dst: Reg,
        name: Reg,
        fallback: Option<String>,
        arguments: Vec<ir::Argument>,
    ) -> Instruction {
        function
            .instructions
            .push(Instruction::InitDynamicCall { callee: name });
        let _ = fallback;
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
}
