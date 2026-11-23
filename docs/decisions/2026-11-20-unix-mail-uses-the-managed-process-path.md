# Unix mail uses the managed process path

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

`mail()` and `mb_send_mail()` deliver through the existing Unix process
policy and wait asynchronously for sendmail. Mail parameters use the same
escaping as `escapeshellcmd()`. The Unix delivery path leaves sender
selection to sendmail and the supplied headers and parameters.

## Why

PHP uses a pipe to the configured command. The existing process support
applies request working directories, limits, cancellation and
`--no-process` consistently.

## Turned down

Direct SMTP delivery and a second process launcher.
