//! A VM owns one outgoing DBGp connection. Waiting for IDE commands suspends only that VM.

mod breakpoints;
mod closure;
mod config;
mod contexts;
mod evaluate;
mod exceptions;
mod execution;
mod features;
mod inspection;
pub(crate) mod php_api;
mod properties;
mod property_xml;
mod protocol;
mod session;
mod source;
mod stack;
mod streams;
mod time_limit;

pub use config::{DebugConfig, Start};
pub use time_limit::DebugTimeLimit;

use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};
use tokio::net::TcpStream;

use crate::Vm;

use protocol::{Wire, file_uri};
pub(crate) use session::Session;
pub(crate) use streams::{Stream, Streams};

impl Vm {
    /// Request triggers use the same parsed values PHP exposes to the script.
    pub fn debugger_trigger(&self, name: &str) -> Option<String> {
        ["_GET", "_POST", "_COOKIE"].into_iter().find_map(|scope| {
            let value = self.request_global_value(scope)?.deref();
            let Value::Array(values) = &*value else {
                return None;
            };
            let value = values.get(&ArrayKey::str(name))?.deref();
            let Value::String(value) = &*value else {
                return None;
            };
            Some(String::from_utf8_lossy(value.as_bytes()).into_owned())
        })
    }

    /// An unavailable IDE does not change the PHP request.
    #[cold]
    pub async fn connect_debugger(
        &mut self,
        config: &DebugConfig,
        file: &str,
        idekey: &str,
        time_limit: Option<DebugTimeLimit>,
    ) {
        let connection = tokio::time::timeout(
            config.timeout,
            TcpStream::connect((config.host.as_str(), config.port)),
        )
        .await;
        if let Ok(Ok(stream)) = connection {
            let mut session = Session::new(Wire::new(stream), file_uri(file), time_limit);
            if session.init(idekey).await.is_ok() {
                self.output_writer.debugger = Some(session.streams.clone());
                self.debugger = Some(Box::new(session));
                return;
            }
        }
        let message = format!(
            "Phera debugger: could not connect to the IDE at {}:{}\n",
            config.host, config.port,
        );
        if let Some(path) = &config.log
            && let Ok(mut log) = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await
        {
            use tokio::io::AsyncWriteExt;
            if log.write_all(message.as_bytes()).await.is_ok() {
                return;
            }
        }
        eprint!("{message}");
    }

    #[cold]
    pub(crate) async fn finish_debugger(&mut self) {
        if let Some(mut session) = self.debugger.take() {
            let _ = session.finish().await;
        }
    }

    pub(crate) fn debugger_exit() -> NativeError {
        NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            crate::request_end::Exit,
        )))
    }
}
