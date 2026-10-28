//! The contract between the runtime and Phera extensions.
//!
//! Extensions only see the types defined here, never the VM's heap or register
//! layout, so the VM can change its internals without breaking extensions.

mod error;
mod registry;
mod value;

pub use error::CallError;
pub use registry::{NativeFunction, Registry};
pub use value::Value;

/// A compile-time linked bundle of PHP functions and constants.
pub trait Extension {
    fn name(&self) -> &'static str;

    fn register(&self, registry: &mut Registry);
}
