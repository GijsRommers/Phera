//! Intern compiler IR paths and translate operator tags into plain bytecode data.

use phera_bytecode::{BinaryOp, Function, Path, PathBase, PathId, StaticId, Step};

use crate::{ir, passes::lower::Tables};

/// Append an access path with evaluated keys; absent keys denote array append.
pub(crate) fn path(function: &mut Function, place: ir::Place) -> PathId {
    let index = function.paths.len();
    function.paths.push(Path {
        base: match place.scope {
            ir::PlaceScope::Local => PathBase::Register(place.base),
            ir::PlaceScope::Dynamic => PathBase::Named(place.base),
            ir::PlaceScope::Global(scratch) => PathBase::GlobalNamed {
                name: place.base,
                scratch,
            },
            ir::PlaceScope::Globals => PathBase::Globals,
        },
        steps: place
            .path
            .into_iter()
            .map(|step| step.map_or(Step::Append, Step::Index))
            .collect(),
    });
    PathId(index as u32)
}

/// Translate supported binary operators without folding or reordering operands.
pub(crate) fn binary(operator: ir::Binary) -> BinaryOp {
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

/// Reuse the function-local static slot for repeated occurrences of a name.
pub(crate) fn static_id(tables: &mut Tables, function: &mut Function, name: &str) -> StaticId {
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
