//! `curl_exec` and `curl_multi_exec` share the response and callback path.

mod body;
mod download;
mod network;
mod output;
mod request;
mod upload;

pub(crate) use body::RequestBody;

use std::{
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::{Context as TaskContext, Poll},
    time::Instant,
};

use bytes::Bytes;
use phera_extension_api::{Context, LocalFuture, NativeError, Object, Value};
use tokio::sync::{
    mpsc::{self, Receiver},
    oneshot,
};

use crate::{
    cookies::{self, CookieJar},
    easy::Handle,
    error::TransferError,
    info::Info,
    options::{Output, Settings},
};

type Sender = mpsc::Sender<Event>;

enum Event {
    Head(Box<Info>, Vec<Vec<u8>>, oneshot::Sender<()>),
    Body(Bytes, i64),
    Trace(String),
    Upload(upload::ReadRequest),
    Done(Box<Info>, Option<TransferError>),
}

#[derive(Clone, Copy)]
pub(crate) struct PoolOptions {
    pub(crate) max_cached: usize,
    pub(crate) multiplex: bool,
}

impl Default for PoolOptions {
    fn default() -> Self {
        Self {
            max_cached: 0,
            multiplex: true,
        }
    }
}

pub(crate) struct Transfer {
    object: Rc<Object>,
    handle: Handle,
    settings: Settings,
    cookies: CookieJar,
    network: Option<LocalFuture<'static, ()>>,
    receiver: Receiver<Event>,
    buffered: Option<Event>,
    content: Vec<u8>,
    started: Instant,
}

impl Transfer {
    pub(crate) async fn start(
        context: &mut dyn Context,
        object: Rc<Object>,
        handle: Handle,
        pool: PoolOptions,
    ) -> Result<Self, NativeError> {
        let started = Instant::now();
        let (settings, cookies, cookie_engine, dns) = {
            let mut easy = handle.borrow_mut();
            easy.active = true;
            easy.paused = false;
            easy.error = None;
            easy.content = None;
            easy.info = Info::default();
            let dns = easy
                .share
                .as_ref()
                .and_then(|share| share.borrow().dns.clone());
            let function = if easy
                .multi
                .as_ref()
                .and_then(|multi| multi.upgrade())
                .is_some()
            {
                "curl_multi_exec"
            } else {
                "curl_exec"
            };
            easy.settings.discard_closed_streams(context, function);
            let mut settings = easy.settings.clone();
            if pool.max_cached != 0 {
                settings.pool_idle = settings.pool_idle.min(pool.max_cached);
            }
            settings.multiplex = pool.multiplex;
            settings.pool_scope = easy
                .share
                .as_ref()
                .map_or(0, |share| share.borrow().pool_scope);
            (settings, easy.cookies(), easy.cookie_engine(), dns)
        };
        let (sender, receiver) = mpsc::channel(1);
        // Own the active flag before performing a PHP callback that can throw or suspend.
        let mut transfer = Self {
            object,
            handle,
            settings,
            cookies,
            network: None,
            receiver,
            buffered: None,
            content: Vec::new(),
            started,
        };
        let settings = &transfer.settings;
        let initial_progress = if !settings.no_progress
            && settings.progress.is_some()
            && settings
                .normalized_url()
                .is_some_and(|url| settings.protocols.permits(url.scheme()))
        {
            output::progress(&transfer, context).await?
        } else {
            Ok(())
        };
        let result = match initial_progress {
            Err(error) => Err(error),
            Ok(()) => {
                cookies::load(
                    context,
                    &transfer.cookies,
                    &settings.cookie_files,
                    settings.cookie_session,
                )
                .await;
                upload::build(context, settings).await?
            }
        };
        transfer.network = Some(match result {
            Ok(payload) => {
                let network = network::Network {
                    settings: settings.clone(),
                    handle: transfer.handle.clone(),
                    payload,
                    jar: transfer.cookies.clone(),
                    cookie_engine,
                    dns,
                };
                Box::pin(network.run(sender, started))
            }
            Err(error) => Box::pin(async move {
                let _ = sender.send(Event::Done(Box::default(), Some(error))).await;
            }),
        });
        Ok(transfer)
    }

    pub(crate) fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<()> {
        if self.buffered.is_some() {
            return Poll::Ready(());
        }
        if let Poll::Ready(event) = self.receiver.poll_recv(cx) {
            self.buffered = event;
            return Poll::Ready(());
        }
        if let Some(network) = &mut self.network
            && Pin::new(network).poll(cx).is_ready()
        {
            self.network = None;
        }
        match self.receiver.poll_recv(cx) {
            Poll::Ready(event) => {
                self.buffered = event;
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }

    pub(crate) async fn step(
        &mut self,
        context: &mut dyn Context,
    ) -> Result<Poll<()>, NativeError> {
        poll_fn(|cx| self.poll_ready(cx)).await;
        let outcome = match self.buffered.take() {
            Some(Event::Head(info, lines, ready)) => {
                self.handle.borrow_mut().info = *info;
                let outcome = output::head(self, context, lines).await?;
                let _ = ready.send(());
                outcome
            }
            Some(Event::Body(data, size)) => {
                self.handle.borrow_mut().info.downloaded = size;
                output::body(self, context, &data).await?
            }
            Some(Event::Trace(line)) => {
                output::trace(context, &self.settings, &line).await?;
                Ok(())
            }
            Some(Event::Upload(request)) => {
                {
                    let mut easy = self.handle.borrow_mut();
                    easy.info.uploaded = request.sent;
                    easy.info.upload_length = request.length;
                }
                if let Err(error) = output::progress(self, context).await? {
                    Err(error)
                } else {
                    upload::respond(context, self, request).await?;
                    Ok(())
                }
            }
            Some(Event::Done(info, error)) => {
                self.handle.borrow_mut().info = *info;
                if let Some(file) = &self.settings.cookie_jar {
                    cookies::save(context, &self.cookies, file).await;
                }
                self.finish(error);
                return Ok(Poll::Ready(()));
            }
            None => Err(TransferError::new(2, "Transfer stopped without a result")),
        };
        if let Err(error) = outcome {
            self.finish(Some(error));
            return Ok(Poll::Ready(()));
        }
        Ok(Poll::Pending)
    }

    fn finish(&mut self, error: Option<TransferError>) {
        self.network = None;
        let mut easy = self.handle.borrow_mut();
        easy.info.timing.total = self.started.elapsed();
        easy.error = error;
        easy.content = matches!(easy.settings.output, Output::Return)
            .then(|| std::mem::take(&mut self.content));
        easy.active = false;
    }

    pub(crate) fn result(&self) -> Value {
        let easy = self.handle.borrow();
        if easy.error.is_some() {
            Value::Bool(false)
        } else {
            easy.content.clone().map_or(Value::Bool(true), Value::from)
        }
    }
}

impl Drop for Transfer {
    fn drop(&mut self) {
        self.handle.borrow_mut().active = false;
    }
}
