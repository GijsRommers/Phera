# `stream_get_transports()` lists only the transports Phera can open

- Date: 2026-11-21
- Area: Scope and compatibility
- Decided by: Design

## Decision

`stream_get_transports()` returns `tcp`, `udp`, `unix`, `udg`, `ssl`, `tls`,
`tlsv1.2` and `tlsv1.3`. The names `sslv3`, `tlsv1.0` and `tlsv1.1` are not
transports in Phera: opening one gives the message PHP gives for a transport
that is not built in ("Unable to find the socket transport ... did you
forget to enable it when you configured PHP?").

## Why

PHP's list is a property of its OpenSSL build, not of PHP: Homebrew's PHP
8.5 lists eleven names with `sslv3`, the PHP 8.5 on the Linux runner lists
ten without it. Phera's TLS speaks 1.2 and 1.3 only. The first version copied
the eleven names of the Mac, so on Linux the list differed from PHP's, and it
named three protocols that could never connect.

A program that asks `in_array('tls', stream_get_transports())` gets the
answer it gets from PHP; one that asks for `tlsv1.0` learns that it is not
available, which is true.

## Turned down

- Copying the list of whichever PHP is installed: the answer would change
  with the machine and promise protocols that fail at the handshake.
- Negotiating TLS 1.2 under the name `tlsv1.0`: a program that names a
  version gets another one without being told.
