//! Adapts PHP's `flush()` and `fastcgi_finish_request()` to the HTTP server.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use phera_runtime::{ResponseHead, WebOutput};
use phera_server::{Response, ResponseWriter, StreamWriter};

use super::compress;

pub(super) struct HttpOutput {
    response: ResponseWriter,
    stream: Option<StreamWriter>,
    compress: bool,
    accept_encoding: Option<Rc<str>>,
}

impl HttpOutput {
    pub(super) fn new(
        response: ResponseWriter,
        compress: bool,
        accept_encoding: Option<Rc<str>>,
    ) -> Self {
        response.watch_disconnect();
        Self {
            response,
            stream: None,
            compress,
            accept_encoding,
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

    fn end_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.finish();
        }
    }

    fn finish_buffered(
        &mut self,
        head: ResponseHead,
        bytes: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = bool> + '_>> {
        Box::pin(async move {
            let mut response = to_response(head, bytes);
            if self.compress {
                response = compress::php_response(response, self.accept_encoding.as_deref()).await;
            }
            self.response.finish(response)
        })
    }
}

fn to_response(head: ResponseHead, body: Vec<u8>) -> Response {
    Response {
        status: head.status,
        headers: head.headers,
        body: phera_server::Body::Bytes(body),
    }
}
