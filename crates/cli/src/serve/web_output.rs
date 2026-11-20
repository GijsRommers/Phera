//! Adapts PHP's `flush()` and `fastcgi_finish_request()` to the HTTP server.

use std::future::Future;
use std::pin::Pin;

use phera_runtime::{ResponseHead, WebOutput};
use phera_server::{Response, ResponseWriter, StreamWriter};

pub(super) struct HttpOutput {
    response: ResponseWriter,
    stream: Option<StreamWriter>,
}

impl HttpOutput {
    pub(super) fn new(response: ResponseWriter) -> Self {
        response.watch_disconnect();
        Self {
            response,
            stream: None,
        }
    }
}

impl WebOutput for HttpOutput {
    fn is_disconnected(&self) -> bool {
        self.response.is_disconnected()
            || self
                .stream
                .as_ref()
                .is_some_and(StreamWriter::is_disconnected)
    }

    fn start(&mut self, head: ResponseHead) {
        self.stream = self.response.stream(to_response(head, Vec::new()));
    }

    fn write(&mut self, bytes: Vec<u8>) -> Pin<Box<dyn Future<Output = bool> + '_>> {
        Box::pin(async move {
            match &self.stream {
                Some(stream) => stream.write(bytes).await,
                None => !self.response.is_disconnected(),
            }
        })
    }

    fn finish(&mut self, head: ResponseHead, bytes: Vec<u8>) {
        if let Some(stream) = self.stream.take() {
            stream.finish();
        } else {
            self.response.finish(to_response(head, bytes));
        }
    }
}

fn to_response(head: ResponseHead, body: Vec<u8>) -> Response {
    Response {
        status: head.status,
        headers: head.headers,
        body: phera_server::Body::Bytes(body),
    }
}
