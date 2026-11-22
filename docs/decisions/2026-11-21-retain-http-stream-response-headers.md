# Retain HTTP stream response headers

- Date: 2026-11-21
- Area: Streams
- Decided by: Design

## Decision

Keep the spelling and order of the response headers from the wire, including
duplicates and the server's status text. Hyper still validates the response
and decodes the body. The stream wrapper stores the resulting array for the
current request, including for HTTP error responses. A new request starts
with no previous headers.

`get_headers()` stops after the final response headers. Follow HTTP and
HTTPS redirects, up to twenty requests by default. A redirect to a different
origin gets a new Host header and drops explicit authorization and cookies.
The wrapper caps the redirect limit at one hundred and the captured header
bytes at 128 KiB per response. Body reads retain the existing 16 MiB limit.

## Why

PHP exposes the raw response headers, and uses their original spelling as
keys in the associative form of `get_headers()`. Many callers also need the
headers from error responses or redirects without downloading the body.
