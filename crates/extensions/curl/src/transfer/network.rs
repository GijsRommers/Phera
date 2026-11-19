//! With `CURLOPT_FOLLOWLOCATION`, each redirect retains the transfer deadline.

use std::{cell::Cell, rc::Rc, time::Instant};

use hyper::{Response, Version, body::Incoming};
use url::Url;

use crate::{
    connector::{self, ConnectionInfo},
    cookies::CookieJar,
    error::TransferError,
    info::Info,
    options::{Method, Settings},
    share::SharedDns,
};

use super::{Event, Sender, download, request, upload::Payload};

pub(super) struct Network {
    pub(super) settings: Settings,
    pub(super) payload: Payload,
    pub(super) jar: CookieJar,
    pub(super) cookie_engine: bool,
    pub(super) dns: Option<SharedDns>,
}

impl Network {
    pub(super) async fn run(self, sender: Sender, started: Instant) {
        let mut info = Info::default();
        let wire_bytes = Rc::new(Cell::new(0));
        let transfer = self.run_redirects(&sender, &mut info, started, &wire_bytes);
        let result = if let Some(timeout) = self.settings.timeout {
            let remaining = timeout.saturating_sub(started.elapsed());
            match tokio::time::timeout(remaining, transfer).await {
                Ok(result) => result,
                Err(_) => {
                    let received = wire_bytes.get();
                    Err(TransferError::new(
                        28,
                        format!(
                            "Operation timed out after {} milliseconds with {received} bytes received",
                            started.elapsed().as_millis(),
                        ),
                    ))
                }
            }
        } else {
            transfer.await
        };
        info.timing.total = started.elapsed();
        info.downloaded = wire_bytes.get();
        if let Some(TransferError { code: 60, .. }) = &result.as_ref().err() {
            info.ssl_verify_result = 1;
        }
        let _ = sender.send(Event::Done(Box::new(info), result.err())).await;
    }

    async fn run_redirects(
        &self,
        sender: &Sender,
        info: &mut Info,
        started: Instant,
        wire: &Rc<Cell<i64>>,
    ) -> Result<(), TransferError> {
        let settings = &self.settings;
        let mut url = settings
            .normalized_url()
            .ok_or_else(|| url_error(settings))?;
        if let Some(port) = settings.port {
            let _ = url.set_port(Some(port));
        }
        let original = url.origin();
        let mut method = settings.method;
        let mut referer = settings.referer.clone();
        let mut first = true;
        loop {
            info.url = Some(url.to_string());
            if !settings.protocols.permits(url.scheme())
                || (!first && !settings.redirect_protocols.permits(url.scheme()))
            {
                return Err(TransferError::new(
                    1,
                    format!("Protocol \"{}\" not supported", url.scheme()),
                ));
            }
            let actual_method = settings.custom_method.as_deref().unwrap_or(method.name());
            info.method = Some(actual_method.to_owned());
            let empty = Payload::default();
            let payload = if matches!(method, Method::Get | Method::Head) {
                &empty
            } else {
                &self.payload
            };
            let origin_credentials = settings.unrestricted_auth || original == url.origin();
            let cookies = if self.cookie_engine {
                self.jar.borrow().header(&url)
            } else {
                String::new()
            };
            let (client, proxy_auth) = connector::client(settings, &url, self.dns.clone()).await?;
            let (request, auth) = request::build(
                &url,
                actual_method,
                payload,
                settings,
                request::Privacy {
                    accept_credentials: origin_credentials,
                    cookies: &cookies,
                    referer: referer.as_deref(),
                    proxy_auth: proxy_auth.as_deref(),
                },
            )?;
            let header = request::capture(&request);
            info.request_size += header.len() as i64;
            if settings.header_out {
                info.request_header = Some(header);
            }
            info.auth = auth;
            info.referer = referer.clone();
            if settings.verbose {
                let message = format!(
                    "* {actual_method} {}{}\n",
                    url.origin().ascii_serialization(),
                    url.path()
                );
                if sender.send(Event::Trace(message)).await.is_err() {
                    return Ok(());
                }
            }
            wire.set(0);
            let response = client.request(request);
            let speed = download::LowSpeed::new(settings);
            let response = if let Some(deadline) = speed.deadline() {
                tokio::time::timeout_at(deadline, response)
                    .await
                    .map_err(|_| speed.error())?
            } else {
                response.await
            }
            .map_err(client_error)?;
            response_info(info, &response, &url, started);
            info.upload_length = payload.has_body.then_some(payload.bytes.len() as i64);
            info.uploaded += payload.bytes.len() as i64;
            info.timing.sent = info.timing.first_byte;
            if self.cookie_engine {
                let mut jar = self.jar.borrow_mut();
                for value in response.headers().get_all("set-cookie") {
                    if let Ok(value) = value.to_str() {
                        jar.parse(value, &url);
                    }
                }
            }
            if settings.file_time {
                info.filetime = response
                    .headers()
                    .get("last-modified")
                    .and_then(|time| {
                        httpdate::parse_http_date(time.to_str().ok()?)
                            .ok()?
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()
                    })
                    .map(|time| time.as_secs() as i64);
            }
            let status = response.status().as_u16();
            let redirect = if matches!(status, 301 | 302 | 303 | 307 | 308) {
                response
                    .headers()
                    .get("location")
                    .and_then(|header| header.to_str().ok())
                    .and_then(|target| url.join(target).ok())
            } else {
                None
            };
            info.redirect_url = redirect.as_ref().map(Url::to_string);
            let lines = headers(&response);
            info.header_size += lines.iter().map(|line| line.len() as i64).sum::<i64>();
            if sender
                .send(Event::Head(Box::new(info.clone()), lines))
                .await
                .is_err()
            {
                return Ok(());
            }
            if settings.fail_on_error && status >= 400 {
                return Err(TransferError::new(
                    22,
                    format!("The requested URL returned error: {status}"),
                ));
            }
            if let Some(target) = redirect.filter(|_| settings.follow) {
                if settings.max_redirects >= 0 && info.redirect_count >= settings.max_redirects {
                    return Err(TransferError::new(
                        47,
                        format!("Maximum ({}) redirects followed", settings.max_redirects),
                    ));
                }
                download::deliver(response.into_body(), None, settings, sender, wire, true).await?;
                if settings.auto_referer {
                    let _ = url.set_password(None);
                    let _ = url.set_username("");
                    url.set_fragment(None);
                    referer = Some(url.as_str().as_bytes().to_vec());
                }
                method = request::redirect_method(status, method, settings.post_redirect);
                url = target;
                info.redirect_count += 1;
                info.timing.redirect = started.elapsed();
                first = false;
                continue;
            }
            if settings.max_file_size != 0
                && info
                    .download_length
                    .is_some_and(|length| length >= 0 && length as u64 > settings.max_file_size)
            {
                return Err(TransferError::new(63, "Maximum file size exceeded"));
            }
            let encoding = response
                .headers()
                .get("content-encoding")
                .and_then(|value| value.to_str().ok())
                .map(str::to_ascii_lowercase);
            let result = download::deliver(
                response.into_body(),
                encoding.as_deref(),
                settings,
                sender,
                wire,
                false,
            )
            .await;
            if let Err(error) = &result
                && matches!(error.code, 18 | 56)
                && !matches!(method, Method::Head)
                && let Some(length) = info.download_length
                && length > wire.get()
            {
                let missing = length - wire.get();
                return Err(TransferError::new(
                    18,
                    format!("end of response with {missing} bytes missing"),
                ));
            }
            return result;
        }
    }
}

