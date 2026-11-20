//! Adapts PHP's `flush()` and `fastcgi_finish_request()` to the HTTP server.

use std::future::Future;
use std::pin::Pin;

use phera_runtime::{ResponseHead, WebOutput};
use phera_server::{Response, ResponseWriter, StreamWriter};

use super::compress;

pub(super) struct HttpOutput {
    target: Target,
    compress: bool,
    accept_encoding: Option<String>,
}

enum Target {
    Buffered(ResponseWriter),
    Streaming(StreamWriter),
    Complete,
}

impl HttpOutput {
    pub(super) fn new(
        response: ResponseWriter,
        compress: bool,
        accept_encoding: Option<String>,
    ) -> Self {
        Self {
            target: Target::Buffered(response),
            compress,
            accept_encoding,
        }
    }
}

impl WebOutput for HttpOutput {
    fn is_disconnected(&self) -> bool {
        match &self.target {
            Target::Buffered(response) => response.is_disconnected(),
            Target::Streaming(stream) => stream.is_disconnected(),
            Target::Complete => false,
        }
    }

    fn start(&mut self, head: ResponseHead) {
        if let Target::Buffered(response) = std::mem::replace(&mut self.target, Target::Complete)
            && let Some(stream) = response.stream(to_response(head, Vec::new()))
        {
            self.target = Target::Streaming(stream);
        }
    }

    fn write(&mut self, bytes: Vec<u8>) -> Pin<Box<dyn Future<Output = bool> + '_>> {
        Box::pin(async move {
            match &self.target {
                Target::Streaming(stream) => stream.write(bytes).await,
                _ => true,
            }
        })
    }

    fn end_stream(&mut self) {
        if let Target::Streaming(stream) = std::mem::replace(&mut self.target, Target::Complete) {
            stream.finish();
        }
    }

    fn finish_buffered(
        &mut self,
        head: ResponseHead,
        bytes: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = bool> + '_>> {
        Box::pin(async move {
            let Target::Buffered(writer) = std::mem::replace(&mut self.target, Target::Complete)
            else {
                return false;
            };
            let mut response = to_response(head, bytes);
            if self.compress {
                response = compress::php_response(response, self.accept_encoding.as_deref()).await;
            }
            writer.finish(response)
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
