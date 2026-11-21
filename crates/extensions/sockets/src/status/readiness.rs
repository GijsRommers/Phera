//! `socket_select()` parks on socket readiness or its timeout.

use std::{
    collections::HashSet,
    future::poll_fn,
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd},
    task::Poll,
    time::Duration,
};

use nix::{
    libc,
    sys::{
        select::{FD_SETSIZE, FdSet, select},
        time::TimeVal,
    },
};
use tokio::io::{Interest, unix::AsyncFd};

use super::Candidate;

type Descriptor<'a> = (usize, BorrowedFd<'a>);

pub(super) async fn wait(
    candidates: &[Candidate],
    timeout: Option<Duration>,
) -> io::Result<Vec<usize>> {
    let descriptors: Vec<_> = candidates
        .iter()
        .map(|candidate| (candidate.group, candidate.socket.as_fd()))
        .collect();
    let mut immediate = TimeVal::new(0, 0);
    let ready = select_ready(&descriptors, None, Some(&mut immediate))?;
    if !ready.is_empty() || timeout == Some(Duration::ZERO) {
        return Ok(ready);
    }

    let readiness = async {
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        if candidates.iter().any(|candidate| candidate.group == 2) {
            return wait_priority(candidates).await;
        }
        wait_reactor(&descriptors).await
    };
    match timeout {
        Some(timeout) => tokio::time::timeout(timeout, readiness)
            .await
            .unwrap_or_else(|_| Ok(Vec::new())),
        None => readiness.await,
    }
}

fn select_ready(
    descriptors: &[Descriptor<'_>],
    wake: Option<BorrowedFd<'_>>,
    timeout: Option<&mut TimeVal>,
) -> io::Result<Vec<usize>> {
    let mut sets: [FdSet<'_>; 3] = std::array::from_fn(|_| FdSet::new());
    for &(group, fd) in descriptors.iter().chain(wake.map(|fd| (0, fd)).iter()) {
        if fd.as_raw_fd() as usize >= FD_SETSIZE {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        sets[group].insert(fd);
    }
    let [read, write, except] = &mut sets;
    select(None, read, write, except, timeout)?;
    Ok(descriptors
        .iter()
        .enumerate()
        .filter_map(|(index, &(group, fd))| sets[group].contains(fd).then_some(index))
        .collect())
}

struct Watch {
    fd: AsyncFd<OwnedFd>,
    interest: Interest,
}

impl Watch {
    fn new(group: usize, fd: BorrowedFd<'_>) -> io::Result<Self> {
        let interest = match group {
            0 => Interest::READABLE,
            1 => Interest::WRITABLE,
            #[cfg(any(target_os = "linux", target_os = "android"))]
            _ => Interest::PRIORITY,
            #[cfg(not(any(target_os = "linux", target_os = "android")))]
            _ => unreachable!("exception sets are handled by wait_priority"),
        };
        // This owned duplicate keeps the same open descriptor for the life of the registration.
        #[allow(deprecated)]
        let fd = AsyncFd::with_interest(fd.try_clone_to_owned()?, interest)?;
        Ok(Self { fd, interest })
    }

    async fn wake(&self) -> io::Result<bool> {
        let mut guard = self.fd.ready(self.interest).await?;
        let events = guard.ready();
        guard.clear_ready();
        Ok(if self.interest == Interest::WRITABLE {
            events.is_write_closed()
        } else {
            events.is_read_closed()
        })
    }
}

async fn wait_reactor(descriptors: &[Descriptor<'_>]) -> io::Result<Vec<usize>> {
    let mut seen = HashSet::new();
    let mut watches: Vec<_> = descriptors
        .iter()
        .filter(|&&(group, fd)| seen.insert((group, fd.as_raw_fd())))
        .map(|&(group, fd)| Watch::new(group, fd))
        .collect::<io::Result<_>>()?;
    loop {
        let (index, closed) = {
            let mut waits: Vec<_> = watches.iter().map(|watch| Box::pin(watch.wake())).collect();
            poll_fn(|cx| {
                use std::future::Future;
                for (index, wait) in waits.iter_mut().enumerate() {
                    if let Poll::Ready(result) = wait.as_mut().poll(cx) {
                        return Poll::Ready(result.map(|closed| (index, closed)));
                    }
                }
                Poll::Pending
            })
            .await?
        };
        let mut immediate = TimeVal::new(0, 0);
        let ready = select_ready(descriptors, None, Some(&mut immediate))?;
        if !ready.is_empty() {
            return Ok(ready);
        }
        // The reactor reports closure permanently even when select excludes it from this set.
        if closed {
            watches.swap_remove(index);
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
async fn wait_priority(candidates: &[Candidate]) -> io::Result<Vec<usize>> {
    use std::os::unix::net::UnixStream;

    // Tokio only exposes priority readiness on epoll. Bound the number of blocking selects
    // and let dropping the caller's end of the socket pair cancel them.
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);
    let permit = SLOTS.acquire().await.map_err(io::Error::other)?;
    let (cancel, wake) = UnixStream::pair()?;
    let sockets: Vec<_> = candidates
        .iter()
        .map(|candidate| (candidate.group, candidate.socket.clone()))
        .collect();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let descriptors: Vec<_> = sockets
            .iter()
            .map(|(group, socket)| (*group, socket.as_fd()))
            .collect();
        select_ready(&descriptors, Some(wake.as_fd()), None)
    })
    .await;
    drop(cancel);
    result.map_err(io::Error::other)?
}
