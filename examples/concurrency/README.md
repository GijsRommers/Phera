# Many requests at once

A PHP-FPM worker process serves one request at a time. While that request
waits for a database or another service, the process does nothing, so serving
1,000 waiting requests at once takes 1,000 processes.

Phera runs each request as a task. A request that waits gives its thread to
another request, and one thread serves thousands of them.

## The scripts

| Script | What it does |
| --- | --- |
| `wait.php` | Waits 100 ms (`usleep`), standing in for one slow query. |
| `page.php` | Three "queries" of 20 ms each, plus sorting and HTML output. |
| `cpu.php` | A loop of 3 million rounds, no waiting. |
| `hello.php` | One line. |

## Try it

```sh
cargo build --release -p phera-cli
target/release/phera serve --workers 1 examples/concurrency   # then open http://127.0.0.1:8080/wait.php
examples/concurrency/loadtest.sh                               # the comparison below
```

## Measured

17 November 2026, Apple M-series with 10 cores, PHP 8.5.12. PHP's built-in
server with `PHP_CLI_SERVER_WORKERS` stands in for PHP-FPM: a fixed number of
processes, each serving one request at a time.

| Scenario | Phera, 1 thread | Phera, 10 threads | PHP, 1 process | PHP, 10 processes |
| --- | --- | --- | --- | --- |
| `wait.php`, 1,000 at once | 3,942 req/s | 5,224 req/s | 9.5 req/s (50 at once) | did not finish (250 at once) |
| `page.php`, 500 at once | 835 req/s | 1,248 req/s | 13.9 req/s (25 at once) | 165 req/s (125 at once) |
| `cpu.php`, 50 at once | 27 req/s | 59 req/s | 101 req/s | 275 req/s |
| `hello.php`, 100 at once | 11,862 req/s | 16,698 req/s | 31,421 req/s | 12,861 req/s |

What the numbers say:

- **Waiting requests:** one Phera thread answers 2,000 requests that each
  wait 100 ms in half a second. A PHP process answers ten per second.
- **Computation:** PHP is about four times faster per core. Phera's
  interpreter is slower than PHP's; see docs/architecture.md section 10.
- **Fairness:** a long computation does not freeze its thread. The
  interpreter lets other requests run every few thousand loop rounds.

Two tests in `crates/cli/tests/serve.rs` keep this true:
`waiting_requests_do_not_block_each_other` and
`a_long_computation_lets_other_requests_through`.
