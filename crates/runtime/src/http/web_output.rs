//! Maps PHP response headers into the transport when a script calls `flush()`.

use std::future::Future;
use std::pin::Pin;

use phera_ext_http::ResponseHead;
use phera_extension_api::RequestState;

/// An embedder can take over the response the first time PHP sends it early.
pub trait WebOutput {
    fn is_disconnected(&self) -> bool;
    fn start(&mut self, head: ResponseHead);
    fn write(&mut self, bytes: Vec<u8>) -> Pin<Box<dyn Future<Output = bool> + '_>>;
    fn finish(&mut self, head: ResponseHead, bytes: Vec<u8>);
}

pub(crate) struct Transport<T>(pub(crate) T);

impl<T: WebOutput> phera_vm::WebOutput for Transport<T> {
    fn is_disconnected(&self) -> bool {
        self.0.is_disconnected()
    }

    fn start(&mut self, state: &RequestState) {
        self.0.start(head(state));
    }

    fn write(&mut self, bytes: Vec<u8>) -> Pin<Box<dyn Future<Output = bool> + '_>> {
        self.0.write(bytes)
    }

    fn finish(&mut self, state: &RequestState, bytes: Vec<u8>) {
        self.0.finish(head(state), bytes);
    }
}

fn head(state: &RequestState) -> ResponseHead {
    state.get::<ResponseHead>().cloned().unwrap_or_default()
}
