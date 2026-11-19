//! `curl_exec` and `curl_multi_exec` share the response and callback path.

mod download;
mod network;
mod output;
mod request;
mod upload;

use std::{
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::{Context as TaskContext, Poll},
    time::Instant,
};

use bytes::Bytes;
use phera_extension_api::{Context, LocalFuture, NativeError, Object, Value};
use tokio::sync::mpsc::{self, Receiver};

use crate::{
    cookies::{self, CookieJar},
    easy::Handle,
    error::TransferError,
    info::Info,
    options::{Output, Settings},
};

type Sender = mpsc::Sender<Event>;

enum Event {
    Head(Box<Info>, Vec<Vec<u8>>),
    Body(Bytes, i64),
    Trace(String),
    Done(Box<Info>, Option<TransferError>),
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
            (
                easy.settings.clone(),
                easy.cookies(),
                easy.cookie_engine(),
                dns,
            )
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
        cookies::load(
            context,
            &transfer.cookies,
            &settings.cookie_files,
            settings.cookie_session,
        )
        .await;
        let result = upload::build(context, &transfer.object, settings).await?;
        transfer.network = Some(match result {
            Ok(payload) => {
                let network = network::Network {
                    settings: settings.clone(),
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
            Some(Event::Head(info, lines)) => {
                self.handle.borrow_mut().info = *info;
                output::head(self, context, lines).await?
            }
            Some(Event::Body(data, size)) => {
                self.handle.borrow_mut().info.downloaded = size;
                output::body(self, context, &data).await?
            }
            Some(Event::Trace(line)) => {
                output::trace(context, &self.settings, &line).await?;
                Ok(())
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
        easy.content = matches!(self.settings.output, Output::Return)
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
