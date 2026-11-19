# Decisions

## 2026-11-19 · Terminate abandoned web children

**Decision.** Give each web child a process group and terminate the child and
its group when the request ends, while Phera still tracks the group leader.
This applies to ordinary cleanup and cancellation. A program can explicitly
detach from its group.

**Why.** Phera reuses its worker threads across requests. Abandoned children
can retain descriptors and consume resources after the response or deadline.

**Rejected.** PHP-FPM leaves ordinary children alive after a response. Phera
also considered waiting for an abandoned child, which would extend request
cleanup for the lifetime of the program.

## 2026-11-19 · Make web program execution configurable

**Decision.** Keep program execution enabled by default, as in PHP.
`phera serve --no-process` makes all program launch entry points throw
`Error`, including `shell_exec()` and backticks.

**Why.** Frameworks and tools expect process execution to be available.
Deployments that never start external programs can disable it in one place.

**Rejected.** Disabling execution by default would require configuration for
existing applications. Disabling functions individually could leave a second
launch entry point available.

## 2026-11-19 · Keep fork out of the Tokio runtime

**Decision.** `pcntl_fork()` throws `Phera\NotSupportedException` in the CLI.
Use `proc_open()` to start a separate program.

**Why.** Forking a multithreaded process leaves the child with one thread.
The child would inherit Tokio and allocator state whose locks may be held by
threads that no longer exist.

**Rejected.** Calling Unix `fork()` and continuing the PHP interpreter in the
child would make those inherited locks and runtime state unsafe to use.
