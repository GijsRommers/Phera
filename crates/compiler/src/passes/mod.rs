//! Emit bytecode from IR, then optimize operands without changing PHP evaluation order.

pub(crate) mod direct_calls;
pub(crate) mod direct_operands;
pub(crate) mod emission_places;
pub(crate) mod integer_operands;
pub(crate) mod lower;
pub(crate) mod lowering;
pub(crate) mod lowering_tables;
pub(crate) mod optimize;
pub(crate) mod register_mapping;

mod object_liveness;
