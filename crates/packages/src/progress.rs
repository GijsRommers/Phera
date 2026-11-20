//! Show package preparation and placement counts during a locked install.

use std::io::{self, IsTerminal};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct Progress {
    verb: &'static str,
    total: usize,
    terminal: bool,
    completed: AtomicUsize,
}

impl Progress {
    pub(crate) fn start(verb: &'static str, total: usize, verbose: bool) -> Self {
        let terminal = total != 0 && !verbose && io::stderr().is_terminal();
        if terminal {
            eprint!("\r{verb} 0/{total} packages");
        } else if total != 0 {
            eprintln!("{verb} {total} packages");
        }
        Self {
            verb,
            total,
            terminal,
            completed: AtomicUsize::new(0),
        }
    }

    pub(crate) fn complete(&self) {
        if self.terminal {
            let done = self.completed.fetch_add(1, Ordering::Relaxed) + 1;
            if done.is_multiple_of(8) {
                eprint!("\r{} {done}/{} packages", self.verb, self.total);
            }
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        if self.terminal {
            let done = self.completed.load(Ordering::Relaxed);
            eprintln!("\r{} {done}/{} packages", self.verb, self.total);
        }
    }
}
