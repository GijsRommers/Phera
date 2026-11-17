//! A call reserves registers before evaluating its arguments.
//!
//! For `f(1, g(2))`, Send writes directly into the reserved parameter slots.
//! The inner call is above the unfinished outer call; returning releases only g.
//!
//! ```text
//! low addresses                              high addresses
//! [caller locals][f: 1, undefined, locals][g: 2, locals]
//!                  waiting                 executing
//! ```
//!
//! Call turns the filled window into an activation; Return releases it. Named
//! arguments, unpacking, native callbacks and exception unwinding use the same stack.

pub(crate) mod argument_errors;
mod argument_types;
pub(crate) mod call_execution;
pub(crate) mod call_instructions;
pub(crate) mod call_stack;
pub(crate) mod call_state;
pub(crate) mod callback_arguments;
pub(crate) mod callback_binding;
pub(crate) mod callback_execution;
pub(crate) mod callback_references;
pub(crate) mod execution_loop;
pub(crate) mod frame;
pub(crate) mod frame_extras;
mod native;
pub(crate) mod parameter_binding;
pub(crate) mod pending;
pub(crate) mod registers;
pub(crate) mod returns;
mod scope;
pub(crate) mod sends;
#[cfg(test)]
mod tests;
