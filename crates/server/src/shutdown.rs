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
            shutdown.send_replace(true);
        })
        .expect("failed to spawn the signal watcher thread")
}

/// Like [`spawn_signal_watcher`], for a sender that others hold too. The
/// thread also ends when `shutdown` turns true for another reason (the
/// server was stopped from PHP). The signal handlers stay installed, as
/// Tokio leaves them: a later SIGINT is then caught and ignored.
pub(super) fn spawn_shared_signal_watcher(shutdown: std::sync::Arc<watch::Sender<bool>>) {
    // Subscribed here, before the thread starts, so that a stop that comes
    // first is not missed.
    let mut stopped = shutdown.subscribe();
    let spawned = std::thread::Builder::new()
        .name("phera-signals".to_string())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async {
                tokio::select! {
                    _ = wait_for_signal() => {
                        shutdown.send_replace(true);
                    }
                    _ = stopped.wait_for(|&stop| stop) => {}
                }
            });
        });
    if let Err(error) = spawned {
        eprintln!("phera-server: no signal handling: {error}");
    }
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
