# Project decisions

## 2026-11-19 · PHP curl transport

### Decision

Phera exposes PHP's curl API for HTTP and HTTPS. Hyper implements HTTP/1.1
and HTTP/2, Tokio drives asynchronous I/O, and rustls with ring handles TLS.
Each worker owns bounded connection pools shared by compatible transfers.

### Why

Guzzle, Symfony HttpClient, and other PHP clients use curl's easy and multi
interfaces. The Rust HTTP stack lets requests wait for sockets, timers, and
PHP callbacks on the existing worker scheduler.

### Alternatives considered

Binding to libcurl was rejected because it would introduce a second network
stack and require a separate bridge for its event loop and PHP callbacks.
Enabling FTP, file, and other non-HTTP schemes was rejected to keep the
extension focused on HTTP clients and reduce the request-forgery surface.
