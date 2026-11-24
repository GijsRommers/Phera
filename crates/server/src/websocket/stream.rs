//! Upgraded WebSocket connection loop shared by the HTTP/1.1 upgrade and the
//! HTTP/2 extended CONNECT stream.
//!
//! The loop only needs bytes in and out (`AsyncRead + AsyncWrite`): an
//! upgraded TCP socket, or one HTTP/2 stream whose DATA frames hyper turns
//! into the same byte stream. Back pressure comes from the transport: a full
//! socket buffer, or an exhausted HTTP/2 stream window.

use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Notify, OwnedSemaphorePermit, mpsc, oneshot, watch};
use tokio::time::Instant;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Utf8Bytes;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{
    CloseFrame, Message as WsMessage, Role, WebSocketConfig as TungsteniteConfig,
};

use crate::handler::Handler;
use crate::worker::ConnectionSlot;

use super::hub::{OutboundCommand, RegisteredConnection, WebSocketHub};
use super::{WebSocketConfig, WebSocketEvent, WebSocketEventKind, WebSocketUpgrade};

pub(crate) struct UpgradedContext {
    pub(crate) upgrade: WebSocketUpgrade,
    pub(crate) handler: Rc<dyn Handler>,
    pub(crate) hub: Arc<WebSocketHub>,
    pub(crate) config: WebSocketConfig,
    pub(crate) shutdown: watch::Receiver<bool>,
    /// An HTTP/1.1 upgrade takes the socket over, and with it the connection's
    /// slot. HTTP/2 WebSocket streams share their connection, which keeps the
    /// slot itself (`None`).
    pub(crate) tcp_slot: Option<ConnectionSlot>,
    pub(crate) ws_slot: OwnedSemaphorePermit,
}

struct IoChannels<'a> {
    rx_outbound: &'a mut mpsc::Receiver<OutboundCommand>,
    queued_bytes: &'a std::sync::atomic::AtomicUsize,
    disconnect: &'a Notify,
    close_notify: &'a Notify,
    pending_close: &'a Mutex<Option<(u16, String)>>,
    tx_events: &'a mpsc::Sender<WebSocketEventKind>,
    fail_rx: &'a mut mpsc::Receiver<()>,
    shutdown: &'a mut watch::Receiver<bool>,
    config: &'a WebSocketConfig,
}

/// Drive one upgraded WebSocket connection to completion.
///
/// Events for this connection execute strictly sequentially in arrival order
/// on a dedicated local task, while multiple connections run concurrently on
/// the worker thread. Ping/Pong frames are answered in Rust without invoking
/// PHP.
pub(crate) async fn run_upgraded_websocket<S>(io: S, mut ctx: UpgradedContext)
where
    S: AsyncRead + AsyncWrite + Unpin + 'static,
{
    let ws_config = TungsteniteConfig::default()
        .read_buffer_size(8 * 1024)
        .write_buffer_size(0)
        .max_write_buffer_size(ctx.config.send_queue_bytes)
        .max_message_size(Some(ctx.config.max_message_bytes))
        .max_frame_size(Some(ctx.config.max_message_bytes));
    let mut ws_stream = WebSocketStream::from_raw_socket(io, Role::Server, Some(ws_config)).await;

    let RegisteredConnection {
        id: conn_id,
        id_str: conn_id_str,
        mut rx_outbound,
        queued_bytes,
        disconnect,
        close_notify,
        pending_close,
    } = ctx.hub.register(ctx.upgrade.context, ctx.upgrade.rooms);
    let (tx_events, rx_events) = mpsc::channel(ctx.config.send_queue_len.max(1));
    let (fail_tx, mut fail_rx) = mpsc::channel::<()>(1);
    let (close_tx, close_rx) = oneshot::channel();

    // Queue Open before the reader loop pulls any incoming client frame.
    let _ = tx_events.try_send(WebSocketEventKind::Open);

    let event_task = tokio::task::spawn_local({
        let handler = ctx.handler.clone();
        let connection_id: Arc<str> = Arc::from(conn_id_str.as_str());
        let entry: Arc<str> = Arc::from(ctx.upgrade.entry.as_str());
        let handler_class: Arc<str> = Arc::from(ctx.upgrade.handler_class.as_str());
        async move {
            run_events(
                rx_events,
                close_rx,
                handler,
                connection_id,
                entry,
                handler_class,
                fail_tx,
            )
            .await;
        }
    });

    let mut channels = IoChannels {
        rx_outbound: &mut rx_outbound,
        queued_bytes: &queued_bytes,
        disconnect: &disconnect,
        close_notify: &close_notify,
        pending_close: &pending_close,
        tx_events: &tx_events,
        fail_rx: &mut fail_rx,
        shutdown: &mut ctx.shutdown,
        config: &ctx.config,
    };
    let close_reason = drive_io(&mut ws_stream, &mut channels).await;
    // End the transport before `onClose` runs in PHP: on HTTP/1.1 this
    // sends FIN, on HTTP/2 it ends the stream (END_STREAM) while the
    // connection and its other streams carry on (RFC 8441 section 5).
    let _ = tokio::time::timeout(ctx.config.write_timeout, ws_stream.get_mut().shutdown()).await;
    drop(ws_stream);

    if let Some((code, reason)) = close_reason {
        let _ = close_tx.send((code, reason));
    }
    drop(tx_events);

    let _ = event_task.await;
    ctx.hub.remove(conn_id);
    drop(ctx.ws_slot);
    drop(ctx.tcp_slot);
}

