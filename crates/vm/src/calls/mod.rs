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
pub(crate) mod call_view;
pub(crate) mod callback_arguments;
pub(crate) mod callback_binding;
pub(crate) mod callback_execution;
pub(crate) mod callback_references;
mod discarded_results;
pub(crate) mod execution_loop;
mod fast_arguments;
mod fast_calls;
mod fast_class_names;
mod fast_compound_assignments;
mod fast_dimensions;
mod fast_element_writes;
mod fast_foreach;
mod fast_instance_of;
mod fast_methods;
mod fast_named_calls;
mod fast_native_methods;
mod fast_objects;
mod fast_path_sends;
mod fast_properties;
mod fast_property_elements;
pub(crate) mod frame;
pub(crate) mod frame_extras;
pub(crate) mod frame_view;
mod further_steps;
pub(crate) mod magic_dispatch;
mod native;
mod native_calls;
mod native_defaults;
pub(crate) mod native_frames;
mod native_function;
mod native_loops;
mod native_method;
mod native_ready;
mod native_retries;
mod native_signature;
mod native_sorts;
pub(crate) mod parameter_binding;
pub(crate) mod pending;
pub(crate) mod php_windows;
pub(crate) mod prepared_call;
pub(crate) mod registers;
pub(crate) mod returns;
mod scope;
pub(crate) mod sends;
pub(crate) mod sync_native;
mod sync_native_method;
#[cfg(test)]
mod tests;
mod user_callback;
pub(crate) mod window_context;

mod string_arguments;
