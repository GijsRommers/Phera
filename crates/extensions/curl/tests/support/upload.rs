//! Consume streaming uploads incrementally and report the bytes seen.

use http_body_util::BodyExt;
use hyper::{Request, Response, body::Incoming};
use sha2::{Digest, Sha256};

use super::{Body, full};

pub(super) async fn receive(request: Request<Incoming>) -> Response<Body> {
    let redirect = request.uri().path() == "/upload-redirect";
    let mut count = 0;
    let mut digest = Sha256::new();
    let mut body = request.into_body();
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => {
                if let Some(bytes) = frame.data_ref() {
                    count += bytes.len();
                    digest.update(bytes);
                }
            }
            Err(_) => {
                return Response::builder()
                    .status(400)
                    .body(full("incomplete upload"))
                    .unwrap();
            }
        }
    }
    if redirect {
        Response::builder()
            .status(307)
            .header("location", "/upload-size")
            .body(full("redirect upload"))
            .unwrap()
    } else {
        Response::new(full(format!("{count}:{:x}", digest.finalize())))
    }
}
