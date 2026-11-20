# Keep buffered HTTP request and response conversion direct

- Date: 2026-11-20
- Area: Server and requests
- Decided by: Design

## Decision

Use Hyper's end-of-stream signal to skip the body reader when the request
head already completes the message. Build static-file conditions only
after resolving a static file. Move the existing response headers into
the wire response. Allocate storage for the early response at handoff.

## Why

A buffered response should avoid the setup work for an incoming body,
file serving, and early output when it uses none of those paths. HTTP/1
and HTTP/2 expose the same end-of-stream property. Requests with a body
continue through the timed receiver.

## Turned down

Using the HTTP method to guess whether a body arrived. The received frame
determines whether the body needs reading.