async fn drive_io<S: AsyncRead + AsyncWrite + Unpin>(
    ws_stream: &mut WebSocketStream<S>,
    channels: &mut IoChannels<'_>,
) -> Option<(u16, String)> {
    let mut last_activity = Instant::now();
    let ping_duration = if channels.config.ping_interval.is_zero() {
        Duration::from_secs(86_400)
    } else {
        channels.config.ping_interval
    };
    let mut ping_ticker = tokio::time::interval_at(Instant::now() + ping_duration, ping_duration);
    ping_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;
            _ = wait_for_shutdown(channels.shutdown) => {
                send_close(ws_stream, 1001, "server shutting down", Duration::from_secs(2)).await;
                return Some((1001, "server shutting down".to_string()));
            }
            _ = channels.fail_rx.recv() => {
                send_close(ws_stream, 1011, "internal server error", channels.config.write_timeout).await;
                return None;
            }
            _ = channels.disconnect.notified() => {
                send_close(ws_stream, 1008, "slow consumer", Duration::from_secs(1)).await;
                return Some((1008, "slow consumer".to_string()));
            }
            _ = channels.close_notify.notified() => {
                if let Some((code, reason)) = channels
                    .pending_close
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.take())
                {
                    send_close(ws_stream, code, &reason, channels.config.write_timeout).await;
                    return Some((code, reason));
                }
            }
            Some(cmd) = channels.rx_outbound.recv() => {
                match cmd {
                    OutboundCommand::Send { data, binary } => {
                        channels.queued_bytes.fetch_sub(data.len(), Ordering::AcqRel);
                        let msg = if binary {
                            WsMessage::Binary(data)
                        } else {
                            let Ok(text) = Utf8Bytes::try_from(data) else {
                                send_close(
                                    ws_stream,
                                    1011,
                                    "invalid outbound UTF-8",
                                    channels.config.write_timeout,
                                )
                                .await;
                                return Some((1011, "invalid outbound UTF-8".to_string()));
                            };
                            WsMessage::Text(text)
                        };
                        if write_failed(
                            tokio::time::timeout(channels.config.write_timeout, ws_stream.send(msg))
                                .await,
                        ) {
                            return Some((1006, String::new()));
                        }
                    }
                    OutboundCommand::Close { code, reason } => {
                        send_close(ws_stream, code, &reason, channels.config.write_timeout).await;
                        return Some((code, reason));
                    }
                }
            }
            _ = ping_ticker.tick() => {
                if !channels.config.idle_timeout.is_zero()
                    && last_activity.elapsed() >= channels.config.idle_timeout
                {
                    send_close(ws_stream, 1001, "idle timeout", channels.config.write_timeout).await;
                    return Some((1001, "idle timeout".to_string()));
                }
                if !channels.config.ping_interval.is_zero()
                    && write_failed(
                        tokio::time::timeout(
                            channels.config.write_timeout,
                            ws_stream.send(WsMessage::Ping(Bytes::new())),
                        )
                        .await,
                    )
                {
                    return Some((1006, String::new()));
                }
            }
            incoming = ws_stream.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        last_activity = Instant::now();
                        let event = WebSocketEventKind::Message {
                            data: text.as_bytes().to_vec(),
                            binary: false,
                        };
                        if !enqueue_event(channels.tx_events, event, channels.shutdown, channels.disconnect).await {
                            return Some((1001, "server shutting down".to_string()));
                        }
                    }
                    Some(Ok(WsMessage::Binary(data))) => {
                        last_activity = Instant::now();
                        let event = WebSocketEventKind::Message {
                            data: data.to_vec(),
                            binary: true,
                        };
                        if !enqueue_event(channels.tx_events, event, channels.shutdown, channels.disconnect).await {
                            return Some((1001, "server shutting down".to_string()));
                        }
                    }
                    Some(Ok(WsMessage::Ping(_))) => {
                        last_activity = Instant::now();
                        let _ = tokio::time::timeout(channels.config.write_timeout, ws_stream.flush()).await;
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        last_activity = Instant::now();
                    }
                    Some(Ok(WsMessage::Close(frame))) => {
                        let (code, reason) = match frame {
                            Some(frame) => (u16::from(frame.code), frame.reason.to_string()),
                            None => (1005, String::new()),
                        };
                        let _ = tokio::time::timeout(channels.config.write_timeout, ws_stream.flush()).await;
                        return Some((code, reason));
                    }
                    Some(Ok(WsMessage::Frame(_))) => {}
                    Some(Err(tokio_tungstenite::tungstenite::Error::Capacity(_))) => {
                        send_close(
                            ws_stream,
                            1009,
                            "message too big",
                            channels.config.write_timeout,
                        )
                        .await;
                        return Some((1009, "message too big".to_string()));
                    }
                    Some(Err(tokio_tungstenite::tungstenite::Error::Utf8(_))) => {
                        send_close(
                            ws_stream,
                            1007,
                            "invalid UTF-8",
                            channels.config.write_timeout,
                        )
                        .await;
                        return Some((1007, "invalid UTF-8".to_string()));
                    }
                    Some(Err(tokio_tungstenite::tungstenite::Error::Protocol(_))) => {
                        send_close(
                            ws_stream,
                            1002,
                            "protocol error",
                            channels.config.write_timeout,
                        )
                        .await;
                        return Some((1002, "protocol error".to_string()));
                    }
                    Some(Err(_)) | None => return Some((1006, String::new())),
                }
            }
        }
    }
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    let _ = shutdown.changed().await;
}