fn url_error(settings: &Settings) -> TransferError {
    match settings.url.as_deref() {
        None => TransferError::new(3, "No URL set"),
        Some(url) if url.trim().ends_with("://") => {
            TransferError::new(3, "URL rejected: No host present")
        }
        Some(_) => TransferError::new(3, "URL rejected: Malformed input to a URL function"),
    }
}

fn client_error(error: hyper_util::client::legacy::Error) -> TransferError {
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        if let Some(transport) = cause.downcast_ref::<TransferError>() {
            return transport.clone();
        }
        if let Some(hyper) = cause.downcast_ref::<hyper::Error>() {
            if hyper.is_incomplete_message() {
                return TransferError::new(52, "Empty reply from server");
            }
            if hyper.is_parse() {
                return TransferError::new(8, format!("Invalid HTTP response: {hyper}"));
            }
        }
        source = cause.source();
    }
    if error.is_connect() {
        TransferError::new(7, format!("Could not connect to server: {error}"))
    } else {
        TransferError::new(
            56,
            format!("Failure when receiving data from the peer: {error}"),
        )
    }
}

fn response_info(info: &mut Info, response: &Response<Incoming>, url: &Url, started: Instant) {
    let duration = |instant: Instant| instant.saturating_duration_since(started);
    info.status = i64::from(response.status().as_u16());
    info.scheme = Some(url.scheme().to_owned());
    info.http_version = match response.version() {
        Version::HTTP_10 => 1,
        Version::HTTP_11 => 2,
        Version::HTTP_2 => 3,
        _ => 0,
    };
    info.content_type = response
        .headers()
        .get("content-type")
        .map(|value| value.as_bytes().to_vec());
    info.download_length = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok()?.parse::<i64>().ok());
    info.auth_available = request::auth_available(response.headers(), "www-authenticate");
    info.proxy_auth_available = request::auth_available(response.headers(), "proxy-authenticate");
    info.retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| {
            let value = value.to_str().ok()?;
            value.parse().ok().or_else(|| {
                let date = httpdate::parse_http_date(value).ok()?;
                let future = date.duration_since(std::time::SystemTime::now()).ok()?;
                Some(future.as_secs() as i64)
            })
        })
        .unwrap_or(0);
    info.timing.first_byte = started.elapsed();
    if let Some(connection) = response.extensions().get::<ConnectionInfo>() {
        info.peer = connection.peer;
        info.local = connection.local;
        if connection.connected >= started && info.connection_id != Some(connection.id) {
            info.new_connections += 1;
            info.timing.lookup = duration(connection.lookup);
            info.timing.connect = duration(connection.connected);
            info.timing.tls = connection.tls.map_or(std::time::Duration::ZERO, duration);
        }
        info.timing.pretransfer = duration(connection.tls.unwrap_or(connection.connected));
        info.connection_id = Some(connection.id);
        info.proxy = connection.proxy;
        info.proxy_connect_code = connection.proxy_connect_code;
        info.proxy_auth = i64::from(connection.proxy_auth);
    }
}

fn headers(response: &Response<Incoming>) -> Vec<Vec<u8>> {
    let version = match response.version() {
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_2 => "HTTP/2",
        _ => "HTTP/1.1",
    };
    let status = response.status();
    let reason = if response.version() == Version::HTTP_2 {
        ""
    } else {
        status.canonical_reason().unwrap_or("")
    };
    let mut headers = vec![format!("{version} {} {reason}\r\n", status.as_u16()).into_bytes()];
    for (name, value) in response.headers() {
        headers.push(request::line(name.as_str(), value.as_bytes()));
    }
    headers.push(b"\r\n".to_vec());
    headers
}
