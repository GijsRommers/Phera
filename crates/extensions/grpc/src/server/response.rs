//! Writing one call's response: headers, message frames, then trailers
//! with the status. A call that fails before any message gets one HEADERS
//! frame with the status in it ("trailers-only"), as gRPC specifies.

use bytes::Bytes;
use h2::SendStream;
use h2::server::SendResponse;
use http::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use http::{Response, StatusCode};

use crate::framing::Encoding;
use crate::headers::encode_message;
use crate::status::Status;
use crate::transport;

const GRPC: HeaderValue = HeaderValue::from_static("application/grpc");
const GZIP: HeaderValue = HeaderValue::from_static("gzip");
pub const GRPC_STATUS: HeaderName = HeaderName::from_static("grpc-status");
pub const GRPC_MESSAGE: HeaderName = HeaderName::from_static("grpc-message");
pub const GRPC_ENCODING: HeaderName = HeaderName::from_static("grpc-encoding");
pub const GRPC_ACCEPT_ENCODING: HeaderName = HeaderName::from_static("grpc-accept-encoding");

pub struct Responder {
    respond: SendResponse<Bytes>,
    stream: Option<SendStream<Bytes>>,
    pub encoding: Encoding,
}

impl Responder {
    pub fn new(respond: SendResponse<Bytes>, encoding: Encoding) -> Self {
        Self {
            respond,
            stream: None,
            encoding,
        }
    }

    pub fn started(&self) -> bool {
        self.stream.is_some()
    }

    /// Sends the response headers, with the handler's own added.
    pub fn start(&mut self, extra: HeaderMap) -> Result<(), h2::Error> {
        if self.stream.is_some() {
            return Ok(());
        }
        let mut response = Response::new(());
        let headers = response.headers_mut();
        *headers = extra;
        headers.insert(CONTENT_TYPE, GRPC);
        headers.insert(GRPC_ACCEPT_ENCODING, GZIP);
        if self.encoding == Encoding::Gzip {
            headers.insert(GRPC_ENCODING, GZIP);
        }
        self.stream = Some(self.respond.send_response(response, false)?);
        Ok(())
    }

    /// Sends one framed message; the headers must have been started.
    pub async fn send(&mut self, frame: Bytes) -> Result<(), h2::Error> {
        if self.stream.is_none() {
            self.start(HeaderMap::new())?;
        }
        match &mut self.stream {
            Some(stream) => transport::send(stream, frame, false).await,
            None => Ok(()),
        }
    }

    /// Ends the call with `status` and the handler's trailers.
    pub fn finish(
        mut self,
        status: &Status,
        mut trailers: HeaderMap,
        headers: HeaderMap,
    ) -> Result<(), h2::Error> {
        put_status(&mut trailers, status);
        match self.stream.as_mut() {
            Some(stream) => stream.send_trailers(trailers),
            None => {
                // Trailers-only: everything in the one HEADERS frame.
                let mut response = Response::new(());
                let all = response.headers_mut();
                *all = headers;
                all.insert(CONTENT_TYPE, GRPC);
                all.insert(GRPC_ACCEPT_ENCODING, GZIP);
                for (name, value) in trailers.iter() {
                    all.append(name.clone(), value.clone());
                }
                self.respond.send_response(response, true).map(drop)
            }
        }
    }

    /// The client is gone or the server is stopping mid-call.
    pub fn reset(mut self) {
        match self.stream.as_mut() {
            Some(stream) => stream.send_reset(h2::Reason::CANCEL),
            None => self.respond.send_reset(h2::Reason::CANCEL),
        }
    }
}

fn put_status(headers: &mut HeaderMap, status: &Status) {
    headers.insert(GRPC_STATUS, HeaderValue::from(status.code));
    if !status.message.is_empty()
        && let Ok(message) = HeaderValue::from_str(&encode_message(&status.message))
    {
        headers.insert(GRPC_MESSAGE, message);
    }
}

/// A plain HTTP error, for requests that are not gRPC at all.
pub fn http_error(mut respond: SendResponse<Bytes>, status: StatusCode) {
    let mut response = Response::new(());
    *response.status_mut() = status;
    let _ = respond.send_response(response, true);
}
