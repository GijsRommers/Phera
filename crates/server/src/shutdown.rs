//! SIGINT and SIGTERM handling for [`Server::run`](crate::Server::run).

use std::thread::JoinHandle;

use tokio::sync::watch;

/// Watch for SIGINT and SIGTERM on a dedicated thread and trigger shutdown.
pub(super) fn spawn_signal_watcher(shutdown: watch::Sender<bool>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("phera-signals".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => return,
            };
            runtime.block_on(wait_for_signal());
            let _ = shutdown.send(true);
        })
        .expect("failed to spawn the signal watcher thread")
}

async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut interrupt = match signal(SignalKind::interrupt()) {
            Ok(signal) => signal,
            Err(_) => return,
        };
        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(_) => return,
        };
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
