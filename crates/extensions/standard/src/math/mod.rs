mod basic;
mod minmax;
mod misc;
mod trig;
use phera_extension_api::*;
pub(crate) fn register(r: &mut Registry) {
    basic::register(r);
    minmax::register(r);
    misc::register(r);
    trig::register(r);
}