async fn enqueue_event(
    tx_events: &mpsc::Sender<WebSocketEventKind>,
    event: WebSocketEventKind,
    shutdown: &mut watch::Receiver<bool>,
    disconnect: &Notify,
) -> bool {
    tokio::select! {
        biased;
        _ = wait_for_shutdown(shutdown) => false,
        _ = disconnect.notified() => false,
        res = tx_events.send(event) => res.is_ok(),
    }
}

async fn run_events(
    mut rx_events: mpsc::Receiver<WebSocketEventKind>,
    close_rx: oneshot::Receiver<(u16, String)>,
    handler: Rc<dyn Handler>,
    connection_id: Arc<str>,
    entry: Arc<str>,
    handler_class: Arc<str>,
    fail_tx: mpsc::Sender<()>,
) {
    let mut terminated_1011 = false;
    while let Some(kind) = rx_events.recv().await {
        if terminated_1011 {
            continue;
        }
        let event = WebSocketEvent {
            connection_id: connection_id.clone(),
            entry: entry.clone(),
            handler_class: handler_class.clone(),
            kind,
        };
        let outcome = handler.handle_websocket(event).await;
        if outcome.close_1011 {
            terminated_1011 = true;
            let _ = fail_tx.try_send(());
        }
    }
    if !terminated_1011 && let Ok((code, reason)) = close_rx.await {
        let _ = handler
            .handle_websocket(WebSocketEvent {
                connection_id,
                entry,
                handler_class,
                kind: WebSocketEventKind::Close { code, reason },
            })
            .await;
    }
}

async fn send_close<S: AsyncRead + AsyncWrite + Unpin>(
    ws_stream: &mut WebSocketStream<S>,
    code: u16,
    reason: &str,
    timeout: Duration,
) {
    let frame = CloseFrame {
        code: CloseCode::from(code),
        reason: Utf8Bytes::from(reason.to_string()),
    };
    let _ = tokio::time::timeout(timeout, ws_stream.close(Some(frame))).await;
}

fn write_failed<E>(result: Result<Result<(), E>, tokio::time::error::Elapsed>) -> bool {
    !matches!(result, Ok(Ok(())))
}
