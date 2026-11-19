//! `openssl_error_string()` drains errors from earlier cryptographic operations.

use std::collections::VecDeque;

use phera_extension_api::{Context, FunctionEntry, NativeResult, Value};

pub(crate) const ENTRY: FunctionEntry = FunctionEntry::sync("openssl_error_string", &[], pop);
pub(crate) const INVALID_IV: &str = "error:1C80006D:Provider routines::invalid iv length";
pub(crate) const INVALID_TAG: &str = "error:1C80006E:Provider routines::invalid tag";
pub(crate) const INVALID_TAG_LENGTH: &str = "error:1C800076:Provider routines::invalid tag length";
pub(crate) const BAD_DECRYPT: &str = "error:1C800064:Provider routines::bad decrypt";
pub(crate) const WRONG_BLOCK_LENGTH: &str =
    "error:1C80006B:Provider routines::wrong final block length";
pub(crate) const INVALID_KEY: &str = "error:03000082:digital envelope routines::invalid key length";
pub(crate) const INVALID_CHACHA_KEY: &str = "error:1C800069:Provider routines::invalid key length";

#[derive(Default)]
struct ErrorQueue(VecDeque<&'static str>);

pub(crate) fn push(context: &mut dyn Context, message: &'static str) {
    // OpenSSL's ring reserves one of its sixteen slots.
    let queue = &mut context.state().get_or_default::<ErrorQueue>().0;
    if queue.len() == 15 {
        queue.pop_front();
    }
    queue.push_back(message);
}

fn pop(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let queue = &mut context.state().get_or_default::<ErrorQueue>().0;
    Ok(queue.pop_front().map_or(Value::Bool(false), Value::from))
}
