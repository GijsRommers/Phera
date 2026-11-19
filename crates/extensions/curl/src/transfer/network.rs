//! With `CURLOPT_FOLLOWLOCATION`, each redirect retains the transfer deadline.

use std::{cell::Cell, rc::Rc, time::Instant};

use url::Url;

use crate::{
    connector,
    cookies::CookieJar,
    easy::Handle,
    error::TransferError,
    info::Info,
    options::{Method, Settings},
    share::SharedDns,
};

mod response;

use super::{Event, Sender, download, request, upload::Payload};

pub(super) struct Network {
    pub(super) settings: Settings,
    pub(super) handle: Handle,
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
                            "Operation timed out after {} milliseconds with {} bytes received",
                            started.elapsed().as_millis(),
                            received,
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
        let mut configuration = self.settings.clone();
        let mut url = configuration
            .normalized_url()
            .ok_or_else(|| url_error(&configuration))?;
        if let Some(port) = configuration.port {
            let _ = url.set_port(Some(port));
        }
        let original = url.origin();
        let mut method = configuration.method;
        let mut referer = configuration.referer.clone();
        let mut follow = configuration.follow;
        let mut first = true;
        loop {
            let settings = &configuration;
            info.url = Some(url.to_string());
            if !settings.protocols.permits(url.scheme())
                || (!first && !settings.redirect_protocols.permits(url.scheme()))
            {
                return Err(TransferError::new(
                    1,
                    format!("Protocol \"{}\" not supported", url.scheme()),
                ));
            }
            info.scheme = Some(url.scheme().to_owned());
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
            info.timing.queue = started.elapsed();
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
            response::info(info, &response, started);
            info.upload_length = payload.has_body.then_some(payload.bytes.len() as i64);
            info.uploaded += payload.bytes.len() as i64;
            info.timing.sent = info.timing.pretransfer.max(info.timing.queue);
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
            let lines = response::headers(&response);
            info.header_size += lines.iter().map(|line| line.len() as i64).sum::<i64>();
            let (acknowledge, callback) = tokio::sync::oneshot::channel();
            if sender
                .send(Event::Head(Box::new(info.clone()), lines, acknowledge))
                .await
                .is_err()
                || callback.await.is_err()
            {
                return Ok(());
            }
            follow = follow && self.handle.borrow().settings.follow;
            if settings.fail_on_error && status >= 400 {
                return Err(TransferError::new(
                    22,
                    format!("The requested URL returned error: {status}"),
                ));
            }
            if let Some(target) = redirect.filter(|_| follow) {
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
                let current = &self.handle.borrow().settings;
                configuration.headers.clone_from(&current.headers);
                configuration
                    .custom_method
                    .clone_from(&current.custom_method);
                configuration
                    .transport
                    .proxy
                    .clone_from(&current.transport.proxy);
                configuration.transport.http = current.transport.http;
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
